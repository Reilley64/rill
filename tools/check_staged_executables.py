"""Refuse a Rust test that copies an executable instead of staging it through rill-test-support.

`fs::copy` of `current_exe()` or a `CARGO_BIN_EXE_*` binary leaves a writable descriptor that a
concurrent fork can inherit, so a later spawn of the copy fails with ETXTBSY on Linux.
`rill_test_support::stage_executable` and `stage_current_exe` are the only sanctioned way to stage one.
"""
import argparse
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIRECTORIES = ["crates", "runtime", "tools"]
HELPER_DIRECTORY = "crates/test-support/"
EXECUTABLE = re.compile(r"current_exe\b|CARGO_BIN_EXE_")
BINDING = re.compile(r"\blet\s+(?:mut\s+)?(\w+)\s*(?::[^=;]+)?=([^;]*)", re.S)
MESSAGE = (
    "Stage the executable with rill_test_support::stage_executable or stage_current_exe "
    "and spawn it with rill_test_support::spawn_staged or run_staged (crates/test-support)."
)


def first_argument(source, start):
    depth = 0
    for index in range(start, len(source)):
        character = source[index]
        if character in "([{":
            depth += 1
        elif character in ")]}":
            if depth == 0:
                return source[start:index]
            depth -= 1
        elif character == "," and depth == 0:
            return source[start:index]
    return source[start:]


def check_source(source):
    """Return the 1-based lines of `fs::copy` calls whose source is an executable."""
    executables = {
        match.group(1) for match in BINDING.finditer(source) if EXECUTABLE.search(match.group(2)) and "{" not in match.group(2)
    }
    lines = []
    for match in re.finditer(r"\b(?:std::)?fs::copy\s*\(", source):
        argument = first_argument(source, match.end())
        name = re.fullmatch(r"\s*&?\s*(\w+)\s*", argument)
        if EXECUTABLE.search(argument) or (name and name.group(1) in executables):
            lines.append(source.count("\n", 0, match.start()) + 1)
    return lines


def rust_sources(root):
    listed = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "--", *SOURCE_DIRECTORIES],
        cwd=root, text=True, capture_output=True, check=True).stdout.splitlines()
    return [path for path in listed if path.endswith(".rs") and not path.startswith(HELPER_DIRECTORY)]


def main(argv=None, root=ROOT):
    argparse.ArgumentParser(description=__doc__).parse_args(argv)
    problems = []
    for path in rust_sources(root):
        file = Path(root) / path
        if file.is_file():
            problems += [f"{path}:{line}: fs::copy of an executable" for line in check_source(file.read_text(errors="replace"))]
    if problems:
        print("\n".join(problems), file=sys.stderr)
        print(f"\ncheck_staged_executables: {len(problems)} raw copy(ies) of an executable.\n{MESSAGE}", file=sys.stderr)
        return 1
    print("check_staged_executables: no raw copy of an executable")
    return 0


if __name__ == "__main__":
    sys.exit(main())
