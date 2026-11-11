"""File: scripts/postgres-sdk.py
Purpose: Prepare only exact official PostgreSQL18.6 Windows server resources for native packaging.
Symbols: main validates the provider archive and bounded selected extraction; no database is started.
Inputs are build-time public resources. No installer, system service, profile or Docker is touched.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import subprocess
import time
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath

URL = (
    "https://get.enterprisedb.com/postgresql/postgresql-18.6-5-windows-x64-binaries.zip"
)
SHA256 = "e2246ba91d22345bc3d017586c09ede52d9df180b1eeb480f050445f1cad84e2"
ARCHIVE_BYTES = 384_620_317
MAX_MEMBERS = 5000
MAX_MEMBER = 64 * 1024 * 1024
MAX_EXPANDED = 200 * 1024 * 1024
EXECUTABLES = {"postgres.exe", "initdb.exe", "pg_ctl.exe", "pg_isready.exe"}
NOTICES = {"server_license.txt", "commandlinetools_3rd_party_licenses.txt"}
ROOT = Path(__file__).resolve().parents[1]
CRT_NAMES = {
    "msvcp140.dll",
    "msvcp140_1.dll",
    "msvcp140_2.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll",
}


def require(condition: bool, message: str) -> None:
    """Keep all build-input checks active under optimized Python."""
    if not condition:
        raise ValueError(message)


def selected(name: str) -> bool:
    """Exclude pgAdmin/StackBuilder/development tools; retain fixed server binaries and support data."""
    parts = PurePosixPath(name).parts
    if not parts or parts[0] != "pgsql":
        return False
    relative = parts[1:]
    if len(relative) == 1:
        return relative[0] in NOTICES
    if len(relative) < 2:
        return False
    folder, leaf = relative[0], relative[-1]
    if folder == "bin":
        return leaf in EXECUTABLES or (
            leaf.endswith(".dll")
            and (leaf.startswith(("lib", "icu")) or leaf == "zlib1.dll")
        )
    return folder == "share" or (folder == "lib" and leaf.endswith(".dll"))


def real_directory(path: Path) -> None:
    """Refuse linked build-output ancestors before creating a fresh staging folder."""
    for parent in (path, *path.parents):
        require(
            not parent.is_symlink() and not parent.is_junction(),
            "Linked SDK output refused",
        )


def fetch(archive: Path) -> None:
    """Download a single fixed official HTTPS resource with byte/time bounds and no overwrite."""
    archive.parent.mkdir(parents=True, exist_ok=True)
    deadline, total = time.monotonic() + 240, 0
    with (
        urllib.request.urlopen(URL, timeout=30) as source,
        archive.open("xb") as output,
    ):
        require(
            source.status == 200 and source.url == URL,
            "Unexpected PostgreSQL provider origin",
        )
        while True:
            require(
                time.monotonic() <= deadline,
                "Official PostgreSQL download deadline exceeded",
            )
            block = source.read(1024 * 1024)
            if not block:
                break
            total += len(block)
            require(total <= ARCHIVE_BYTES, "Official PostgreSQL byte count exceeded")
            output.write(block)
    require(total == ARCHIVE_BYTES, "Official PostgreSQL download was incomplete")


def prepare(archive: Path, destination: Path, crt_directory: Path) -> dict:
    """Hash and extract from the same descriptor; preserve failed/unknown outputs without resetting."""
    destination = destination.absolute()
    require(".." not in destination.parts, "Parent traversal is not an SDK output path")
    require(not destination.exists(), "Use a fresh SDK staging directory")
    real_directory(destination)
    destination = destination.resolve()
    require(
        destination.is_relative_to((ROOT / ".runtime").resolve()),
        "SDK staging must stay in this repo .runtime",
    )
    environment = {
        key: value
        for key, value in os.environ.items()
        if key.casefold() != "psmodulepath"
    }
    powershell = (
        Path(os.environ["SYSTEMROOT"])
        / "System32/WindowsPowerShell/v1.0/powershell.exe"
    )
    verified = subprocess.run(
        [
            str(powershell),
            "-NoProfile",
            "-NonInteractive",
            "-File",
            str(ROOT / "scripts/check-crt.ps1"),
            "-Directory",
            str(crt_directory),
        ],
        check=True,
        capture_output=True,
        text=True,
        timeout=90,
        env=environment,
    )
    crt_hashes = json.loads(verified.stdout)
    require(
        isinstance(crt_hashes, dict) and set(crt_hashes) == CRT_NAMES,
        "Incomplete app-local runtime verification",
    )
    descriptor = os.open(
        archive,
        os.O_RDONLY
        | getattr(os, "O_BINARY", 0)
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_NONBLOCK", 0),
    )
    with os.fdopen(descriptor, "rb") as raw:
        info = os.fstat(raw.fileno())
        require(
            stat.S_ISREG(info.st_mode)
            and info.st_nlink == 1
            and info.st_size == ARCHIVE_BYTES
            and not archive.is_symlink(),
            "Expected the exact regular public SDK archive",
        )
        require(
            hashlib.file_digest(raw, "sha256").hexdigest() == SHA256,
            "Official SDK digest differs",
        )
        raw.seek(0)
        with zipfile.ZipFile(raw) as package:
            members = [
                member
                for member in package.infolist()
                if selected(member.filename) and not member.is_dir()
            ]
            require(
                0 < len(members) <= MAX_MEMBERS,
                "Selected SDK inventory exceeds its bound",
            )
            seen, total = set(), 0
            for member in members:
                name = member.filename.removeprefix("pgsql/")
                parts = name.split("/")
                require(
                    name
                    and "\\" not in name
                    and ":" not in name
                    and not name.startswith("/")
                    and all(part not in ("", ".", "..") for part in parts)
                    and name.casefold() not in seen,
                    "Unsafe or duplicate SDK member",
                )
                require(
                    (member.external_attr >> 16) & 0xF000 in (0, stat.S_IFREG),
                    "Linked SDK member",
                )
                require(
                    0 <= member.file_size <= MAX_MEMBER,
                    "SDK member exceeds its byte budget",
                )
                total += member.file_size
                require(
                    total <= MAX_EXPANDED, "Selected SDK expanded-byte budget exceeded"
                )
                seen.add(name.casefold())
            require(
                {"bin/" + name for name in EXECUTABLES}.issubset(seen)
                and NOTICES.issubset(seen),
                "SDK is missing fixed executables/notices",
            )
            destination.mkdir(parents=True)
            hashes, actual_total = {}, 0
            for member in members:
                name = member.filename.removeprefix("pgsql/")
                output = destination.joinpath(*PurePosixPath(name).parts)
                output.parent.mkdir(parents=True, exist_ok=True)
                size, digest = 0, hashlib.sha256()
                with package.open(member) as source, output.open("xb") as target:
                    while block := source.read(65536):
                        size += len(block)
                        actual_total += len(block)
                        require(
                            size <= MAX_MEMBER and actual_total <= MAX_EXPANDED,
                            "Actual selected SDK bytes exceed their bounds",
                        )
                        target.write(block)
                        digest.update(block)
                require(
                    size == member.file_size,
                    "SDK member actual/declared lengths differ",
                )
                hashes[name] = digest.hexdigest()
    for name, expected in crt_hashes.items():
        source = crt_directory / name
        require(
            not source.is_symlink()
            and source.stat().st_nlink == 1
            and source.stat().st_size <= 4 * 1024 * 1024,
            "Linked or oversized runtime input",
        )
        with source.open("rb") as stream:
            content = stream.read(4 * 1024 * 1024 + 1)
        require(
            len(content) <= 4 * 1024 * 1024
            and hashlib.sha256(content).hexdigest() == expected,
            "Runtime input changed after signature verification",
        )
        with (destination / "bin" / name).open("xb") as target:
            target.write(content)
        actual_total += len(content)
    receipt = {
        "schema": 2,
        "version": "18.6",
        "provider": URL,
        "archive_sha256": SHA256,
        "archive_bytes": ARCHIVE_BYTES,
        "files": hashes,
        "expanded_bytes": actual_total,
        "crt_files": crt_hashes,
        "excluded": ["pgAdmin", "StackBuilder", "development headers/import libraries"],
    }
    with (destination / "SDK.json").open("x", encoding="utf-8") as output:
        json.dump(receipt, output, indent=2, sort_keys=True)
    return receipt


def main() -> None:
    """Prepare only public build resources, with an optional already verified official ZIP cache."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--archive", type=Path, default=ROOT / ".runtime/postgresql-official.zip"
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--crt-sdk", type=Path, required=True)
    args = parser.parse_args()
    archive, output = args.archive.absolute(), args.output.absolute()
    if not archive.exists():
        real_directory(archive.parent)
        require(
            archive.resolve().is_relative_to((ROOT / ".runtime").resolve()),
            "New SDK download caches must remain in this repo .runtime",
        )
        fetch(archive)
    result = prepare(archive, output, args.crt_sdk.absolute())
    print(
        json.dumps(
            {
                "version": result["version"],
                "archive_sha256": SHA256,
                "members": len(result["files"]),
                "expanded_bytes": result["expanded_bytes"],
            }
        )
    )


if __name__ == "__main__":
    main()
