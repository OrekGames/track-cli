#!/usr/bin/env python3
"""Regression tests for scripts/build-pypi-wheels.py

Usage: python3 scripts/test-build-pypi-wheels.py

Uses a local file:// "release" directory as the download source so the tests
do not touch the network. The binaries in it are synthetic: just enough of an
ELF, Mach-O or PE header for the script to read a platform tag from.
"""

import base64
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "build-pypi-wheels.py"
REPO_ROOT = SCRIPT.parent.parent

# Loading the script by path would otherwise leave a scripts/__pycache__ behind.
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("build_pypi_wheels", SCRIPT)
wheels = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wheels)

X86_64, AARCH64 = 0x3E, 0xB7
MACHO_X86_64, MACHO_ARM64 = 0x01000007, 0x0100000C


# --- Synthetic binaries ------------------------------------------------------


def make_elf(machine, glibc=("2.17",), needed=("libc.so.6",), verneed=True):
    """A minimal ELF64 with .dynstr, .dynamic and (optionally) .gnu.version_r."""
    strings = b"\0"
    offsets = {}
    for name in ["libc.so.6", *needed, *(f"GLIBC_{v}" for v in glibc)]:
        if name not in offsets:
            offsets[name] = len(strings)
            strings += name.encode() + b"\0"

    dynamic = b"".join(struct.pack("<qQ", 1, offsets[lib]) for lib in needed)
    dynamic += struct.pack("<qQ", 0, 0)

    need = struct.pack("<HHIII", 1, len(glibc), offsets["libc.so.6"], 16, 0)
    for index, version in enumerate(glibc):
        last = index == len(glibc) - 1
        need += struct.pack("<IHHII", 0, 0, index + 2, offsets[f"GLIBC_{version}"], 0 if last else 16)

    body = b""
    sections = [struct.pack("<IIQQQQIIQQ", 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)]

    def add(sh_type, content, link=0, info=0):
        nonlocal body
        sections.append(
            struct.pack("<IIQQQQIIQQ", 0, sh_type, 0, 0, 64 + len(body), len(content), link, info, 0, 0)
        )
        body += content

    add(3, strings)  # .dynstr is section 1
    add(6, dynamic, link=1)
    if verneed:
        add(0x6FFFFFFE, need, link=1, info=1)

    ident = b"\x7fELF\x02\x01\x01" + b"\0" * 9
    header = struct.pack(
        "<16sHHIQQQIHHHHHH", ident, 3, machine, 1, 0, 0, 64 + len(body), 0, 64, 0, 0, 64, len(sections), 0
    )
    return header + body + b"".join(sections)


def macos_version(major, minor):
    return (major << 16) | (minor << 8)


def make_macho(cpu, minimum, build_version=True):
    """A minimal Mach-O with one load command carrying the deployment target."""
    if build_version:
        command = struct.pack("<IIIIII", 0x32, 24, 1, minimum, minimum, 0)
    else:
        command = struct.pack("<IIII", 0x24, 16, minimum, minimum)
    header = struct.pack("<IiiIIIII", 0xFEEDFACF, cpu - (1 << 32) if cpu >> 31 else cpu, 0, 2, 1, len(command), 0, 0)
    return header + command


def make_pe(machine):
    return b"MZ" + b"\0" * 0x3A + struct.pack("<I", 0x40) + b"PE\0\0" + struct.pack("<H", machine)


BINARIES = {
    "aarch64-apple-darwin": make_macho(MACHO_ARM64, macos_version(11, 0)),
    "x86_64-apple-darwin": make_macho(MACHO_X86_64, macos_version(10, 12), build_version=False),
    "aarch64-unknown-linux-gnu": make_elf(AARCH64, glibc=("2.17", "2.34", "2.39")),
    "x86_64-unknown-linux-gnu": make_elf(X86_64, glibc=("2.2.5", "2.39"), needed=("libz.so.1", "libc.so.6")),
    "aarch64-pc-windows-msvc": make_pe(0xAA64),
    "x86_64-pc-windows-msvc": make_pe(0x8664),
}

EXPECTED_WHEELS = {
    "orekgames_track-1.1.0-py3-none-macosx_11_0_arm64.whl",
    "orekgames_track-1.1.0-py3-none-macosx_10_12_x86_64.whl",
    "orekgames_track-1.1.0-py3-none-manylinux_2_39_aarch64.whl",
    "orekgames_track-1.1.0-py3-none-manylinux_2_39_x86_64.whl",
    "orekgames_track-1.1.0-py3-none-win_arm64.whl",
    "orekgames_track-1.1.0-py3-none-win_amd64.whl",
}


