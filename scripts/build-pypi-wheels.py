#!/usr/bin/env python3
"""Build the PyPI wheels for a published track release.

Usage: scripts/build-pypi-wheels.py [--out <dir>] [--index-json <url>] <version> <checksums-file>

--out         directory to write the wheels to (default: dist/pypi-wheels).
--index-json  the package index's JSON page for the project, for example
              https://pypi.org/pypi/orekgames-track/json. When given, a version
              lower than the one the index already has is refused.

The wheels repackage the binaries of the GitHub release: nothing is compiled
here, so PyPI users run the same bytes as everyone else. Each wheel is
`orekgames_track-<version>-py3-none-<platform>.whl` and holds one file that
matters, the binary under `<name>.data/scripts/`, which installers (pip, uv,
pipx) copy onto the user's PATH as `track`.

The checksums file must contain lines like:
  abc123...  track-1.19.3-aarch64-apple-darwin.tar.gz

Safety guarantees (the same as scripts/update-homebrew-formula.sh):
  * Exactly one valid SHA256 entry is required for each of the six release
    archives, matched on the full filename. Duplicate, missing, malformed, or
    wrong-version entries are fatal errors.
  * Every archive is downloaded and its bytes verified against the checksums
    before any wheel is written; on any failure no wheel is left behind.
  * Version downgrades (relative to --index-json) are refused.
  * The platform tag of each wheel is read from the binary itself (glibc
    version needs, macOS deployment target, PE machine type), never assumed,
    and a Linux binary that links a library outside the allow-list is refused.
  * Output is deterministic: the same release produces byte-identical wheels.

Environment (shared with update-homebrew-formula.sh):
  TRACK_DOWNLOAD_DIR          where archives are downloaded (default dist/pypi-archives)
  TRACK_ARCHIVE_URL_TEMPLATE  download URL with {VERSION} and {FILE} placeholders
  TRACK_KEEP_DOWNLOADS        set to 1 to keep the downloaded archives
"""

import argparse
import base64
import contextlib
import hashlib
import io
import json
import os
import re
import shutil
import struct
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

PROJECT = "orekgames-track"
DIST = "orekgames_track"
REPO_URL = "https://github.com/OrekGames/track-cli"
DOCS_URL = "https://orekgames.github.io/track-cli/"
DEFAULT_URL_TEMPLATE = REPO_URL + "/releases/download/v{VERSION}/{FILE}"

# Release target -> (archive extension, binary name inside the archive).
TARGETS = {
    "aarch64-apple-darwin": ("tar.gz", "track"),
    "x86_64-apple-darwin": ("tar.gz", "track"),
    "aarch64-unknown-linux-gnu": ("tar.gz", "track"),
    "x86_64-unknown-linux-gnu": ("tar.gz", "track"),
    "aarch64-pc-windows-msvc": ("zip", "track.exe"),
    "x86_64-pc-windows-msvc": ("zip", "track.exe"),
}

# Libraries a manylinux wheel may link without bundling them. This is a
# subset of the manylinux policy (PEP 600), limited to what a CLI needs, so
# an unexpected new dependency fails the build instead of the user's install.
MANYLINUX_ALLOWED_LIBS = {
    "libc.so.6",
    "libm.so.6",
    "libdl.so.2",
    "librt.so.1",
    "libpthread.so.0",
    "libutil.so.1",
    "libresolv.so.2",
    "libgcc_s.so.1",
    "libz.so.1",
    "ld-linux-x86-64.so.2",
    "ld-linux-aarch64.so.1",
}
# No manylinux tag is lower than this (it is manylinux2014).
MANYLINUX_FLOOR = (2, 17)

# Zip timestamps are fixed so that wheels are reproducible.
ZIP_DATE = (1980, 1, 1, 0, 0, 0)


class BuildError(Exception):
    """A problem that must stop the build; the message is shown to the user."""


# --- Checksums ---------------------------------------------------------------


def archive_name(version, target):
    return f"track-{version}-{target}.{TARGETS[target][0]}"


