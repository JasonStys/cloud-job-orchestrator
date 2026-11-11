"""File: scripts/archive_native.py
Purpose: Validate a complete Windows ZIP, relocate it and use the actual bundled native application.
Functions: create_archive/extract_archive bound names and real bytes; main owns only fresh test paths.
Variables: fixed product/version, finite expansion budgets and current-source synthetic receipts.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import subprocess
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
MAX_MEMBERS = 5000
MAX_TOTAL = 1024 * 1024 * 1024
MAX_MEMBER = 300 * 1024 * 1024
BLOCK = 65536
RESERVED = {
    "CON",
    "PRN",
    "AUX",
    "NUL",
    *(f"COM{i}" for i in range(1, 10)),
    *(f"LPT{i}" for i in range(1, 10)),
}


def require(condition, message):
    """Keep archive checks active under optimized Python."""
    if not condition:
        raise ValueError(message)


def real_directory(path):
    """Refuse pre-existing directory aliases before traversing or creating generated output."""
    for current in (path.absolute(), *path.absolute().parents):
        require(
            not current.is_symlink() and not current.is_junction(),
            "Linked archive directory refused",
        )
        require(
            not current.exists() or current.is_dir(),
            "Expected a real archive directory",
        )


def create_archive(bundle: Path, archive: Path) -> str:
    """Bound traversal before descent and write only regular single-link files to a new ZIP."""
    real_directory(bundle)
    real_directory(archive.parent)
    pending, files, total, members = [bundle], [], 0, 0
    while pending:
        directory = pending.pop()
        for path in directory.iterdir():
            members += 1
            require(members <= MAX_MEMBERS, "Portable member count exceeded")
            require(
                not path.is_symlink() and not path.is_junction(),
                "Linked portable member refused",
            )
            info = path.lstat()
            if stat.S_ISDIR(info.st_mode):
                pending.append(path)
            else:
                require(
                    stat.S_ISREG(info.st_mode)
                    and info.st_nlink == 1
                    and info.st_size <= MAX_MEMBER,
                    "Expected bounded regular portable bytes",
                )
                total += info.st_size
                require(total <= MAX_TOTAL, "Portable byte count exceeded")
                files.append(path)
    require(files, "Empty portable input")
    with zipfile.ZipFile(
        archive,
        "x",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=1,
        strict_timestamps=False,
    ) as output:
        actual_total = 0
        for path in sorted(files):
            member = zipfile.ZipInfo.from_file(
                path, path.relative_to(bundle).as_posix(), strict_timestamps=False
            )
            member.compress_type = zipfile.ZIP_DEFLATED
            member.compress_level = 1
            descriptor = os.open(
                path,
                os.O_RDONLY
                | getattr(os, "O_NOFOLLOW", 0)
                | getattr(os, "O_NONBLOCK", 0)
                | getattr(os, "O_BINARY", 0),
            )
            with os.fdopen(descriptor, "rb") as source:
                opened = os.fstat(source.fileno())
                require(
                    stat.S_ISREG(opened.st_mode)
                    and opened.st_nlink == 1
                    and opened.st_size == member.file_size,
                    "Portable input changed before reading",
                )
                count = 0
                with output.open(member, "w") as target:
                    while block := source.read(min(BLOCK, MAX_MEMBER - count + 1)):
                        count += len(block)
                        actual_total += len(block)
                        require(
                            count <= member.file_size
                            and count <= MAX_MEMBER
                            and actual_total <= MAX_TOTAL,
                            "Actual portable source bytes exceeded",
                        )
                        target.write(block)
                require(
                    count == member.file_size,
                    "Actual portable source byte count differs",
                )
    with archive.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def extract_archive(archive: Path, destination: Path) -> None:
    """Validate the complete inventory first; bound real decompression and never replace output."""
    real_directory(destination.parent)
    require(not destination.exists(), "A fresh relocation directory is required")
    with zipfile.ZipFile(archive) as package:
        members = package.infolist()
        require(0 < len(members) <= MAX_MEMBERS, "Portable archive count exceeded")
        names, total = set(), 0
        for member in members:
            name, path = member.filename, PurePosixPath(member.filename)
            require(
                name
                and member.orig_filename == name
                and not path.is_absolute()
                and "\\" not in name
                and ":" not in name
                and all(
                    part not in ("", ".", "..") for part in name.rstrip("/").split("/")
                )
                and all(
                    not part.endswith((" ", "."))
                    and part.split(".")[0].upper() not in RESERVED
                    for part in path.parts
                )
                and str(path).casefold() not in names
                and stat.S_IFMT(member.external_attr >> 16)
                in (0, stat.S_IFREG, stat.S_IFDIR)
                and 0 <= member.file_size <= MAX_MEMBER,
                "Unsafe or ambiguous portable member",
            )
            names.add(str(path).casefold())
            total += member.file_size
            require(total <= MAX_TOTAL, "Portable declared bytes exceeded")
        destination.mkdir()
        actual_total = 0
        for member in members:
            target = destination.joinpath(*PurePosixPath(member.filename).parts)
            if member.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            count = 0
            with package.open(member) as source, target.open("xb") as output:
                while block := source.read(min(BLOCK, MAX_MEMBER - count + 1)):
                    count += len(block)
                    actual_total += len(block)
                    require(
                        count <= member.file_size
                        and count <= MAX_MEMBER
                        and actual_total <= MAX_TOTAL,
                        "Portable actual bytes exceeded",
                    )
                    output.write(block)
            require(count == member.file_size, "Portable declared/actual bytes differ")


def main():
    """Use the full extracted app with a marked profile; retain any earlier failure without deletion."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("proof", type=Path)
    args = parser.parse_args()
    bundle, proof = args.bundle.absolute(), args.proof.absolute()
    require(
        bundle.is_relative_to(ROOT / "release")
        and proof.is_relative_to(ROOT / "artifacts/native"),
        "Generated native paths must remain in their project output directories",
    )
    archive = bundle.parent / "CloudJobOrchestrator-1.1.0-windows-x64.zip"
    digest = create_archive(bundle, archive)
    extracted = bundle.parent / "zip-relocated"
    extract_archive(archive, extracted)
    environment = dict(os.environ)
    environment["CJO_NATIVE_EXECUTABLE"] = str(extracted / "CloudJobOrchestrator.exe")
    environment["CJO_NATIVE_PROOF"] = str(proof)
    result = subprocess.run(
        ["node", str(ROOT / "scripts/test-native-app.mjs")],
        cwd=ROOT,
        env=environment,
        timeout=180,
        check=False,
    )
    require(result.returncode == 0, "The actual extracted application did not pass")
    receipt = json.loads((proof / "acceptance.json").read_text(encoding="utf-8"))
    require(
        receipt.get("passed") is True
        and receipt.get("actualLaunches") == 2
        and receipt.get("normalExitCodes") == [0, 0],
        "Actual native receipt is incomplete",
    )
    with (archive.with_suffix(archive.suffix + ".sha256")).open(
        "x", encoding="utf-8", newline="\n"
    ) as output:
        output.write(digest + "  " + archive.name + "\n")
    (proof / "archive.json").write_text(
        json.dumps(
            {
                "schema": 1,
                "passed": True,
                "archiveSha256": digest,
                "archiveBytes": archive.stat().st_size,
                "source": receipt["source"],
                "sourceDirty": receipt["sourceDirty"],
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(json.dumps({"passed": True, "archiveSha256": digest, "actualLaunches": 2}))


if __name__ == "__main__":
    main()
