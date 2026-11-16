"""Explicit optional LLVM 23.1.5 provisioning; --plan performs no tool execution or I/O writes."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import urllib.request

from common import (ARTIFACTS, BASE, RECEIPT, ROOT, TARGETS, clean_env, evidence, inside,
                    native_host, no_fallback, pin, prefix_for, require, run)
from check import inspect

GIB = 1024 ** 3


def download(destination, asset, size, digest):
    hashed, count = hashlib.sha256(), 0
    with urllib.request.urlopen(BASE + asset, timeout=60) as response, destination.open("xb") as out:
        while True:
            block = response.read(1024 * 1024)
            if not block:
                break
            count += len(block)
            require(count <= (size or 2 * GIB), "archive exceeds compressed size limit")
            hashed.update(block)
            out.write(block)
    require(size is None or count == size, "archive compressed size mismatch")
    require(hashed.hexdigest() == digest, "archive SHA-256 mismatch")


def extract(archive, destination):
    """Stream into a new directory; defer links so none can redirect file writes."""
    destination.mkdir()
    links, total = [], 0
    with tarfile.open(archive, "r|xz") as source:
        for count, member in enumerate(source, 1):
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and ".." not in path.parts, "archive path traversal")
            target = destination / path
            require(inside(target, destination), "archive path escapes destination")
            total += member.size
            require(count <= 500000 and 0 <= member.size and total <= 40 * GIB, "archive extraction limit exceeded")
            if member.isfile():
                require(shutil.disk_usage(destination).free >= member.size + GIB, "insufficient extraction space (1 GiB reserve)")
            if member.issym() or member.islnk():
                require(not PurePosixPath(member.linkname).is_absolute(), "absolute archive link")
                linked = (target.parent if member.issym() else destination) / member.linkname
                require(inside(linked, destination), "archive link escapes destination")
                links.append((member, target, linked))
            elif member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                require(member.isfile(), "unsupported archive entry (device/fifo/etc)")
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.extractfile(member) as data, target.open("xb") as out:
                    shutil.copyfileobj(data, out)
                target.chmod(member.mode & 0o777)
    for member, target, linked in links:
        require(inside(target.parent, destination) and inside(linked, destination), "escaping archive link chain")
        target.parent.mkdir(parents=True, exist_ok=True)
        if member.issym():
            target.symlink_to(member.linkname)
        else:
            require(linked.is_file() and not linked.is_symlink(), "invalid hardlink target")
            os.link(linked, target)
    for _, target, _ in links:
        require(inside(target, destination), "escaping archive link chain")
    roots = list(destination.iterdir())
    require(len(roots) == 1 and roots[0].is_dir() and not roots[0].is_symlink(), "expected one archive root directory")
    return roots[0]


def source_flags(prefix, sdk, cc, cxx, ninja):
    values = {
        "CMAKE_BUILD_TYPE": "Release", "CMAKE_INSTALL_PREFIX": prefix,
        "CMAKE_C_COMPILER": cc, "CMAKE_CXX_COMPILER": cxx, "CMAKE_MAKE_PROGRAM": ninja,
        "CMAKE_OSX_SYSROOT": sdk, "CMAKE_OSX_ARCHITECTURES": "x86_64",
        "CMAKE_OSX_DEPLOYMENT_TARGET": "13.0", "CMAKE_CXX_STANDARD": "17",
        "Python3_EXECUTABLE": sys.executable, "Python_EXECUTABLE": sys.executable,
        "LLVM_ENABLE_PROJECTS": "clang;lld", "LLVM_ENABLE_RUNTIMES": "",
        "LLVM_TARGETS_TO_BUILD": ";".join(TARGETS), "LLVM_EXPERIMENTAL_TARGETS_TO_BUILD": "",
        "LLVM_ENABLE_ASSERTIONS": "ON", "BUILD_SHARED_LIBS": "OFF",
        "LLVM_BUILD_LLVM_DYLIB": "OFF", "LLVM_LINK_LLVM_DYLIB": "OFF", "LLVM_BUILD_LLVM_C_DYLIB": "OFF",
        "LLVM_INSTALL_TOOLCHAIN_ONLY": "OFF", "LLVM_INSTALL_UTILS": "ON",
        "LLVM_BUILD_TOOLS": "ON", "LLVM_INCLUDE_TOOLS": "ON",
        "LLVM_INCLUDE_TESTS": "OFF", "LLVM_INCLUDE_EXAMPLES": "OFF", "LLVM_INCLUDE_BENCHMARKS": "OFF",
        "LLVM_ENABLE_BINDINGS": "OFF", "LLVM_PARALLEL_LINK_JOBS": "1",
        "CMAKE_FIND_USE_PACKAGE_REGISTRY": "OFF", "CMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY": "OFF",
        "CMAKE_IGNORE_PREFIX_PATH": "/opt/homebrew;/usr/local;/home/linuxbrew/.linuxbrew",
    }
    # No auto-discovered optional native-library closure, including through Homebrew.
    for library in ["ZLIB", "ZSTD", "LIBXML2", "FFI", "LIBEDIT", "LIBPFM", "CURL", "ICU", "ICONV", "HTTPLIB", "Z3_SOLVER"]:
        values["LLVM_ENABLE_" + library] = "OFF"
    return [f"-D{key}={value}" for key, value in values.items()]


def source_prerequisites(args):
    for path in [args.sdk, args.cc, args.cxx]:
        require(path and path.is_absolute() and path.exists(), "source build requires absolute --sdk, --cc and --cxx paths")
        no_fallback(str(path.resolve()))
    require((args.sdk / "usr/lib/libSystem.tbd").is_file(), "--sdk is not a macOS SDK")
    for compiler in [args.cc, args.cxx]:
        require(compiler.is_file() and os.access(compiler, os.X_OK), "bootstrap compiler is not executable")
        no_fallback(run([compiler, "--version"]))
    # Task-scoped tools are on PATH but are not active in a nested `mise which`.
    # Compare them with explicit, repository-pinned mise installation roots instead.
    config = tomllib.loads((ROOT / "mise.toml").read_text())
    selected = config["tasks"]["setup:llvm"]["tools"]
    pins = {"cmake": ("aqua:Kitware/CMake", selected["aqua:Kitware/CMake"]),
            "ninja": ("aqua:ninja-build/ninja", selected["aqua:ninja-build/ninja"]),
            "python": ("core:python", config["tools"]["core:python"])}
    tools = {}
    for tool, (backend, version) in pins.items():
        root = Path(run(["mise", "where", f"{backend}@{version}"], env=dict(os.environ)))
        path = Path(shutil.which(tool) or "/missing-tool")
        require(root.is_dir() and path.is_file() and os.access(path, os.X_OK)
                and inside(path, root), f"select the pinned mise-installed {tool}, not a system fallback")
        no_fallback(str(path.resolve()))
        tools[tool] = path
    require(tools["python"].resolve() == Path(sys.executable).resolve(), "invoke setup with mise-provided Python")
    return tools


def provision(args, name, prefix):
    require(name == "macos-x64" or not args.build_source, "source route is only approved for native macOS x64")
    if prefix.exists():
        return inspect(prefix, name)  # Requires matching receipt AND repeats content/version checks.
    require(name != "macos-x64" or args.build_source, "macOS x64 requires explicit --build-source (no upstream binary exists)")
    prefix.parent.mkdir(parents=True, exist_ok=True)
    # Atomic mkdir lock; no stale lock or partial tree is automatically deleted/reused.
    lock = prefix.with_name(prefix.name + ".lock")
    lock.mkdir()
    try:
        source = name == "macos-x64"
        tools = source_prerequisites(args) if source else None
        require(1 <= args.jobs <= min(8, os.cpu_count() or 1), "--jobs must be between 1 and min(8, CPU count)")
        require(shutil.disk_usage(prefix.parent).free >= (80 if source else 48) * GIB, "insufficient free disk (80 GiB source / 48 GiB archive required)")
        if source:
            memory = int(run(["/usr/sbin/sysctl", "-n", "hw.memsize"]))
            require(memory >= max(8, args.jobs * 4) * GIB, "insufficient RAM (8 GiB minimum, 4 GiB/job); reduce --jobs")
        with tempfile.TemporaryDirectory(prefix=".llvm-stage-", dir=prefix.parent) as temp:
            work = Path(temp)
            download(work / "archive.xz", *ARTIFACTS[name])
            tree = extract(work / "archive.xz", work / "unpacked")
            if source:
                build, installed = work / "build", work / "install"
                flags = source_flags(prefix, args.sdk, args.cc, args.cxx, tools["ninja"])
                # DESTDIR retains the final configured prefix without writing into it during install.
                run([tools["cmake"], "-S", tree / "llvm", "-B", build, "-G", "Ninja", *flags], timeout=600)
                run([tools["cmake"], "--build", build, "--parallel", str(args.jobs)], timeout=86400)
                env = dict(clean_env(), DESTDIR=str(installed))
                run([tools["cmake"], "--install", build], timeout=1800, env=env)
                tree = installed / prefix.relative_to(prefix.anchor)
            require(not prefix.exists(), "output appeared during provisioning; refusing replacement")
            tree.rename(prefix)
            try:
                facts = inspect(prefix, name, receipt=False)
                if source:
                    facts["source_configuration"] = {"flags": flags, "jobs": args.jobs,
                                                     "tools": {key: str(value) for key, value in tools.items()}}
                (prefix / RECEIPT).write_text(json.dumps(pin(name), indent=2) + "\n")
            except BaseException:
                shutil.rmtree(prefix)  # Only the newly created tree owned by this invocation.
                raise
            return facts
    finally:
        lock.rmdir()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true")
    parser.add_argument("--build-source", action="store_true", help="explicit native macOS x64 route only")
    for option in ["sdk", "cc", "cxx"]:
        parser.add_argument("--" + option, type=Path)
    parser.add_argument("--jobs", type=int, default=2, help="source parallelism; default 2, max 8, one link job")
    args = parser.parse_args()
    # Plan is pure selection: do not invoke sysctl or any installed LLVM/build tools.
    if args.plan:
        from common import host
        name = host()
        facts = evidence("plan-only", **pin(name), prefix=str(prefix_for(name)),
                         url=BASE + ARTIFACTS[name][0], source_requires="--build-source --sdk PATH --cc PATH --cxx PATH; mise CMake/Ninja/Python",
                         source_flags=source_flags(prefix_for(name), args.sdk or "<SDK>", args.cc or "<CC>", args.cxx or "<CXX>", "<mise-ninja>") if name == "macos-x64" else [],
                         c_api_smoke="not-run", provenance_signatures="unverified")
    else:
        name = native_host()
        facts = provision(args, name, prefix_for(name))
    print(json.dumps(facts, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, tarfile.TarError) as error:
        raise SystemExit(f"LLVM setup failed: {error}")