def parse_checksums(text, version):
    """Return {target: sha256} for the six archives of `version`."""
    entries = {}
    for line in text.splitlines():
        if not line:
            continue
        match = re.fullmatch(r"([a-f0-9]{64})\s+(\S+)\s*", line)
        if not match:
            raise BuildError(f"malformed checksums line: '{line}'")
        sha, filename = match.groups()
        if filename in entries:
            raise BuildError(f"duplicate checksum entry for '{filename}'")
        entries[filename] = sha

    expected = {archive_name(version, target): target for target in TARGETS}
    missing = sorted(name for name in expected if name not in entries)
    if missing:
        raise BuildError(
            "checksums file is missing entries for:\n  "
            + "\n  ".join(missing)
            + "\nFound entries:\n  "
            + "\n  ".join(sorted(entries))
        )

    # An entry for one of our platforms at another version means this is the
    # checksums file of a different release.
    for filename in entries:
        if filename in expected:
            continue
        for target, (ext, _) in TARGETS.items():
            if re.fullmatch(rf"track-.*-{re.escape(target)}\.{re.escape(ext)}", filename):
                raise BuildError(
                    f"checksums file contains entry for wrong version: '{filename}' "
                    f"(expected version {version})"
                )

    return {target: entries[name] for name, target in expected.items()}


# --- Downgrade guard ---------------------------------------------------------


def version_on_index(index_json_url):
    """The project's latest version on the index, or None if it has none yet."""
    try:
        with urllib.request.urlopen(index_json_url) as response:
            version = json.load(response)["info"]["version"]
    except urllib.error.HTTPError as err:
        if err.code == 404:
            return None
        raise BuildError(f"cannot read {index_json_url}: {err}") from err
    except (OSError, ValueError, KeyError, TypeError) as err:
        if isinstance(getattr(err, "reason", None), FileNotFoundError):
            return None  # a file:// index without the project
        raise BuildError(f"cannot read {index_json_url}: {err}") from err
    if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise BuildError(f"unexpected version '{version}' at {index_json_url}")
    return version


def refuse_downgrade(version, index_json_url):
    published = version_on_index(index_json_url)
    if published is None:
        print(f"{PROJECT} is not on the index yet; {version} will be its first version.")
        return
    if [int(part) for part in version.split(".")] < [int(part) for part in published.split(".")]:
        raise BuildError(
            f"refusing downgrade: the index already has {PROJECT} {published}, "
            f"refusing to publish {version}"
        )
    print(f"The index has {PROJECT} {published}; {version} is not a downgrade.")


# --- Download and extraction -------------------------------------------------


def download(url, destination):
    try:
        with urllib.request.urlopen(url) as response, open(destination, "wb") as out:
            shutil.copyfileobj(response, out)
    except OSError as err:
        raise BuildError(f"failed to download {url}: {err}") from err


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def extract_binary(archive_path, version, target):
    """Return the bytes of the track binary inside a release archive."""
    ext, binary = TARGETS[target]
    member = f"track-{version}-{target}/{binary}"
    try:
        if ext == "zip":
            with zipfile.ZipFile(archive_path) as archive:
                # Compress-Archive on Windows may write backslash separators.
                names = {name.replace("\\", "/"): name for name in archive.namelist()}
                if member not in names:
                    raise KeyError(member)
                return archive.read(names[member])
        with tarfile.open(archive_path, "r:gz") as archive:
            handle = archive.extractfile(archive.getmember(member))
            if handle is None:
                raise KeyError(member)
            return handle.read()
    except KeyError:
        raise BuildError(f"{archive_path.name} does not contain {member}") from None
    except (tarfile.TarError, zipfile.BadZipFile, OSError) as err:
        raise BuildError(f"cannot read {archive_path.name}: {err}") from err


# --- Platform tags, read from the binary -------------------------------------

ELF_MACHINES = {0x3E: "x86_64", 0xB7: "aarch64"}
MACHO_CPUS = {0x01000007: "x86_64", 0x0100000C: "arm64"}
PE_MACHINES = {0x8664: "win_amd64", 0xAA64: "win_arm64"}

