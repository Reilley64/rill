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
# Pins for the trimmed archives that the "Release: Trimmed LLVM" workflow builds and release
# llvm-23.1.5-1 publishes: (asset, compressed size, SHA-256). A None size or digest is unpinned and setup refuses it.
ARTIFACTS = {
    "linux-x64": ("llvm-23.1.5-1-linux-x64.tar.xz", 54808368,
                  "e8cd21e7b8ec65b822c92a8b5be092991ccb7c956b5b639c554ffb87c7b9c7c8"),
    "linux-arm64": ("llvm-23.1.5-1-linux-arm64.tar.xz", 50311096,
                    "573a0b92d30d13bedc19fbe8f3641e7bb8c5ab46b667c2321f06a575fc3a9d0e"),
    "macos-arm64": ("llvm-23.1.5-1-macos-arm64.tar.xz", 37105012,
                    "94b54cadae2212ad418213ef879853c89c37ba78f3f5bc66044a1fa0793b145f"),
    "macos-x64": ("llvm-23.1.5-1-macos-x64.tar.xz", 41803336,
                  "eda7f3c02f73b2332ee243a56362ac37fd21af20441023e662092d305bed25be"),
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
    require(digest, f"checksum pin for {name} ({asset}) is unset in tools/llvm/common.py ARTIFACTS")


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
