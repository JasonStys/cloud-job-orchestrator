//! File: Owns identified native profile files and checks the closed PostgreSQL tree before mutation.
//! Functions: `open` retains the OS lock; bounded reads/metadata checks refuse aliases and program roots.
//! Variables: credentials are generated per profile, never shared in source or returned to the renderer.

use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

const PRODUCT: &str = "cloud-job-orchestrator";
const MAX_MEMBERS: usize = 10_000;
const MAX_PROFILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MEMBER_BYTES: u64 = 1024 * 1024 * 1024;
#[cfg(windows)]
const REPARSE_POINT: u32 = 0x400;

/// Private profile identity; deliberately has no Debug implementation that could log credentials.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub schema: u8,
    pub product: String,
    pub cluster: String,
    pub password: String,
}

/// One durable plaintext OS-owner profile, protected by an actual handle lock until cleanup completes.
pub struct Profile {
    pub directory: PathBuf,
    pub identity: Identity,
    _owner: File,
}

/// Include the complete shipped shell tree when the service occupies its fixed packaged layout.
pub fn program_roots(program: &Path, assets: &Path, postgres: &Path) -> io::Result<Vec<PathBuf>> {
    let mut roots = vec![program.to_owned(), postgres.to_owned()];
    match fs::canonicalize(assets.join("bin")) {
        Ok(binary_root) if fs::canonicalize(program)? == binary_root => {
            let installation = assets
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| invalid_data("Packaged native installation root is unavailable"))?;
            roots.push(installation.to_owned());
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(roots)
}

impl Profile {
    /// Admit only empty or identified state; never reset corrupt data or use a program directory.
    pub fn open(directory: &Path, forbidden: &[PathBuf]) -> io::Result<Self> {
        admit_directory(directory, forbidden)?;
        admit_mutable(&directory.join("postgres-startup.log"))?;
        fs::create_dir_all(directory)?;
        let nonempty = fs::read_dir(directory)?.next().transpose()?.is_some();
        let marker = directory.join("profile.json");
        let retained = if nonempty {
            Some(parse_identity(&read_regular(&marker, 4096)?)?)
        } else {
            None
        };
        let owner_path = directory.join("owner.lock");
        let owner = file_options()
            .write(true)
            .create_new(regular(&owner_path)?.is_none())
            .open(&owner_path)?;
        require_regular(&owner)?;
        owner
            .try_lock()
            .map_err(|_| io::Error::other("The native workspace already has an owner"))?;
        let identity = retained.map_or_else(|| new_identity(&marker), Ok)?;
        let database = directory.join("postgres");
        match fs::symlink_metadata(&database) {
            Ok(value) if value.is_dir() && !linked(&value) => inspect_tree(&database)?,
            Ok(_) => {
                return Err(invalid_data(
                    "Native database root must be a real directory",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(Self {
            directory: directory.to_owned(),
            identity,
            _owner: owner,
        })
    }
}

/// Produce independent cryptographic random UUID material, not identifiers derived from user data.
pub fn nonce() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn new_identity(marker: &Path) -> io::Result<Identity> {
    let value = Identity {
        schema: 1,
        product: PRODUCT.to_owned(),
        cluster: nonce(),
        password: nonce(),
    };
    write_new(
        marker,
        &serde_json::to_vec_pretty(&value).map_err(invalid_data)?,
    )?;
    Ok(value)
}

fn parse_identity(raw: &[u8]) -> io::Result<Identity> {
    let identity: Identity = serde_json::from_slice(raw).map_err(invalid_data)?;
    if identity.schema != 1
        || identity.product != PRODUCT
        || !hex(&identity.cluster)
        || !hex(&identity.password)
    {
        return Err(invalid_data(
            "Native workspace identity is invalid; no reset attempted",
        ));
    }
    Ok(identity)
}

fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn linked(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn regular(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(value) if value.is_file() && !linked(&value) => Ok(Some(value)),
        Ok(_) => Err(invalid_data(
            "Native state must be an ordinary unlinked file",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(OPEN_REPARSE_POINT);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .mode(0o600);
    }
    options
}

fn require_regular(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || linked(&metadata) {
        return Err(invalid_data(
            "Native state descriptor is not an ordinary file",
        ));
    }
    #[cfg(windows)]
    if winapi_util::file::information(file)?.number_of_links() != 1 {
        return Err(invalid_data("Native mutable files must have one hardlink"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid_data("Native mutable files must have one hardlink"));
        }
    }
    Ok(())
}

/// Refuse pre-existing aliases before a vendor lifecycle program can append to the fixed log.
pub fn admit_mutable(path: &Path) -> io::Result<()> {
    if regular(path)?.is_some() {
        let file = file_options().open(path)?;
        require_regular(&file)?;
        if file.metadata()?.len() > 4 * 1024 * 1024 {
            return Err(invalid_data(
                "Native startup log capacity reached; preserve it for review",
            ));
        }
    }
    Ok(())
}

/// Inspect a bounded regular descriptor without loading a caller-selected unlimited file.
pub fn read_regular(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    regular(path)?.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let mut file = file_options().open(path)?;
    require_regular(&file)?;
    let length = file.metadata()?.len();
    if length > limit {
        return Err(invalid_data("Native file exceeds its byte bound"));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(length.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_err(invalid_data)? != length {
        return Err(invalid_data("Native file changed while reading"));
    }
    Ok(bytes)
}

/// Create only a new flushed native-owned file; no existing object is truncated or removed.
pub fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > 16_384 {
        return Err(invalid_data("Native state write exceeds its bound"));
    }
    let mut output = file_options().write(true).create_new(true).open(path)?;
    require_regular(&output)?;
    output.write_all(bytes)?;
    output.sync_all()
}

/// Refuse every existing alias/non-directory ancestor and program-root destination before mkdir.
pub fn admit_directory(path: &Path, forbidden: &[PathBuf]) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid_data(
            "Use an absolute native profile without parent traversal",
        ));
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(value) if value.is_dir() && !linked(&value) => {}
            Ok(_) => {
                return Err(invalid_data(
                    "Linked native profile ancestors are unsupported",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    let target = canonical_intent(path)?;
    for root in forbidden {
        if target.starts_with(fs::canonicalize(root)?) {
            return Err(invalid_data(
                "Keep durable native data outside the application/resources",
            ));
        }
    }
    Ok(())
}

fn canonical_intent(path: &Path) -> io::Result<PathBuf> {
    let mut existing = path.to_owned();
    let mut missing = Vec::new();
    loop {
        match fs::canonicalize(&existing) {
            Ok(mut root) => {
                for part in missing.into_iter().rev() {
                    root.push(part);
                }
                return Ok(root);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(
                    existing
                        .file_name()
                        .ok_or_else(|| invalid_data("Invalid profile root"))?
                        .to_owned(),
                );
                if !existing.pop() {
                    return Err(invalid_data("No existing profile ancestor"));
                }
            }
            Err(error) => return Err(error),
        }
    }
}

/// Validate only this profile's closed PostgreSQL tree; never follow tablespace/link aliases.
pub fn inspect_tree(root: &Path) -> io::Result<()> {
    admit_directory(root, &[])?;
    let mut directories = vec![root.to_owned()];
    let (mut count, mut total) = (0_usize, 0_u64);
    while let Some(directory) = directories.pop() {
        admit_directory(&directory, &[])?;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            count += 1;
            if count > MAX_MEMBERS {
                return Err(invalid_data("Native database member budget exceeded"));
            }
            let value = fs::symlink_metadata(entry.path())?;
            if linked(&value) {
                return Err(invalid_data(
                    "Linked native database members are unsupported",
                ));
            }
            if value.is_dir() {
                directories.push(entry.path());
            } else if value.is_file() {
                let file = file_options().open(entry.path())?;
                require_regular(&file)?;
                let bytes = file.metadata()?.len();
                total = total
                    .checked_add(bytes)
                    .ok_or_else(|| invalid_data("Native database byte count overflow"))?;
                if bytes > MAX_MEMBER_BYTES || total > MAX_PROFILE_BYTES {
                    return Err(invalid_data(
                        "Native database byte capacity reached; preserve the closed profile",
                    ));
                }
            } else {
                return Err(invalid_data(
                    "Special native database members are unsupported",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::inspect_tree;
    use super::{Profile, admit_directory, nonce, program_roots, read_regular};
    use std::{fs, io, path::PathBuf};

    fn fixture() -> io::Result<PathBuf> {
        let root = std::env::current_dir()?
            .join(".runtime/native-profile-tests")
            .join(nonce());
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    #[test]
    fn genuine_owner_lock_is_exclusive_and_identity_survives_reopen() -> io::Result<()> {
        let root = fixture()?.join("profile");
        let first = Profile::open(&root, &[])?;
        let identity = read_regular(&root.join("profile.json"), 4096)?;
        assert!(Profile::open(&root, &[]).is_err());
        drop(first);
        let reopened = Profile::open(&root, &[])?;
        assert_eq!(read_regular(&root.join("profile.json"), 4096)?, identity);
        assert_eq!(reopened.identity.password.len(), 64);
        Ok(())
    }

    #[test]
    fn unknown_directory_and_malformed_identity_are_never_reset() -> io::Result<()> {
        let root = fixture()?;
        let original = b"Preserve this unrelated original file";
        fs::write(root.join("keep.txt"), original)?;
        assert!(Profile::open(&root, &[]).is_err());
        assert_eq!(fs::read_dir(&root)?.count(), 1);
        fs::write(
            root.join("profile.json"),
            br#"{"schema":true,"product":"cloud-job-orchestrator"}"#,
        )?;
        assert!(Profile::open(&root, &[]).is_err());
        assert_eq!(fs::read(root.join("keep.txt"))?, original);
        Ok(())
    }

    #[test]
    fn every_program_root_is_refused_before_directory_or_marker_creation() -> io::Result<()> {
        let installation = fixture()?;
        let target = installation.join("not-created/profile");
        assert!(Profile::open(&target, std::slice::from_ref(&installation)).is_err());
        assert!(!target.exists());
        assert_eq!(fs::read_dir(installation)?.count(), 0);
        assert!(admit_directory(&PathBuf::from("relative"), &[]).is_err());
        Ok(())
    }

    #[test]
    fn actual_hardlinked_marker_and_owner_retain_external_bytes() -> io::Result<()> {
        for name in ["profile.json", "owner.lock", "postgres-startup.log"] {
            let fixture = fixture()?;
            let root = fixture.join("profile");
            drop(Profile::open(&root, &[])?);
            let selected = root.join(name);
            if name == "postgres-startup.log" {
                fs::write(&selected, b"Preserve original log bytes")?;
            }
            let original = fs::read(&selected)?;
            let target = fixture.join("outside-sentinel");
            fs::write(&target, &original)?;
            fs::remove_file(&selected)?;
            fs::hard_link(&target, &selected)?;
            assert!(Profile::open(&root, &[]).is_err());
            assert_eq!(fs::read(target)?, original);
        }
        Ok(())
    }

    #[test]
    fn closed_database_hardlinks_are_refused_without_touching_targets() -> io::Result<()> {
        let fixture = fixture()?;
        let root = fixture.join("profile");
        drop(Profile::open(&root, &[])?);
        let database = root.join("postgres");
        fs::create_dir(&database)?;
        let target = fixture.join("outside-sentinel");
        fs::write(&target, b"Retain actual closed database alias target")?;
        fs::hard_link(&target, database.join("postgresql.auto.conf"))?;
        assert!(Profile::open(&root, &[]).is_err());
        assert_eq!(
            fs::read(target)?,
            b"Retain actual closed database alias target"
        );
        Ok(())
    }

    #[test]
    fn ordinary_closed_database_tree_is_not_rewritten_on_admission() -> io::Result<()> {
        let root = fixture()?.join("profile");
        drop(Profile::open(&root, &[])?);
        let database = root.join("postgres");
        fs::create_dir_all(database.join("pg_wal"))?;
        fs::write(database.join("PG_VERSION"), b"18\n")?;
        fs::write(
            database.join("pg_wal/retained"),
            b"Preserve ordinary recovery evidence",
        )?;
        drop(Profile::open(&root, &[])?);
        assert_eq!(
            fs::read(database.join("pg_wal/retained"))?,
            b"Preserve ordinary recovery evidence"
        );
        Ok(())
    }

    #[test]
    fn bounded_regular_reader_rejects_aliases_special_kind_and_oversize() -> io::Result<()> {
        let root = fixture()?;
        let file = root.join("bounded");
        fs::write(&file, b"ordinary bounded bytes")?;
        assert!(read_regular(&file, 2).is_err());
        assert!(read_regular(&root, 4096).is_err());
        fs::hard_link(&file, root.join("alias"))?;
        assert!(read_regular(&file, 4096).is_err());
        assert_eq!(fs::read(file)?, b"ordinary bounded bytes");
        Ok(())
    }

    #[test]
    fn actual_packaged_layout_refuses_outer_install_tree_but_keeps_source_profiles()
    -> io::Result<()> {
        let root = fixture()?;
        let install = root.join("installed");
        let assets = install.join("resources/native");
        let program = assets.join("bin");
        let postgres = assets.join("postgres");
        fs::create_dir_all(&program)?;
        fs::create_dir_all(&postgres)?;
        let roots = program_roots(&program, &assets, &postgres)?;
        let candidate = install.join("accidental/new-workspace");
        assert!(Profile::open(&candidate, &roots).is_err());
        assert!(!candidate.exists());
        let developer = root.join("developer/target/debug");
        fs::create_dir_all(&developer)?;
        let profile = root.join("developer/.runtime/profile");
        let roots = program_roots(&developer, &root.join("developer"), &postgres)?;
        drop(Profile::open(&profile, &roots)?);
        assert!(profile.join("profile.json").is_file());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn actual_directory_symlink_is_refused_before_target_mutation() -> io::Result<()> {
        let root = fixture()?;
        let target = root.join("target");
        fs::create_dir(&target)?;
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&target, &alias)?;
        assert!(Profile::open(&alias.join("new"), &[]).is_err());
        assert!(inspect_tree(&alias).is_err());
        assert_eq!(fs::read_dir(target)?.count(), 0);
        Ok(())
    }
}