SHT_DYNAMIC = 6
SHT_GNU_VERNEED = 0x6FFFFFFE
DT_NEEDED = 1
LC_VERSION_MIN_MACOSX = 0x24
LC_BUILD_VERSION = 0x32
MACHO_PLATFORM_MACOS = 1


def _cstring(data, offset):
    end = data.index(b"\0", offset)
    return data[offset:end].decode("ascii")


def elf_platform_tag(data):
    """manylinux tag for a 64-bit little-endian glibc ELF binary."""
    if data[4:6] != b"\x02\x01":
        raise BuildError("unsupported ELF binary (need 64-bit little-endian)")
    machine = struct.unpack_from("<H", data, 18)[0]
    arch = ELF_MACHINES.get(machine)
    if arch is None:
        raise BuildError(f"unsupported ELF machine type {machine:#x}")

    shoff = struct.unpack_from("<Q", data, 40)[0]
    shentsize, shnum = struct.unpack_from("<HH", data, 58)
    sections = [
        struct.unpack_from("<IIQQQQIIQQ", data, shoff + index * shentsize)
        for index in range(shnum)
    ]

    needed = []
    glibc = None
    for _, sh_type, _, _, offset, size, link, info, _, _ in sections:
        if sh_type == SHT_DYNAMIC:
            strtab = sections[link][4]
            for entry in range(offset, offset + size, 16):
                tag, value = struct.unpack_from("<qQ", data, entry)
                if tag == DT_NEEDED:
                    needed.append(_cstring(data, strtab + value))
        elif sh_type == SHT_GNU_VERNEED:
            strtab = sections[link][4]
            need = offset
            for _ in range(info):
                _, count, _, aux, next_need = struct.unpack_from("<HHIII", data, need)
                entry = need + aux
                for _ in range(count):
                    _, _, _, name, next_aux = struct.unpack_from("<IHHII", data, entry)
                    match = re.fullmatch(r"GLIBC_(\d+)\.(\d+)(?:\.\d+)?", _cstring(data, strtab + name))
                    if match:
                        found = (int(match[1]), int(match[2]))
                        glibc = max(glibc, found) if glibc else found
                    entry += next_aux
                need += next_need

    if glibc is None:
        raise BuildError(
            "cannot derive a manylinux tag: the binary has no GLIBC version needs "
            "(static and musl builds are not supported yet)"
        )
    unexpected = sorted(set(needed) - MANYLINUX_ALLOWED_LIBS)
    if unexpected:
        raise BuildError(
            "binary links libraries a manylinux wheel may not rely on: " + ", ".join(unexpected)
        )

    major, minor = max(glibc, MANYLINUX_FLOOR)
    return f"manylinux_{major}_{minor}_{arch}"


def macho_platform_tag(data):
    """macosx tag for a 64-bit Mach-O binary, from its deployment target."""
    cpu, _, _, ncmds = struct.unpack_from("<iiII", data, 4)
    arch = MACHO_CPUS.get(cpu & 0xFFFFFFFF)
    if arch is None:
        raise BuildError(f"unsupported Mach-O cpu type {cpu & 0xFFFFFFFF:#x}")

    offset = 32
    minimum = None
    for _ in range(ncmds):
        command, size = struct.unpack_from("<II", data, offset)
        if command == LC_BUILD_VERSION:
            platform, version = struct.unpack_from("<II", data, offset + 8)
            if platform == MACHO_PLATFORM_MACOS:
                minimum = version
        elif command == LC_VERSION_MIN_MACOSX:
            minimum = struct.unpack_from("<I", data, offset + 8)[0]
        offset += size
    if minimum is None:
        raise BuildError("Mach-O binary does not declare a minimum macOS version")

    major, minor = minimum >> 16, (minimum >> 8) & 0xFF
    # From macOS 11 on, installers match on the major version only.
    if major >= 11:
        minor = 0
    return f"macosx_{major}_{minor}_{arch}"


def pe_platform_tag(data):
    """win tag for a PE binary, from its machine type."""
    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe_offset : pe_offset + 4] != b"PE\0\0":
        raise BuildError("not a PE binary")
    machine = struct.unpack_from("<H", data, pe_offset + 4)[0]
    tag = PE_MACHINES.get(machine)
    if tag is None:
        raise BuildError(f"unsupported PE machine type {machine:#x}")
    return tag


