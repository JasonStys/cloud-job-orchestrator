//! File: Starts only this native profile's real bundled PostgreSQL18.6 cluster, never a service.
//! Functions: start validates provider resources and closed state; close waits for actual owned stop.
//! Variables: per-profile random credentials stay outside source, arguments and renderer messages.

use crate::{
    database as sqlx,
    native_profile::{self, Profile, admit_directory, read_regular, write_new},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    net::TcpListener,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

const PROVIDER: &str =
    "https://get.enterprisedb.com/postgresql/postgresql-18.6-5-windows-x64-binaries.zip";
const ARCHIVE_SHA256: &str = "e2246ba91d22345bc3d017586c09ede52d9df180b1eeb480f050445f1cad84e2";
const ARCHIVE_BYTES: u64 = 384_620_317;
const SDK_FILES: usize = 1485;
const MAX_SDK_BYTES: u64 = 200 * 1024 * 1024;
const CRT_FILES: [&str; 5] = [
    "msvcp140.dll",
    "msvcp140_1.dll",
    "msvcp140_2.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sdk {
    schema: u8,
    version: String,
    provider: String,
    archive_sha256: String,
    archive_bytes: u64,
    expanded_bytes: u64,
    files: BTreeMap<String, String>,
    crt_files: BTreeMap<String, String>,
    excluded: Vec<String>,
}

/// Validate actual provider bytes before invoking a bundled executable; no user executables import.
pub fn verify_resources(root: &Path) -> io::Result<()> {
    admit_directory(root, &[])?;
    let sdk: Sdk = serde_json::from_slice(&read_regular(&root.join("SDK.json"), 512 * 1024)?)
        .map_err(|_| invalid("PostgreSQL resource manifest is invalid"))?;
    if sdk.schema != 2
        || sdk.version != "18.6"
        || sdk.provider != PROVIDER
        || sdk.archive_sha256 != ARCHIVE_SHA256
        || sdk.archive_bytes != ARCHIVE_BYTES
        || sdk.files.len() != SDK_FILES
        || sdk
            .crt_files
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != CRT_FILES.into_iter().collect()
        || sdk.expanded_bytes > MAX_SDK_BYTES
        || sdk.excluded
            != [
                "pgAdmin",
                "StackBuilder",
                "development headers/import libraries",
            ]
    {
        return Err(invalid(
            "PostgreSQL resource identity differs from the reviewed provider input",
        ));
    }
    let mut total = 0_u64;
    let mut expected: BTreeMap<String, String> = sdk.files;
    for (name, digest) in sdk.crt_files {
        if expected.insert(format!("bin/{name}"), digest).is_some() {
            return Err(invalid(
                "A runtime input collides with the provider inventory",
            ));
        }
    }
    verify_tree(root, &expected)?;
    for (name, digest) in &expected {
        if name.contains('\\')
            || name.contains(':')
            || name.split('/').any(str::is_empty)
            || Path::new(name)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(invalid("Invalid fixed PostgreSQL resource path"));
        }
        let path = root.join(name);
        admit_directory(
            path.parent()
                .ok_or_else(|| invalid("Invalid resource parent"))?,
            &[],
        )?;
        let bytes = read_regular(&path, 64 * 1024 * 1024)?;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid("Resource size overflow"))?;
        if total > MAX_SDK_BYTES || hex::encode(Sha256::digest(&bytes)) != *digest {
            return Err(invalid(
                "Actual PostgreSQL resources differ from their provider inventory",
            ));
        }
    }
    if total != sdk.expanded_bytes {
        return Err(invalid("Actual PostgreSQL expanded byte count differs"));
    }
    Ok(())
}

/// Refuse missing, extra and linked resources before any bundled executable is started.
fn verify_tree(root: &Path, expected: &BTreeMap<String, String>) -> io::Result<()> {
    let mut actual = BTreeSet::new();
    let mut expected_directories = BTreeSet::new();
    for name in expected.keys() {
        let mut parent = Path::new(name).parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            expected_directories.insert(directory.to_string_lossy().replace('\\', "/"));
            parent = directory.parent();
        }
    }
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        admit_directory(&directory, &[])?;
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                admit_directory(&path, &[])?;
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| invalid("Invalid resource prefix"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if !expected_directories.contains(&relative) {
                    return Err(invalid("Unexpected PostgreSQL resource directory"));
                }
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| invalid("Invalid resource prefix"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative != "SDK.json" && (!kind.is_file() || !expected.contains_key(&relative))
                {
                    return Err(invalid("Unexpected PostgreSQL resource member"));
                }
                if relative != "SDK.json" {
                    actual.insert(relative);
                }
            }
            if actual.len() + pending.len() > SDK_FILES + CRT_FILES.len() + 256 {
                return Err(invalid("PostgreSQL resource tree exceeds its bound"));
            }
        }
    }
    if actual != expected.keys().cloned().collect() {
        return Err(invalid("PostgreSQL resource inventory is incomplete"));
    }
    Ok(())
}

