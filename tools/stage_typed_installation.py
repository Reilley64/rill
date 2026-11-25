"""Stage the first-party inputs of an installed toolchain on the host target: the LLVM-free M4.8 check by default, the building `rill` with --llvm."""
import argparse
import json
import os
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
                for name in ("stdlib-sources.txt", "routine-sources.txt", "append-sources.txt")]


def stage(prefix: Path, llvm: bool = False) -> None:
    if prefix.exists():
        raise ValueError(f"installation prefix already exists: {prefix}")
    if llvm and not os.environ.get("LLVM_SYS_231_PREFIX"):
        raise ValueError("LLVM_SYS_231_PREFIX must name the LLVM prefix that mise run setup:llvm installs")
    selection = ["--lib", "--bin", "rill"]
    selection += [] if llvm else ["--no-default-features", "--example", "m4_8_installed"]
    cargo = subprocess.run(
        ["cargo", "build", "--locked", "--target", PROFILE,
         "-p", "cli", "-p", "compiler", "-p", "rill-runtime", *selection,
         "--message-format=json-render-diagnostics"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    records = [json.loads(line) for line in cargo.stdout.splitlines()]
    executables = {record["target"]["name"]: Path(record["executable"])
                   for record in records if record.get("executable")}
    [runtime_archive] = [Path(name) for record in records
                         if record["reason"] == "compiler-artifact"
                         and record["target"]["name"] == "rill_runtime"
                         for name in record["filenames"] if name.endswith("librill_runtime.a")]
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
        f"{runtime}/librill_runtime.a": runtime_archive,
        f"{runtime}/librill_ownership.rlib": build / "librill_ownership.rlib",
        f"{runtime}/error.rs": ROOT / "runtime/error/src/lib.rs",
    }
    if not llvm:
        inputs["bin/m4_8_installed"] = executables["m4_8_installed"]
    for source_list in SOURCE_LISTS:
        for name in source_list.read_text(encoding="utf-8").splitlines():
            inputs[f"lib/rill/std/{name}"] = ROOT / "stdlib" / name
    for source in inputs.values():
        if not source.is_file():
            raise ValueError(f"missing distribution input: {source}")

    prefix.mkdir(parents=True)
    for name, source in inputs.items():
        destination = prefix / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path, help="new installation directory")
    parser.add_argument("--llvm", action="store_true",
                        help="build rill with the llvm feature and stage no M4.8 runner; needs LLVM_SYS_231_PREFIX")
    arguments = parser.parse_args()
    try:
        stage(arguments.prefix, arguments.llvm)
    except subprocess.CalledProcessError as failure:
        parser.exit(failure.returncode, failure.stderr + failure.stdout)
    except (OSError, ValueError) as failure:
        parser.exit(1, f"{failure}\n")
