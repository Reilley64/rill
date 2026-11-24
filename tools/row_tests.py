"""Map each acceptance-matrix row to the tests whose names carry its id, and fail on a row with none.

A matrix is a Markdown table whose first cell is a row id: a capital letter or two and a number,
such as `O1`, `K3` or `C12`. A test belongs to a row when its fn name, or a directory or file name
on its path under the tests directory, is the row's prefix (`o01`, `k03`, `c12`) or starts with the
prefix and an underscore (`o01_shifted_point_checks`, `c12_pre.rs`). The prefix is the id in lower
case with its number padded to two digits. A test with `#[ignore]` does not count.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
# (matrix, tests directory) pairs that `check` runs in CI. A milestone adds its matrix here.
MATRICES = (("crates/compiler/tests/m5-status.md", "crates/compiler/tests"),)

ROW = re.compile(r"^\|\s*`?([A-Z]{1,2})(\d{1,3})`?\s*\|")
TEST = re.compile(r"((?:[ \t]*#\[[^\]\n]*\][ \t]*\n)+)[ \t]*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)")


def row_prefix(letters, number):
    return f"{letters.lower()}{int(number):02d}"


def matrix_rows(text):
    """The row prefixes of a matrix, keyed by the row id as written, in order."""
    rows = {}
    for line in text.splitlines():
        row = ROW.match(line)
        if row:
            rows[row[1] + row[2]] = row_prefix(row[1], row[2])
    return rows


def source_tests(tests_dir):
    """Every non-ignored `#[test]` fn under `tests_dir`, as (path segments, fn name) pairs."""
    found = []
    for file in sorted(Path(tests_dir).rglob("*.rs")):
        relative = file.relative_to(tests_dir)
        parts = list(relative.parts[:-1])
        if relative.stem != "mod":
            parts.append(relative.stem)
        for attributes, name in TEST.findall(file.read_text()):
            if "#[test]" in attributes and "#[ignore" not in attributes:
                found.append((relative, parts, name))
    return found


def belongs(prefix, parts, name):
    return any(segment == prefix or segment.startswith(prefix + "_") for segment in [*parts, name])


def row_tests(rows, tests):
    """Each row id mapped to the `file::fn` names of its tests."""
    return {
        row: [f"{relative.as_posix()}::{name}" for relative, parts, name in tests if belongs(prefix, parts, name)]
        for row, prefix in rows.items()
    }


def missing_rows(rows, mapping):
    """The (row id, prefix) pairs of the rows with no test."""
    return [(row, prefix) for row, prefix in rows.items() if not mapping[row]]


def render(mapping):
    lines = ["| Row | Tests |", "| --- | --- |"]
    for row, found in mapping.items():
        lines.append(f"| {row} | {', '.join(f'`{test}`' for test in found) or 'none'} |")
    return "\n".join(lines)


def main(argv):
    command = argv[0] if argv else ""
    if command not in {"check", "map"} or len(argv) not in {1, 3}:
        raise SystemExit("usage: row_tests.py check|map [MATRIX TESTS_DIR]")
    pairs = [tuple(argv[1:])] if len(argv) == 3 else [(ROOT / m, ROOT / t) for m, t in MATRICES]
    failed = False
    for matrix, tests_dir in pairs:
        rows = matrix_rows(Path(matrix).read_text())
        if not rows:
            raise SystemExit(f"{matrix}: no matrix row found (a table row whose first cell is an id such as O1)")
        mapping = row_tests(rows, source_tests(tests_dir))
        if command == "map":
            print(render(mapping))
        for row, prefix in missing_rows(rows, mapping) if command == "check" else []:
            print(f"{matrix}: row {row} has no test: name one `{prefix}_...`", file=sys.stderr)
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