def platform_tag(data, target):
    """The wheel platform tag for `data`, which must be a binary for `target`."""
    try:
        if data[:4] == b"\x7fELF":
            tag, system = elf_platform_tag(data), "linux"
        elif data[:4] == b"\xcf\xfa\xed\xfe":
            tag, system = macho_platform_tag(data), "apple"
        elif data[:2] == b"MZ":
            tag, system = pe_platform_tag(data), "windows"
        else:
            raise BuildError("not an ELF, Mach-O or PE binary")
    except (struct.error, ValueError, IndexError) as err:
        raise BuildError(f"{target}: cannot parse the binary: {err}") from err
    except BuildError as err:
        raise BuildError(f"{target}: {err}") from None

    # The binary must be what the archive name says it is.
    arch = target.split("-")[0]
    tag_arch = {"amd64": "x86_64", "arm64": "aarch64"}
    actual = tag.rsplit("_", 1)[1] if system == "windows" else tag.split("_", 3)[3]
    if system not in target or tag_arch.get(actual, actual) != arch:
        raise BuildError(f"{target}: binary is for {tag}, which does not match the archive")
    return tag


# --- Wheel -------------------------------------------------------------------


def metadata(version, summary, readme):
    return (
        "Metadata-Version: 2.4\n"
        f"Name: {PROJECT}\n"
        f"Version: {version}\n"
        f"Summary: {summary}\n"
        f"Project-URL: Homepage, {REPO_URL}\n"
        f"Project-URL: Documentation, {DOCS_URL}\n"
        f"Project-URL: Repository, {REPO_URL}\n"
        f"Project-URL: Issues, {REPO_URL}/issues\n"
        f"Project-URL: Releases, {REPO_URL}/releases\n"
        "License-Expression: MIT\n"
        "License-File: LICENSE\n"
        "Keywords: issue-tracker,cli,youtrack,jira,github,gitlab,linear\n"
        "Classifier: Development Status :: 5 - Production/Stable\n"
        "Classifier: Environment :: Console\n"
        "Classifier: Intended Audience :: Developers\n"
        "Classifier: Operating System :: MacOS\n"
        "Classifier: Operating System :: Microsoft :: Windows\n"
        "Classifier: Operating System :: POSIX :: Linux\n"
        "Classifier: Programming Language :: Rust\n"
        "Classifier: Topic :: Software Development :: Bug Tracking\n"
        "Requires-Python: >=3.8\n"
        "Description-Content-Type: text/markdown\n"
        "\n"
        f"{readme}"
    )