# --- Release fixture ---------------------------------------------------------


def write_archive(path, member, content):
    if path.name.endswith(".zip"):
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr(member, content)
    else:
        with tarfile.open(path, "w:gz") as archive:
            info = tarfile.TarInfo(member)
            info.size = len(content)
            archive.addfile(info, io.BytesIO(content))


def make_release(directory, version, binaries=BINARIES):
    """Build a local "release": six archives and a checksums file."""
    directory.mkdir(parents=True)
    lines = []
    for target, (ext, binary) in wheels.TARGETS.items():
        path = directory / f"track-{version}-{target}.{ext}"
        write_archive(path, f"track-{version}-{target}/{binary}", binaries[target])
        lines.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
    (directory / "checksums-sha256.txt").write_text("".join(lines))
    return directory / "checksums-sha256.txt"


class ReleaseCase(unittest.TestCase):
    def setUp(self):
        self.work = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.release = self.work / "release"
        self.out = self.work / "wheels"
        self.checksums = make_release(self.release, "1.1.0")

    def build(self, version="1.1.0", keep=False, checksums=None, index_version=None):
        index = self.work / "index.json"
        if index_version:
            index.write_text(json.dumps({"info": {"version": index_version}}))
        with contextlib.redirect_stdout(io.StringIO()):
            return wheels.build_wheels(
                index_json_url=f"file://{index}",
                version=version,
                checksums_file=checksums or self.checksums,
                out_dir=self.out,
                repo_root=REPO_ROOT,
                download_dir=self.work / "archives",
                url_template=f"file://{self.release}/{{FILE}}",
                keep=keep,
            )

    def assert_fails(self, message, **kwargs):
        with self.assertRaises(wheels.BuildError) as caught:
            self.build(**kwargs)
        self.assertIn(message, str(caught.exception))
        # A failed build leaves no wheel behind.
        self.assertEqual(list(self.out.glob("*.whl")) if self.out.exists() else [], [])

    def rewrite_checksums(self, transform):
        self.checksums.write_text(transform(self.checksums.read_text()))


# --- Tests -------------------------------------------------------------------


class BuildTests(ReleaseCase):
    def test_builds_one_wheel_per_archive_with_the_tag_read_from_the_binary(self):
        built = self.build()
        self.assertEqual({path.name for path in built}, EXPECTED_WHEELS)
        self.assertEqual({path.name for path in self.out.iterdir()}, EXPECTED_WHEELS)
        self.assertFalse((self.work / "archives").exists(), "downloads cleaned up")

    def test_cleanup_leaves_other_files_in_the_download_directory(self):
        (self.work / "archives").mkdir()
        (self.work / "archives" / "unrelated.txt").write_text("keep me")
        self.build()
        self.assertEqual([p.name for p in (self.work / "archives").iterdir()], ["unrelated.txt"])

    def test_downloads_are_kept_on_request(self):
        self.build(keep=True)
        self.assertEqual(len(list((self.work / "archives").iterdir())), 6)

    def test_wheel_layout_metadata_and_record(self):
        self.build()
        for name, binary, target in [
            ("orekgames_track-1.1.0-py3-none-manylinux_2_39_x86_64.whl", "track", "x86_64-unknown-linux-gnu"),
            ("orekgames_track-1.1.0-py3-none-win_amd64.whl", "track.exe", "x86_64-pc-windows-msvc"),
        ]:
            with self.subTest(wheel=name), zipfile.ZipFile(self.out / name) as wheel:
                names = wheel.namelist()
                script = f"orekgames_track-1.1.0.data/scripts/{binary}"
                info = "orekgames_track-1.1.0.dist-info"
                self.assertEqual(
                    names,
                    [script, f"{info}/METADATA", f"{info}/WHEEL", f"{info}/licenses/LICENSE", f"{info}/RECORD"],
                )
                # The binary is the release's, byte for byte, and executable.
                self.assertEqual(wheel.read(script), BINARIES[target])
                self.assertEqual(wheel.getinfo(script).external_attr >> 16, 0o100755)

                metadata = wheel.read(f"{info}/METADATA").decode()
                self.assertIn("\nName: orekgames-track\n", metadata)
                self.assertIn("\nVersion: 1.1.0\n", metadata)
                self.assertIn("\nLicense-Expression: MIT\n", metadata)
                self.assertIn("uv tool install orekgames-track", metadata)

                tag = name.removesuffix(".whl").split("-", 2)[2]
                wheel_file = wheel.read(f"{info}/WHEEL").decode()
                self.assertIn(f"\nTag: {tag}\n", wheel_file)
                self.assertIn("\nRoot-Is-Purelib: false\n", wheel_file)

                # RECORD lists every file with its real hash and size.
                record = wheel.read(f"{info}/RECORD").decode().splitlines()
                self.assertEqual(record[-1], f"{info}/RECORD,,")
                self.assertEqual(len(record), len(names))
                for line in record[:-1]:
                    path, digest, size = line.split(",")
                    content = wheel.read(path)
                    expected = base64.urlsafe_b64encode(hashlib.sha256(content).digest()).rstrip(b"=")
                    self.assertEqual(digest, "sha256=" + expected.decode())
                    self.assertEqual(int(size), len(content))

    def test_rebuild_is_byte_identical(self):
        first = {path.name: path.read_bytes() for path in self.build()}
        second = {path.name: path.read_bytes() for path in self.build()}
        self.assertEqual(first, second)

    def test_wheels_of_an_older_build_are_replaced(self):
        self.build()
        make_release(self.work / "next", "1.2.0")
        self.release = self.work / "next"
        self.build(version="1.2.0", checksums=self.release / "checksums-sha256.txt")
        self.assertEqual(
            {path.name for path in self.out.iterdir()},
            {name.replace("1.1.0", "1.2.0") for name in EXPECTED_WHEELS},
        )

    def test_windows_zip_with_backslash_paths(self):
        target = "x86_64-pc-windows-msvc"
        path = self.release / f"track-1.1.0-{target}.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr(f"track-1.1.0-{target}\\track.exe", BINARIES[target])
        old = [line for line in self.checksums.read_text().splitlines() if path.name in line][0]
        self.rewrite_checksums(
            lambda text: text.replace(old, f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}")
        )
        self.assertEqual({p.name for p in self.build()}, EXPECTED_WHEELS)


