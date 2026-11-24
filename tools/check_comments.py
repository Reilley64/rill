"""Refuse Rust comments outside the keep list in docs/agents/comments.md.

By default the check reads the comments added or changed against the merge base.
`--tree` reads every comment in the tracked and untracked Rust sources instead.
The base is `--base`, else `origin/$GITHUB_BASE_REF` on a pull request, else `origin/main`.
That covers a pull request run and a dispatched run on a branch with no pull request.
The checkout needs the full history of the base (`fetch-depth: 0`).
"""
import argparse
import bisect
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIRECTORIES = ["crates", "runtime", "tools"]
MESSAGE = (
    "Remove the comment. If it explains a workaround, fix the root cause; "
    "if it states a rule, write a test or a type; if it records a decision, "
    "link the bead, RFC or ADR. The keep list is docs/agents/comments.md."
)

IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
RAW_OPEN = re.compile(r'(#*)"')
BEAD = r"(?:ri|ci)-[a-z0-9]{3,4}(?:\.\d+)*"
DOCUMENT = r"docs/(?:rfcs|adr)/[\w./-]+(?:#[\w-]+)?"
LINK_ONLY = re.compile(f"(?:{BEAD}|{DOCUMENT})")
LICENSE = re.compile(r"licen[sc]e|copyright|spdx", re.IGNORECASE)
UNSAFE_USE = re.compile(r"\bunsafe\b(?!\s+(?:fn|trait)\b)")
PUBLIC_ITEM = re.compile(r"pub\s")
PUBLIC_CONTAINER = re.compile(r"(?:#\[[^\]]*\]\s*)*pub\s+(?:unsafe\s+)?(?:enum|trait)\b")


class Comment:
    def __init__(self, start, end, text, enclosing):
        self.start = start
        self.end = end
        self.text = text
        self.enclosing = enclosing

    @property
    def is_inner_doc(self):
        return self.text.startswith(("//!", "/*!"))

    @property
    def is_outer_doc(self):
        return (self.text.startswith("///") and not self.text.startswith("////")) or (
            self.text.startswith("/**") and not self.text.startswith(("/***", "/**/")))

    @property
    def body(self):
        if self.text.startswith("//"):
            return self.text[2:].strip()
        return self.text[2:-2].strip()


def blank(text):
    return re.sub(r"[^\n]", " ", text)


def scan(source):
    """Return the comments of Rust source and the source with comments and literal contents blanked."""
    n = len(source)
    code = []
    header = []
    stack = []
    comments = []

    def emit(text):
        code.append(text)
        header.append(text)

    def enclosing():
        return stack[-1] if stack else ""

    i = 0
    while i < n:
        char = source[i]
        if source.startswith("//", i):
            j = source.find("\n", i)
            j = n if j < 0 else j
            comments.append(Comment(i, j, source[i:j], enclosing()))
            code.append(" " * (j - i))
            i = j
        elif source.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if source.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif source.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            comments.append(Comment(i, j, source[i:j], enclosing()))
            code.append(blank(source[i:j]))
            i = j
        elif char == '"':
            j = i + 1
            while j < n and source[j] != '"':
                j += 2 if source[j] == "\\" else 1
            j = min(j + 1, n)
            emit('"' + blank(source[i + 1:j - 1]) + '"')
            i = j
        elif char == "'":
            if source.startswith("\\", i + 1):
                j = source.find("'", i + 3) + 1 or n
            elif source.startswith("'", i + 2):
                j = i + 3
            else:
                j = i + 1
            emit("'" + blank(source[i + 1:j - 1]) + "'" if j > i + 1 else "'")
            i = j
        elif IDENT.match(source, i):
            word = IDENT.match(source, i)
            j = word.end()
            opening = RAW_OPEN.match(source, j) if word[0] in ("r", "br", "cr") else None
            if opening:
                closing = '"' + opening[1]
                k = source.find(closing, opening.end())
                k = n if k < 0 else k + len(closing)
                emit(word[0] + opening[1] + '"' + blank(source[opening.end():k - len(closing)]) + closing)
                j = k
            else:
                emit(word[0])
            i = j
        else:
            if char == "{":
                stack.append("".join(header).strip())
                header.clear()
            elif char == "}":
                if stack:
                    stack.pop()
                header.clear()
            elif char == ";":
                header.clear()
            code.append(char)
            if char not in "{};":
                header.append(char)
            i += 1
    return comments, "".join(code)


def skip_attributes(code, position):
    while True:
        while position < len(code) and code[position].isspace():
            position += 1
        if not code.startswith("#", position):
            return position
        opening = code.find("[", position)
        if opening < 0:
            return position
        depth, position = 0, opening
        while position < len(code):
            depth += {"[": 1, "]": -1}.get(code[position], 0)
            position += 1
            if depth == 0:
                break


def is_doc_contract(comment, code, inner_public):
    if comment.is_inner_doc:
        return inner_public
    if not comment.is_outer_doc:
        return False
    if PUBLIC_CONTAINER.match(comment.enclosing):
        return True
    return bool(PUBLIC_ITEM.match(code, skip_attributes(code, comment.end)))


def is_safety(group, code):
    if not group[0].body.startswith("SAFETY:"):
        return False
    rest = code[group[-1].end:].lstrip("\n \t").split("\n", 1)[0]
    return bool(UNSAFE_USE.search(rest))


