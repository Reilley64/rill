"""Build the trimmed LLVM release archive for this host; CI runs it, setup.py consumes its output."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import time

import setup
from check import inspect
from common import ARTIFACTS, VERSION, backend_for, native_host, require

SOURCE_BASE = f"https://github.com/llvm/llvm-project/releases/download/llvmorg-{VERSION}/"
SOURCE_ARCHIVE = f"llvm-project-{VERSION}.src.tar.xz"
SOURCE_SHA256 = "fb50b1eac93ddeb3aa8dab4cfe324dff74f4a9c2bdee6160c9e36188f8d06a6e"
SOURCE_DIRS = ("llvm", "cmake", "third-party", "libc")
INSTALL_TARGETS = ("install-llvm-headers", "install-llvm-libraries", "install-llvm-config")


def flags(prefix, backend):
    """CMake options: one native backend, no projects, no optional native libraries, static only."""
    values = {
        "CMAKE_BUILD_TYPE": "Release", "CMAKE_INSTALL_PREFIX": prefix,
        "LLVM_TARGETS_TO_BUILD": backend, "LLVM_ENABLE_PROJECTS": "", "LLVM_ENABLE_RUNTIMES": "",
        "LLVM_BUILD_TOOLS": "ON", "LLVM_INCLUDE_TOOLS": "ON", "LLVM_INCLUDE_TESTS": "OFF",
        "LLVM_INCLUDE_EXAMPLES": "OFF", "LLVM_INCLUDE_BENCHMARKS": "OFF", "LLVM_INCLUDE_DOCS": "OFF",
        "LLVM_INCLUDE_UTILS": "OFF", "LLVM_ENABLE_BINDINGS": "OFF", "LLVM_ENABLE_ASSERTIONS": "OFF",
        "LLVM_ENABLE_ZLIB": "OFF", "LLVM_ENABLE_ZSTD": "OFF", "LLVM_ENABLE_LIBXML2": "OFF",
        "LLVM_ENABLE_LIBEDIT": "OFF", "LLVM_ENABLE_FFI": "OFF",
        "LLVM_BUILD_LLVM_DYLIB": "OFF", "LLVM_LINK_LLVM_DYLIB": "OFF",
    }
    return [f"-D{key}={value}" for key, value in values.items()]


def extract_source(archive, destination):
    """Stream only the directories the build needs, dropping the archive's root directory."""
    wanted = tuple(f"llvm-project-{VERSION}.src/{name}" for name in SOURCE_DIRS)
    destination.mkdir()
    with tarfile.open(archive, "r|xz") as source:
        for member in source:
            if member.name in wanted or member.name.startswith(tuple(w + "/" for w in wanted)):
                source.extract(member.replace(name=member.name.split("/", 1)[1], deep=False), destination, filter="data")
    for name in SOURCE_DIRS:
        require((destination / name).is_dir(), f"source archive lacks {name}/")


def build(work, prefix, backend):
    """Fetch the pinned source and install headers, static libraries and llvm-config into prefix."""
    setup.download(work / SOURCE_ARCHIVE, SOURCE_ARCHIVE, None, SOURCE_SHA256, base=SOURCE_BASE)
    extract_source(work / SOURCE_ARCHIVE, work / "src")
    (work / SOURCE_ARCHIVE).unlink()
    subprocess.run(["cmake", "-S", work / "src/llvm", "-B", work / "build", "-G", "Ninja",
                    *flags(prefix, backend)], check=True)
    subprocess.run(["ninja", "-C", work / "build", *INSTALL_TARGETS], check=True)


def anonymous(info):
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    return info


def package(prefix, archive):
    with tarfile.open(archive, "w:xz") as out:
        out.add(prefix, arcname=prefix.name, filter=anonymous)
    digest = hashlib.sha256()
    with archive.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    archive.with_name(archive.name + ".sha256").write_text(f"{digest.hexdigest()}  {archive.name}\n")
    return digest.hexdigest()


def verify(archive, name):
    """Extract the archive as setup.py would and inspect the result."""
    with tempfile.TemporaryDirectory(prefix="rill-llvm-verify-") as temp:
        tree = setup.extract(archive, Path(temp).resolve() / "unpacked")
        return inspect(tree, name, receipt=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help="directory for the archive and its .sha256 file; the build tree stays in OUT/work")
    parser.add_argument("--verify", type=Path, help="only verify this existing archive, without building")
    args = parser.parse_args()
    name = native_host()
    asset = ARTIFACTS[name][0]
    if args.verify:
        print(json.dumps(verify(args.verify.resolve(), name), indent=2))
        return
    require(args.out, "--out is required unless --verify is given")
    out = args.out.resolve()
    start = time.time()
    work = out / "work"
    prefix = work / "stage" / asset.removesuffix(".tar.xz")
    prefix.mkdir(parents=True)
    build(work, prefix, backend_for(name))
    digest = package(prefix, out / asset)
    facts = verify(out / asset, name)
    print(json.dumps({"asset": asset, "bytes": (out / asset).stat().st_size, "sha256": digest,
                      "seconds": round(time.time() - start), "backend": backend_for(name),
                      "targets": facts["targets"], "system_libraries": facts["system_libraries"]}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, tarfile.TarError) as error:
        raise SystemExit(f"LLVM release build failed: {error}")