class RefusalTests(ReleaseCase):
    def test_prerelease_version(self):
        self.assert_fails("prereleases are not published", version="1.1.0-rc.1")

    def test_version_lower_than_the_index_has(self):
        for published in ["1.1.1", "1.2.0", "1.10.0", "2.0.0"]:
            with self.subTest(published=published):
                self.assert_fails(f"refusing downgrade: the index already has orekgames-track {published}", index_version=published)

    def test_same_or_higher_version_than_the_index_has(self):
        # Equal is allowed: publishing the same version again completes a
        # partial upload. A project the index does not have yet is covered by
        # every other test, which run against an index without it.
        for published in ["1.1.0", "1.0.9", "0.9.0"]:
            with self.subTest(published=published):
                self.assertEqual(len(self.build(index_version=published)), 6)

    def test_unreadable_index_page(self):
        (self.work / "index.json").write_text("<html>not json</html>")
        self.assert_fails("cannot read file://")

    def test_missing_checksums_file(self):
        self.assert_fails("checksums file not found", checksums=self.work / "nope.txt")

    def test_missing_entry(self):
        self.rewrite_checksums(
            lambda text: "".join(l for l in text.splitlines(True) if "aarch64-pc-windows" not in l)
        )
        self.assert_fails("missing entries for:\n  track-1.1.0-aarch64-pc-windows-msvc.zip")

    def test_duplicate_entry(self):
        self.rewrite_checksums(lambda text: text + text.splitlines(True)[0])
        self.assert_fails("duplicate checksum entry")

    def test_malformed_line(self):
        # Uppercase hex is not what sha256sum writes.
        self.rewrite_checksums(lambda text: "A" * 64 + "  extra.tar.gz\n" + text)
        self.assert_fails("malformed checksums line")

    def test_entry_for_another_version(self):
        self.rewrite_checksums(lambda text: text + "a" * 64 + "  track-9.9.9-x86_64-pc-windows-msvc.zip\n")
        self.assert_fails("wrong version: 'track-9.9.9-x86_64-pc-windows-msvc.zip'")

    def test_unrelated_entries_are_ignored(self):
        self.rewrite_checksums(lambda text: text + "a" * 64 + "  track_1.1.0_amd64.deb\n")
        self.assertEqual(len(self.build()), 6)

    def test_checksum_mismatch(self):
        (self.release / "track-1.1.0-x86_64-apple-darwin.tar.gz").write_bytes(b"tampered")
        self.assert_fails("checksum mismatch for track-1.1.0-x86_64-apple-darwin.tar.gz")
        self.assertFalse((self.work / "archives").exists(), "downloads cleaned up")

    def test_archive_missing_from_release(self):
        (self.release / "track-1.1.0-x86_64-apple-darwin.tar.gz").unlink()
        self.assert_fails("failed to download")

    def test_archive_without_the_binary(self):
        target = "aarch64-apple-darwin"
        path = self.release / f"track-1.1.0-{target}.tar.gz"
        old = hashlib.sha256(path.read_bytes()).hexdigest()
        write_archive(path, f"track-1.1.0-{target}/README.md", b"no binary here")
        self.rewrite_checksums(lambda text: text.replace(old, hashlib.sha256(path.read_bytes()).hexdigest()))
        self.assert_fails(f"does not contain track-1.1.0-{target}/track")

    def test_binary_for_another_platform_than_the_archive_claims(self):
        binaries = dict(BINARIES, **{"aarch64-unknown-linux-gnu": make_elf(X86_64)})
        self.release = self.work / "swapped"
        self.assert_fails(
            "aarch64-unknown-linux-gnu: binary is for manylinux_2_17_x86_64",
            checksums=make_release(self.release, "1.1.0", binaries),
        )