/// An exact newly started cluster. The caller retains the Profile OS lock through actual shutdown.
pub struct NativeDatabase {
    resources: PathBuf,
    data: PathBuf,
    temporary: PathBuf,
    pub port: u16,
}

impl NativeDatabase {
    /// Initialize only genuinely empty new state or reopen version18; preserve partial/foreign state.
    pub async fn start(profile: &Profile, resources: &Path) -> io::Result<Self> {
        verify_resources(resources)?;
        let data = profile.directory.join("postgres");
        match fs::symlink_metadata(data.join("postmaster.pid")) {
            Ok(_) => {
                return Err(invalid(
                    "A PostgreSQL owner marker already exists; preserve the closed profile for review",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let temporary = profile.directory.join("temporary");
        admit_directory(&temporary, &[])?;
        fs::create_dir_all(&temporary)?;
        let lease = TcpListener::bind(("127.0.0.1", 0))?;
        let port = lease.local_addr()?.port();
        let database = Self {
            resources: resources.to_owned(),
            data,
            temporary,
            port,
        };
        database.initialize(profile).await?;
        drop(lease);
        let settings = format!(
            "-h 127.0.0.1 -p {} -c cluster_name={} -c max_connections=20 -c shared_buffers=32MB -c log_statement=none -c log_min_error_statement=panic",
            port,
            cluster_name(&profile.identity.cluster)
        );
        let status = database
            .run(
                "pg_ctl.exe",
                &[
                    "start".into(),
                    "-D".into(),
                    database.data.as_os_str().to_owned(),
                    "-l".into(),
                    profile
                        .directory
                        .join("postgres-startup.log")
                        .into_os_string(),
                    "-w".into(),
                    "-t".into(),
                    "20".into(),
                    "-o".into(),
                    settings.into(),
                ],
            )
            .await;
        match status {
            Ok(0) => Ok(database),
            failed => {
                if fs::symlink_metadata(database.data.join("postmaster.pid")).is_ok() {
                    database.close().await?;
                }
                failed?;
                Err(invalid(
                    "Owned PostgreSQL startup failed; preserve its profile and log",
                ))
            }
        }
    }

    /// Initialize only empty owned data; retain every partial or incompatible existing cluster.
    async fn initialize(&self, profile: &Profile) -> io::Result<()> {
        match fs::symlink_metadata(self.data.join("PG_VERSION")) {
            Ok(_)
                if read_regular(&self.data.join("PG_VERSION"), 16)? == b"18\r\n"
                    || read_regular(&self.data.join("PG_VERSION"), 16)? == b"18\n" => {}
            Ok(_) => {
                return Err(invalid(
                    "Existing database version differs; no reset attempted",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if self.data.exists() && fs::read_dir(&self.data)?.next().is_some() {
                    return Err(invalid(
                        "Partial PostgreSQL initialization is preserved, not reset",
                    ));
                }
                let password = profile
                    .directory
                    .join(format!("initial-password-{}", native_profile::nonce()));
                write_new(&password, profile.identity.password.as_bytes())?;
                let result = self
                    .run(
                        "initdb.exe",
                        &[
                            "-D".into(),
                            self.data.as_os_str().to_owned(),
                            "-U".into(),
                            "cjo_owner".into(),
                            "--auth-host=scram-sha-256".into(),
                            "--auth-local=reject".into(),
                            "--encoding=UTF8".into(),
                            "--no-locale".into(),
                            "--pwfile".into(),
                            password.as_os_str().to_owned(),
                        ],
                    )
                    .await;
                fs::remove_file(password)?;
                if result? != 0 {
                    return Err(invalid(
                        "Owned PostgreSQL initialization failed; partial state is preserved",
                    ));
                }
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    /// Connect only to the exact authenticated own cluster, then verify its version/name/directory.
    pub async fn connect(
        &self,
        profile: &Profile,
    ) -> Result<crate::PostgresStore, Box<dyn std::error::Error + Send + Sync>> {
        use sqlx::Row;
        let options = sqlx::postgres::PgConnectOptions::new()
            .host("127.0.0.1")
            .port(self.port)
            .username("cjo_owner")
            .password(&profile.identity.password)
            .database("postgres");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
            .map_err(|_| invalid("Native PostgreSQL authentication/connection failed"))?;
        let row = sqlx::query("SELECT current_setting('cluster_name') AS cluster, current_setting('data_directory') AS directory, current_setting('server_version') AS version")
            .fetch_one(&pool).await.map_err(|_| invalid("Native PostgreSQL identity query failed"))?;
        let actual: String = row.try_get("directory")?;
        let cluster_matches =
            row.try_get::<String, _>("cluster")? == cluster_name(&profile.identity.cluster);
        let version_matches = row.try_get::<String, _>("version")? == "18.6";
        let directory_matches = fs::canonicalize(actual)? == fs::canonicalize(&self.data)?;
        eprintln!(
            "Owned PostgreSQL identity: cluster={cluster_matches}, version={version_matches}, directory={directory_matches}"
        );
        let own = cluster_matches && version_matches && directory_matches;
        pool.close().await;
        if !own {
            return Err(invalid("Authenticated native PostgreSQL identity differs").into());
        }
        let url = format!(
            "postgres://cjo_owner:{}@127.0.0.1:{}/postgres",
            profile.identity.password, self.port
        );
        Ok(crate::PostgresStore::connect(&url, 5).await?)
    }

    /// Stop only the identified own data directory; retain the owner until `pg_ctl` reports no server.
    pub async fn close(&self) -> io::Result<()> {
        let _ = self
            .run(
                "pg_ctl.exe",
                &[
                    "stop".into(),
                    "-D".into(),
                    self.data.as_os_str().to_owned(),
                    "-m".into(),
                    "fast".into(),
                    "-w".into(),
                    "-t".into(),
                    "20".into(),
                ],
            )
            .await?;
        loop {
            let status = self
                .run(
                    "pg_ctl.exe",
                    &[
                        "status".into(),
                        "-D".into(),
                        self.data.as_os_str().to_owned(),
                    ],
                )
                .await?;
            if status == 3 {
                return Ok(());
            }
            if status != 0 {
                return Err(invalid(
                    "Owned PostgreSQL shutdown status is unknown; profile is preserved",
                ));
            }
            // Wait for actual server shutdown rather than force killing it at an arbitrary UI deadline.
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn run(&self, executable: &str, arguments: &[std::ffi::OsString]) -> io::Result<i32> {
        if !matches!(executable, "initdb.exe" | "pg_ctl.exe") {
            return Err(invalid(
                "Only fixed bundled PostgreSQL lifecycle programs are allowed",
            ));
        }
        let mut command = Command::new(self.resources.join("bin").join(executable));
        command
            .args(arguments)
            .current_dir(&self.resources)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .env("TEMP", &self.temporary)
            .env("TMP", &self.temporary);
        for name in ["SYSTEMROOT", "WINDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let system = PathBuf::from(
            std::env::var_os("SYSTEMROOT")
                .ok_or_else(|| invalid("Windows system root is unavailable"))?,
        )
        .join("System32");
        command.env(
            "PATH",
            std::env::join_paths([self.resources.join("bin"), system]).map_err(io::Error::other)?,
        );
        let status = tokio::time::timeout(Duration::from_secs(30), command.status())
            .await
            .map_err(|_| {
                invalid("Owned PostgreSQL lifecycle command exceeded its bounded operation")
            })??;
        status.code().ok_or_else(|| {
            invalid("Owned PostgreSQL lifecycle process did not return a normal exit code")
        })
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Keep one UUID's hexadecimal identity within PostgreSQL's 63-byte `cluster_name` bound.
fn cluster_name(identity: &str) -> String {
    format!("cjo_{}", identity.chars().take(32).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::cluster_name;
    #[test]
    fn cluster_identity_fits_the_real_postgresql_name_limit_without_disabling_verification() {
        let identity = "abcde123".repeat(8);
        assert_eq!(
            cluster_name(&identity),
            "cjo_abcde123abcde123abcde123abcde123"
        );
        assert_eq!(cluster_name(&identity).len(), 36);
        assert!(cluster_name(&identity).len() < 64);
    }
}
