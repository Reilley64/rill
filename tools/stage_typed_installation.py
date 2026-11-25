"""Stage the first-party inputs for the LLVM-free M4.8 installed check on the host target."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PROFILE = next(
    line.split(": ", 1)[1]
    for line in subprocess.run(["rustc", "-vV"], check=True, capture_output=True, text=True)
    .stdout.splitlines()
    if line.startswith("host: ")
)
SOURCE_LISTS = [ROOT / "tests/installed/typed" / name
                for name in ("stdlib-sources.txt", "routine-sources.txt")]


def stage(prefix: Path) -> None:
    if prefix.exists():
        raise ValueError(f"installation prefix already exists: {prefix}")
    cargo = subprocess.run(
        ["cargo", "build", "--locked", "--no-default-features", "--target", PROFILE,
         "-p", "cli", "-p", "compiler", "--bin", "rill", "--example", "m4_8_installed",
         "--message-format=json-render-diagnostics"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    records = [json.loads(line) for line in cargo.stdout.splitlines()]
    executables = {record["target"]["name"]: Path(record["executable"])
                   for record in records if record.get("executable")}
    [compiler] = [record["package_id"] for record in records
                  if record["reason"] == "compiler-artifact"
                  and record["target"]["name"] == "compiler"
                  and "lib" in record["target"]["kind"]]
    [build] = [Path(record["out_dir"]) for record in records
               if record["reason"] == "build-script-executed"
               and record["package_id"] == compiler]
    runtime = f"lib/rill/runtime/{PROFILE}"
    inputs = {
        "bin/rill": executables["rill"],
        "bin/m4_8_installed": executables["m4_8_installed"],
        f"{runtime}/librill_ownership.rlib": build / "librill_ownership.rlib",
        f"{runtime}/error.rs": ROOT / "runtime/error/src/lib.rs",
    }
    for source_list in SOURCE_LISTS:
        for name in source_list.read_text(encoding="utf-8").splitlines():
            inputs[f"lib/rill/std/{name}"] = ROOT / "stdlib" / name
    for source in inputs.values():
        if not source.is_file():
            raise ValueError(f"missing distribution input: {source}")

    prefix.mkdir()
    for name, source in inputs.items():
        destination = prefix / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path, help="new installation directory")
    arguments = parser.parse_args()
    try:
        stage(arguments.prefix)
    except subprocess.CalledProcessError as failure:
        parser.exit(failure.returncode, failure.stderr + failure.stdout)
    except (OSError, ValueError) as failure:
        parser.exit(1, f"{failure}\n")