class PlatformTagTests(unittest.TestCase):
    def test_manylinux_tag_is_the_highest_glibc_need(self):
        cases = [
            (("2.2.5", "2.39", "2.34"), "manylinux_2_39_x86_64"),
            (("2.28", "2.3.4"), "manylinux_2_28_x86_64"),
            # Nothing is tagged below manylinux2014.
            (("2.2.5", "2.3.4"), "manylinux_2_17_x86_64"),
        ]
        for glibc, tag in cases:
            with self.subTest(glibc=glibc):
                self.assertEqual(wheels.elf_platform_tag(make_elf(X86_64, glibc=glibc)), tag)
        self.assertEqual(wheels.elf_platform_tag(make_elf(AARCH64, glibc=("2.34",))), "manylinux_2_34_aarch64")

    def test_elf_refusals(self):
        cases = [
            (make_elf(X86_64, needed=("libc.so.6", "libssl.so.3")), "may not rely on: libssl.so.3"),
            (make_elf(X86_64, verneed=False), "no GLIBC version needs"),
            (make_elf(0x28), "unsupported ELF machine type 0x28"),
        ]
        for binary, message in cases:
            with self.subTest(message=message), self.assertRaises(wheels.BuildError) as caught:
                wheels.elf_platform_tag(binary)
            self.assertIn(message, str(caught.exception))

    def test_macos_tag_is_the_deployment_target(self):
        cases = [
            (make_macho(MACHO_ARM64, macos_version(11, 0)), "macosx_11_0_arm64"),
            # From macOS 11 on the minor version is not part of the tag.
            (make_macho(MACHO_ARM64, macos_version(12, 3)), "macosx_12_0_arm64"),
            (make_macho(MACHO_X86_64, macos_version(10, 12), build_version=False), "macosx_10_12_x86_64"),
        ]
        for binary, tag in cases:
            with self.subTest(tag=tag):
                self.assertEqual(wheels.macho_platform_tag(binary), tag)

    def test_windows_tag_is_the_machine_type(self):
        self.assertEqual(wheels.pe_platform_tag(make_pe(0x8664)), "win_amd64")
        self.assertEqual(wheels.pe_platform_tag(make_pe(0xAA64)), "win_arm64")
        with self.assertRaises(wheels.BuildError):
            wheels.pe_platform_tag(make_pe(0x014C))  # 32-bit x86

    def test_unreadable_binaries_are_errors_not_crashes(self):
        for binary in [b"#!/bin/sh\necho hi\n", b"\x7fELF\x02\x01", b"MZ", b""]:
            with self.subTest(binary=binary), self.assertRaises(wheels.BuildError):
                wheels.platform_tag(binary, "x86_64-unknown-linux-gnu")


class CommandLineTests(ReleaseCase):
    def run_script(self, *args):
        env = dict(
            os.environ,
            TRACK_ARCHIVE_URL_TEMPLATE=f"file://{self.release}/{{FILE}}",
            TRACK_DOWNLOAD_DIR=str(self.work / "archives"),
            TRACK_KEEP_DOWNLOADS="0",
        )
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--out", str(self.out), *args],
            env=env, capture_output=True, text=True,
        )

    def test_success_and_failure_exit_codes(self):
        result = self.run_script("1.1.0", str(self.checksums))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Built 6 wheels for orekgames-track 1.1.0", result.stdout)
        self.assertEqual({path.name for path in self.out.iterdir()}, EXPECTED_WHEELS)

        result = self.run_script("1.1.0-rc.1", str(self.checksums))
        self.assertEqual(result.returncode, 1)
        self.assertIn("Error: invalid version '1.1.0-rc.1'", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
