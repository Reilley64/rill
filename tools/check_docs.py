"""Check local Markdown destinations and anchors without fetching external URLs."""
from collections import Counter
from pathlib import Path
import html
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
FENCE = re.compile(r"^\s*(`{3,}|~{3,})")
LINK = re.compile(r"!?\[[^\]\n]*\]\(([^\s)]+)(?:\s+\"[^\"]*\")?\)")
REFERENCE = re.compile(r"^\s{0,3}\[[^\]]+\]:\s*(\S+)")


def prose_lines(text):
    fence = None
    for number, line in enumerate(text.splitlines(), 1):
        marker = FENCE.match(line)
        if marker:
            token = marker[1]
            if fence is None:
                fence = token
            elif token[0] == fence[0] and len(token) >= len(fence):
                fence = None
            continue
        if fence is None:
            yield number, line


def anchors(text):
    result = set()
    seen = Counter()
    for _, line in prose_lines(text):
        result.update(re.findall(r'<a\s+(?:id|name)=["\x27]([^"\x27]+)', line))
        heading = re.match(r"^#{1,6}\s+(.+?)\s*#*$", line)
        if not heading:
            continue
        title = html.unescape(re.sub(r"<[^>]*>", "", heading[1])).lower()
        slug = re.sub(r"[^\w\- ]", "", title).replace(" ", "-")
        count = seen[slug]
        seen[slug] += 1
        result.add(slug + ("-" + str(count) if count else ""))
    return result


def check(root):
    errors = []
    count = 0
    files = []
    for directory, children, names in root.walk():
        children[:] = [name for name in children if name not in {".git", ".serena", "target", ".venv"}]
        files.extend(directory / name for name in names if name.endswith(".md"))
    for file in sorted(files):
        for number, line in prose_lines(file.read_text()):
            code = [(m.start(), m.end()) for m in re.finditer(r"`+[^`]*`+", line)]
            destinations = [m[1] for m in LINK.finditer(line)
                            if not any(a <= m.start() < b for a, b in code)]
            reference = REFERENCE.match(line)
            if reference:
                destinations.append(reference[1])
            destinations.extend(re.findall(r'<img\s[^>]*src=["\x27]([^"\x27]+)', line))
            for url in destinations:
                parsed = urlsplit(url.strip("<>"))
                if parsed.scheme or parsed.netloc:
                    continue
                count += 1
                target = file.parent / unquote(parsed.path) if parsed.path else file
                reason = None
                if not target.exists():
                    reason = "missing path"
                elif parsed.fragment and target.suffix == ".md":
                    if unquote(parsed.fragment) not in anchors(target.read_text()):
                        reason = "missing anchor"
                if reason:
                    errors.append(f"{file.relative_to(root)}:{number}: {reason}: {url}")
    return count, errors


if __name__ == "__main__":
    count, errors = check(ROOT)
    for error in errors:
        print(error, file=sys.stderr)
    print(f"Checked {count} local documentation destinations; {len(errors)} errors.")
    raise SystemExit(bool(errors))