def build_wheel(version, tag, binary_name, binary, summary, readme, license_text):
    """Return (filename, bytes) of the wheel for one platform."""
    dist_info = f"{DIST}-{version}.dist-info"
    wheel_file = (
        "Wheel-Version: 1.0\n"
        "Generator: track-cli build-pypi-wheels\n"
        "Root-Is-Purelib: false\n"
        f"Tag: py3-none-{tag}\n"
    )
    # (path, content, mode); RECORD is appended below and must come last.
    files = [
        (f"{DIST}-{version}.data/scripts/{binary_name}", binary, 0o755),
        (f"{dist_info}/METADATA", metadata(version, summary, readme).encode(), 0o644),
        (f"{dist_info}/WHEEL", wheel_file.encode(), 0o644),
        (f"{dist_info}/licenses/LICENSE", license_text.encode(), 0o644),
    ]
    record = "".join(
        "{},sha256={},{}\n".format(
            path,
            base64.urlsafe_b64encode(hashlib.sha256(content).digest()).rstrip(b"=").decode(),
            len(content),
        )
        for path, content, _ in files
    )
    record += f"{dist_info}/RECORD,,\n"
    files.append((f"{dist_info}/RECORD", record.encode(), 0o644))

    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as wheel:
        for path, content, mode in files:
            info = zipfile.ZipInfo(path, ZIP_DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3  # Unix, so the mode below is honoured
            info.external_attr = (0o100000 | mode) << 16
            wheel.writestr(info, content)
    return f"{DIST}-{version}-py3-none-{tag}.whl", buffer.getvalue()


# --- Main --------------------------------------------------------------------


def cargo_description(cargo_toml):
    match = re.search(r'^description = "(.*)"$', cargo_toml.read_text(), re.MULTILINE)
    if not match:
        raise BuildError(f"no description found in {cargo_toml}")
    return match[1]


def build_wheels(
    version, checksums_file, out_dir, repo_root, download_dir, url_template, keep, index_json_url=None
):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise BuildError(
            f"invalid version '{version}' (expected MAJOR.MINOR.PATCH; "
            "prereleases are not published to PyPI)"
        )
    if not checksums_file.is_file():
        raise BuildError(f"checksums file not found: {checksums_file}")

    if index_json_url:
        refuse_downgrade(version, index_json_url)

    checksums = parse_checksums(checksums_file.read_text(), version)
    summary = cargo_description(repo_root / "crates/track/Cargo.toml")
    readme = (repo_root / "pypi/README.md").read_text()
    license_text = (repo_root / "LICENSE").read_text()

    print(f"Checksums validated for version {version}:")
    for target, sha in checksums.items():
        print(f"  {target + ':':<27} {sha}")

    download_dir.mkdir(parents=True, exist_ok=True)
    archives = {}
    try:
        # Download and verify every archive before building anything.
        for target, expected in checksums.items():
            name = archive_name(version, target)
            path = archives[target] = download_dir / name
            print(f"Downloading {name} ...")
            download(url_template.replace("{VERSION}", version).replace("{FILE}", name), path)
            actual = sha256_file(path)
            if actual != expected:
                raise BuildError(
                    f"checksum mismatch for {name}:\n  expected {expected}\n  actual   {actual}"
                )
            print(f"  verified SHA256: {actual}")

        wheels = []
        for target, path in archives.items():
            binary = extract_binary(path, version, target)
            tag = platform_tag(binary, target)
            wheels.append(
                build_wheel(version, tag, TARGETS[target][1], binary, summary, readme, license_text)
            )
    finally:
        if not keep:
            # Remove what was downloaded, and the directory if nothing else is in it.
            for path in archives.values():
                path.unlink(missing_ok=True)
            with contextlib.suppress(OSError):
                download_dir.rmdir()

    # Write all wheels or none: build them in a scratch directory first.
    out_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=out_dir) as scratch:
        for filename, content in wheels:
            Path(scratch, filename).write_bytes(content)
        for stale in out_dir.glob(f"{DIST}-*.whl"):
            stale.unlink()
        for filename, _ in wheels:
            os.replace(Path(scratch, filename), out_dir / filename)

    print(f"Built {len(wheels)} wheels for {PROJECT} {version} in {out_dir}:")
    for filename, content in sorted(wheels):
        print(f"  {filename}  ({len(content)} bytes)")
    return [out_dir / filename for filename, _ in wheels]


def main(argv=None):
    parser = argparse.ArgumentParser(description="Build the PyPI wheels for a track release.")
    parser.add_argument("--out", default="dist/pypi-wheels", help="directory to write the wheels to")
    parser.add_argument("--index-json", help="index JSON page for the project; refuse a downgrade")
    parser.add_argument("version", help="release version, e.g. 1.19.3")
    parser.add_argument("checksums_file", help="the release's checksums-sha256.txt")
    args = parser.parse_args(argv)

    try:
        build_wheels(
            version=args.version,
            checksums_file=Path(args.checksums_file),
            out_dir=Path(args.out),
            repo_root=Path(__file__).resolve().parent.parent,
            download_dir=Path(os.environ.get("TRACK_DOWNLOAD_DIR", "dist/pypi-archives")),
            url_template=os.environ.get("TRACK_ARCHIVE_URL_TEMPLATE", DEFAULT_URL_TEMPLATE),
            keep=os.environ.get("TRACK_KEEP_DOWNLOADS", "0") == "1",
            index_json_url=args.index_json,
        )
    except BuildError as err:
        print(f"Error: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
