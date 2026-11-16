"""Verify the mise-selected base tools. This is not a hermetic-build claim."""
from pathlib import Path
import os
import platform
import re
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def capture(*args):
    return subprocess.check_output(args, text=True, cwd=ROOT).strip()


def check():
    config = tomllib.loads((ROOT / "mise.toml").read_text())
    tools = config["tools"]
    rust = tools["core:rust"]["version"]
    python = tools["core:python"]
    uv = tools["aqua:astral-sh/uv"]
    if (platform.system(), platform.machine()) not in {
        ("Darwin", "arm64"), ("Darwin", "x86_64"),
        ("Linux", "aarch64"), ("Linux", "x86_64"),
    }:
        raise ValueError("The scaffold supports macOS/Linux x86-64/AArch64 hosts only")
    mise_version = capture("mise", "--version").split()[0]
    version = lambda text: tuple(map(int, text.split(".")))
    if version(mise_version) < version(config["min_version"]):
        raise ValueError(f"mise >= {config['min_version']} is required; global mise was not changed")
    if platform.python_version() != python:
        raise ValueError(f"Use mise's Python {python}, not {platform.python_version()}")
    expected_python = Path(capture("mise", "where", "core:python@" + python)).resolve()
    if not Path(sys.executable).resolve().is_relative_to(expected_python):
        raise ValueError("Python is outside the selected mise installation")
    if not re.match(r"rustc " + re.escape(rust) + r"\s", capture("rustc", "--version")):
        raise ValueError(f"Rust {rust} is required; check user-global mise overrides")
    # Mise's Rust install path can point to rustup proxies, not the sysroot.
    expected_rustc = Path(capture("rustup", "which", "--toolchain", rust, "rustc")).resolve()
    actual_rustc = Path(capture("rustc", "--print", "sysroot")) / "bin/rustc"
    if actual_rustc.resolve() != expected_rustc:
        raise ValueError("Rust is outside the selected mise/rustup toolchain")
    if not capture("cargo", "--version").startswith("cargo " + rust + " "):
        raise ValueError("Cargo does not match the selected Rust toolchain")
    if not capture("uv", "--version").startswith("uv " + uv + " "):
        raise ValueError(f"uv {uv} is required; check user-global mise overrides")
    selected_tools = {
        "rustc": "core:rust@" + rust,
        "cargo": "core:rust@" + rust,
        "uv": "aqua:astral-sh/uv@" + uv,
    }
    for tool, selected in selected_tools.items():
        # The explicit selector avoids a mise 2026.11 macOS/AArch64 shim bug where
        # `mise which rustc` reports an installed configured tool as inactive.
        expected = Path(capture("mise", "which", tool, "--tool", selected)).resolve()
        actual = Path(shutil.which(tool) or "/missing-tool").resolve()
        if actual != expected:
            raise ValueError(f"{tool} is outside the selected mise environment")
    overrides = [key for key in ("RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER") if os.environ.get(key)]
    if overrides:
        raise ValueError("Unreviewed Cargo compiler overrides: " + ", ".join(overrides))
    print(f"Base tools verified: Rust {rust}, Python {python}, uv {uv}; no project LLVM required.")


if __name__ == "__main__":
    try:
        check()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Tool preflight failed: {error}", file=sys.stderr)
        raise SystemExit(1)
