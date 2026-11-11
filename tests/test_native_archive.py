"""File: tests/test_native_archive.py
Purpose: Check actual archive inputs, traversal, special files, duplicate names and expansion bounds.
Functions: unittest cases exercise real ZIPs and retained byte sentinels; no app/profile is launched.
Variables: all files live in new temporary directories; originals and earlier evidence are untouched.
"""

import importlib.util
import os
import stat
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "cloud_native_archive",
    Path(__file__).resolve().parents[1] / "scripts/archive_native.py",
)
archive_native = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(archive_native)


class ArchiveBoundaries(unittest.TestCase):
    """Actual small independent ZIP fixtures must not become unsafe output or false success."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="cjo-archive-contract-")
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def test_real_payload_with_old_vendor_timestamp_roundtrips_without_replacement(
        self,
    ):
        bundle = self.root / "bundle"
        bundle.mkdir()
        content = bundle / "runtime.dll"
        content.write_bytes(b"inert vendor bytes\x00\xff")
        os.utime(content, (0, 0))
        target = self.root / "app.zip"
        self.assertEqual(len(archive_native.create_archive(bundle, target)), 64)
        output = self.root / "relocated"
        archive_native.extract_archive(target, output)
        self.assertEqual((output / "runtime.dll").read_bytes(), content.read_bytes())
        with zipfile.ZipFile(target) as package:
            self.assertEqual(package.getinfo("runtime.dll").date_time[0], 1980)
        with self.assertRaises(FileExistsError):
            archive_native.create_archive(bundle, target)
        with self.assertRaises(ValueError):
            archive_native.extract_archive(target, output)

    def test_actual_hardlinked_payload_refuses_without_touching_outside_bytes(self):
        bundle = self.root / "bundle"
        bundle.mkdir()
        sentinel = self.root / "outside.txt"
        sentinel.write_bytes(b"retain inert sentinel")
        os.link(sentinel, bundle / "alias.txt")
        with self.assertRaises(ValueError):
            archive_native.create_archive(bundle, self.root / "refused.zip")
        self.assertEqual(sentinel.read_bytes(), b"retain inert sentinel")
        self.assertFalse((self.root / "refused.zip").exists())

    def test_malicious_names_fail_before_output_creation(self):
        for index, name in enumerate(
            (
                "../escape",
                "/absolute",
                "C:/drive",
                "a:stream",
                "NUL",
                "x/CON.txt",
                "space ",
                "trailing.",
                "x//y",
                "x/./y",
            )
        ):
            with self.subTest(name=name):
                target = self.root / f"case-{index}.zip"
                with zipfile.ZipFile(target, "w") as package:
                    package.writestr(name, b"refused")
                output = self.root / f"case-{index}"
                with self.assertRaises(ValueError):
                    archive_native.extract_archive(target, output)
                self.assertFalse(output.exists())

    def test_case_collisions_and_declared_links_fail_before_output(self):
        for index, names in enumerate((("Case.txt", "case.TXT"), ("file", "link"))):
            target = self.root / f"collision-{index}.zip"
            with zipfile.ZipFile(target, "w") as package:
                for name in names:
                    member = zipfile.ZipInfo(name)
                    if name == "link":
                        member.create_system = 3
                        member.external_attr = (stat.S_IFLNK | 0o777) << 16
                    package.writestr(member, b"inert")
            with self.assertRaises(ValueError):
                archive_native.extract_archive(target, self.root / f"collision-{index}")

    def test_actual_declared_expansion_budget_is_enforced_before_output(self):
        target = self.root / "expanded.zip"
        with zipfile.ZipFile(target, "w", compression=zipfile.ZIP_DEFLATED) as package:
            package.writestr("payload", b"x" * 200)
        with (
            patch.object(archive_native, "MAX_MEMBER", 100),
            self.assertRaises(ValueError),
        ):
            archive_native.extract_archive(target, self.root / "expanded")
        self.assertFalse((self.root / "expanded").exists())

    def test_real_source_growth_after_inventory_cannot_silently_enter_archive(self):
        bundle = self.root / "growing"
        bundle.mkdir()
        source = bundle / "source.txt"
        source.write_bytes(b"original")
        original = zipfile.ZipInfo.from_file

        def changed(path, *args, **kwargs):
            result = original(path, *args, **kwargs)
            with source.open("ab") as output:
                output.write(b"extra actual bytes")
            return result

        with (
            patch.object(zipfile.ZipInfo, "from_file", side_effect=changed),
            self.assertRaises(ValueError),
        ):
            archive_native.create_archive(bundle, self.root / "changed.zip")


if __name__ == "__main__":
    unittest.main()