def is_license(group, code):
    first = group[0]
    if first.is_inner_doc or first.is_outer_doc or code[:first.start].strip():
        return False
    return bool(LICENSE.search(" ".join(c.body for c in group)))


def groups_of(comments, code, line_of):
    """Group consecutive full-line `//` comments, so a multi-line SAFETY note or license header is one unit."""
    groups = []
    previous_line = None
    for comment in comments:
        plain = comment.text.startswith("//") and not (comment.is_inner_doc or comment.is_outer_doc)
        full_line = not code[source_line_start(code, comment.start):comment.start].strip()
        start = line_of(comment.start)
        if plain and full_line and previous_line == start - 1:
            groups[-1].append(comment)
        else:
            groups.append([comment])
        previous_line = line_of(comment.end) if plain and full_line else None
    return groups


def source_line_start(code, offset):
    return code.rfind("\n", 0, offset) + 1


def check_source(source, inner_public=False, changed=None):
    """Return (line, text) for each comment outside the keep list that touches a changed line (any line when None)."""
    comments, code = scan(source)
    starts = [0] + [m.end() for m in re.finditer("\n", source)]

    def line_of(offset):
        return bisect.bisect_right(starts, offset)

    found = []
    for group in groups_of(comments, code, line_of):
        if is_safety(group, code) or is_license(group, code):
            continue
        for comment in group:
            if is_doc_contract(comment, code, inner_public) or LINK_ONLY.fullmatch(comment.body):
                continue
            first, last = line_of(comment.start), line_of(max(comment.end - 1, comment.start))
            if changed is None or any(line in changed for line in range(first, last + 1)):
                found.append((first, comment.text.splitlines()[0]))
    return found


def is_public_module(root, relative):
    path = root / relative
    if path.name == "lib.rs":
        return True
    name, directory = (path.parent.name, path.parent.parent) if path.name == "mod.rs" else (path.stem, path.parent)
    parents = [directory / "lib.rs", directory / "mod.rs", directory.with_suffix(".rs")]
    pattern = re.compile(rf"\bpub\s+mod\s+{re.escape(name)}\b")
    return any(p.is_file() and pattern.search(p.read_text(errors="replace")) for p in parents)


def git(root, *args):
    result = subprocess.run(["git", "-C", str(root), "-c", "core.quotePath=false", *args],
                            capture_output=True, text=True)
    if result.returncode:
        raise SystemExit(f"check_comments: git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def rust_sources(paths):
    return [p for p in paths if p.endswith(".rs") and p.split("/", 1)[0] in SOURCE_DIRECTORIES]


def merge_base(root, base):
    if base is None:
        base = "origin/" + (os.environ.get("GITHUB_BASE_REF") or "main")
    result = subprocess.run(["git", "-C", str(root), "merge-base", "HEAD", base], capture_output=True, text=True)
    if result.returncode:
        raise SystemExit(f"check_comments: no merge base of HEAD and {base}. "
                         f"Fetch the full history of the base, for example `fetch-depth: 0` in actions/checkout. "
                         f"{result.stderr.strip()}")
    return result.stdout.strip()


def added_lines(root, base):
    """Map each Rust source to the set of lines the working tree adds or changes against `base`."""
    changed = {}
    path = None
    diff = git(root, "diff", "-U0", "--no-color", "--no-ext-diff", "--no-renames", base, "--", *SOURCE_DIRECTORIES)
    for line in diff.splitlines():
        if line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else None
        elif line.startswith("@@") and path:
            hunk = re.match(r"@@ -\S+ \+(\d+)(?:,(\d+))? @@", line)
            first, count = int(hunk[1]), int(hunk[2] if hunk[2] is not None else 1)
            changed.setdefault(path, set()).update(range(first, first + count))
    untracked = git(root, "ls-files", "--others", "--exclude-standard", "--", *SOURCE_DIRECTORIES).splitlines()
    for path in untracked:
        changed[path] = None
    return {p: lines for p, lines in changed.items() if rust_sources([p])}


def main(argv=None, root=ROOT):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tree", action="store_true", help="check every comment in the tree, not only added ones")
    parser.add_argument("--base", help="the commit to diff against; default is the merge base with origin/main")
    options = parser.parse_args(argv)
    root = Path(root)

    if options.tree:
        listed = git(root, "ls-files", "--cached", "--others", "--exclude-standard", "--", *SOURCE_DIRECTORIES)
        targets = {path: None for path in rust_sources(listed.splitlines())}
        scope = "every comment"
    else:
        base = merge_base(root, options.base)
        targets = added_lines(root, base)
        scope = f"comments added or changed since {base[:8]}"

    problems = []
    for path, changed in sorted(targets.items()):
        file = root / path
        if not file.is_file():
            continue
        source = file.read_text(errors="replace")
        inner = ("//!" in source or "/*!" in source) and is_public_module(root, path)
        problems += [f"{path}:{line}: {text}" for line, text in check_source(source, inner, changed)]
    if problems:
        print("\n".join(problems), file=sys.stderr)
        print(f"\ncheck_comments: {len(problems)} comment(s) outside the keep list.\n{MESSAGE}", file=sys.stderr)
        return 1
    print(f"check_comments: {scope} in {len(targets)} Rust file(s): all in the keep list")
    return 0


if __name__ == "__main__":
    sys.exit(main())
