"""Pins and small shared checks for the explicit LLVM provisioning commands."""
import json
import os
from pathlib import Path
import platform
import re
import struct
import subprocess

VERSION = "23.1.5"
RELEASE = "llvm-23.1.5-1"
ROOT = Path(__file__).resolve().parents[2]
BASE = f"https://github.com/Reilley64/rill/releases/download/{RELEASE}/"
# Pins for the trimmed archives that the "Release: Trimmed LLVM" workflow builds. The release
# does not exist yet: None marks a size or SHA-256 not yet pinned, and setup refuses it.
ARTIFACTS = {
    host: (f"llvm-{VERSION}-1-{host}.tar.xz", None, None)
    for host in ["linux-x64", "linux-arm64", "macos-arm64", "macos-x64"]
}
BACKENDS = {"x64": "X86", "arm64": "AArch64"}
RECEIPT = ".rill-llvm.json"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def host(system=None, machine=None):
    system, machine = system or platform.system(), machine or platform.machine()
    arch = {"x86_64": "x64", "AMD64": "x64", "aarch64": "arm64", "arm64": "arm64"}.get(machine)
    name = {"Linux": "linux", "Darwin": "macos"}.get(system)
    require(name and arch and struct.calcsize("P") == 8, f"unsupported host: {system}/{machine}")
    return f"{name}-{arch}"


def native_host():
    result = host()
    if result.startswith("linux"):
        require(platform.libc_ver()[0] == "glibc", "the Linux archives are built on glibc; no musl fallback")
    if result == "macos-x64":
        translated = subprocess.run(["/usr/sbin/sysctl", "-in", "sysctl.proc_translated"],
                                    capture_output=True, text=True, check=False).stdout.strip()
        require(translated != "1", "Rosetta is not a native x64 provisioning host")
    return result


def prefix_for(name):
    prefix = ROOT / "target" / "toolchains" / f"llvm-{VERSION}" / name
    require(prefix.resolve() == prefix, f"symlinked output path: {prefix}")
    return prefix


def backend_for(name):
    return BACKENDS[name.split("-")[1]]


def pin(name):
    asset, _, digest = ARTIFACTS[name]
    return {"version": VERSION, "host": name, "asset": asset, "sha256": digest, "route": "release-archive"}


def require_pinned(name):
    asset, _, digest = ARTIFACTS[name]
    require(digest, f"checksum pin for {name} ({asset}) is unset in tools/llvm/common.py ARTIFACTS; "
                    f"release {RELEASE} is not published yet")


def evidence(status, **facts):
    return dict(status=status, inkwell="unverified", binding_libffi="unverified",
                rill_codegen="unverified", **facts)


def clean_env():
    env = dict(os.environ)
    for key in list(env):
        if key.startswith(("DYLD_", "LD_", "CMAKE_", "PKG_CONFIG")) or key in {
            "CC", "CXX", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "CPATH",
            "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH", "LIBRARY_PATH", "LD_LIBRARY_PATH", "LD_PRELOAD",
            "SDKROOT", "MACOSX_DEPLOYMENT_TARGET", "CCC_OVERRIDE_OPTIONS", "CLANG_CONFIG_FILE_SYSTEM_DIR",
            "CLANG_CONFIG_FILE_USER_DIR", "GCC_EXEC_PREFIX", "COMPILER_PATH", "DESTDIR"}:
            env.pop(key)
    env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    env["LC_ALL"] = "C"
    return env


def run(args, *, timeout=60, env=None, combined=False):
    result = subprocess.run([str(a) for a in args], env=clean_env() if env is None else env,
                            text=True, capture_output=True, timeout=timeout, check=True)
    return (result.stdout + ("\n" + result.stderr if combined else "")).strip()


def no_fallback(text):
    require(not re.search(r"/opt/homebrew|/usr/local|/[^\s]*linuxbrew|/Cellar/|/[^\s]*conda", text, re.I),
            f"forbidden fallback path: {text}")
    require(not re.search(r"lib(?:LLVM|clang)[^\s/]*(?:\.so|\.dylib)|-lLLVM", text, re.I),
            f"shared LLVM fallback: {text}")


def inside(path, root):
    try:
        return path.resolve().is_relative_to(root.resolve())
    except RuntimeError:  # Symlink cycle, not a contained path.
        return False


def read_receipt(prefix, name):
    receipt = prefix / RECEIPT
    require(receipt.is_file() and not receipt.is_symlink(), "missing provisioning receipt; refusing partial/arbitrary prefix")
    require(json.loads(receipt.read_text()) == pin(name), "provisioning receipt does not match pinned route/host")
