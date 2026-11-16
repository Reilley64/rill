"""Validate hand-authored data/source coordinates, NOT Rill syntax or execution.

Run with jsonschema==4.25.1; see docs/prototypes/21-diagnostics.md for the uv command.
"""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re

from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[1]
# Keep the authored report URIs stable; source() resolves their suffixes under ROOT.
PREFIX = "file:///workspace/rill/prototypes/17-diagnostics/"
schema = json.loads((REPO / "docs/diagnostic-report.schema.json").read_text())
Draft202012Validator.check_schema(schema)
validator = Draft202012Validator(schema, format_checker=FormatChecker())
codes = set(re.findall(r"^### (R[0-9]+)\.", (REPO / "docs/diagnostic-codes.md").read_text(), re.M))


def source(location):
    assert location["uri"].startswith(PREFIX)
    path = ROOT / location["uri"][len(PREFIX):]
    assert path.resolve().is_relative_to(ROOT)
    data = path.read_bytes()
    start, end = int(location["startByte"]), int(location["endByte"])
    assert 0 <= start <= end <= len(data)
    assert location["sourceSnapshotId"] == "sha256:" + hashlib.sha256(data).hexdigest()
    for name, offset in [("start", start), ("end", end)]:
        before = data[:offset]
        tail = before.rsplit(b"\n", 1)[-1].decode("utf-8")
        assert location[name] == {"line": str(before.count(b"\n") + 1), "column": str(len(tail) + 1)}
    return data, start, end


def locations(value):
    if isinstance(value, dict):
        if {"uri", "startByte", "endByte", "sourceSnapshotId"} <= value.keys():
            source(value)
        for child in value.values():
            locations(child)
    elif isinstance(value, list):
        for child in value:
            locations(child)


reports = {}
for path in sorted(ROOT.glob("*-report.jsonl")):
    records = [json.loads(line) for line in path.read_text().splitlines()]
    reports[path.name] = records
    assert records and records[-1]["kind"] == "summary"
    assert len({record["runId"] for record in records}) == 1
    assert [int(record["sequence"]) for record in records] == list(range(1, len(records) + 1))
    for record in records:
        validator.validate(record)
        locations(record)
        if record["kind"] == "diagnostic":
            assert record["diagnostic"]["code"] in codes
        if record["kind"] == "expectationResult":
            item = record["expectation"]
            data, start, end = source(item["location"])
            assert data[start:end].decode() == item["expression"]
            budget = item["previewBudget"]
            assert budget == {"maxDepth": "4", "maxChildren": "8", "maxRenderedBytes": "4096"}
            assert sum(len(o.get("rendered", "").encode()) for o in item["observations"]) <= int(budget["maxRenderedBytes"])
            for observation in item["observations"]:
                data, start, end = source(observation["location"])
                assert data[start:end].decode() == observation["expression"]
    summary = records[-1]["summary"]
    counts = summary["counts"]
    assert int(counts["expectationsPassed"]) + int(counts["expectationsFailed"]) == int(counts["expectationsEvaluated"])
    assert int(counts["expectationsEvaluated"]) <= int(counts["expectationsDiscovered"])
    errors = sum(r["kind"] == "diagnostic" and r["diagnostic"]["severity"] == "error" for r in records)
    assert int(counts["errors"]) == errors
    warnings = sum(r["kind"] == "diagnostic" and r["diagnostic"]["severity"] == "warning" for r in records)
    assert int(counts["warnings"]) == warnings
    outcomes = [r["expectation"]["status"] for r in records if r["kind"] == "expectationResult"]
    assert outcomes.count("failed") == int(counts["expectationsFailed"])

expectations = reports["expect-report.jsonl"]
assert expectations[0]["expectation"]["observations"][0]["expression"] == "probe()"
assert expectations[1]["expectation"]["observations"][1]["status"] == "notEvaluated"
assert "rendered" not in expectations[1]["expectation"]["observations"][1]

coordinates = json.loads((ROOT / "coordinate-map.json").read_text())
raw = (ROOT / coordinates["fixture"]).read_bytes()
assert raw.count(b"\r\n") == raw.count(b"\n") and b"\texpect" in raw
for token in coordinates["tokens"]:
    data, start, end = source(token["location"])
    assert data[start:end].decode() == token["text"]
    for label, offset in [("start", start), ("end", end)]:
        before = data[:offset]
        tail = before.rsplit(b"\n", 1)[-1].decode()
        assert token["lspRange"][label] == {"line": before.count(b"\n"), "character": len(tail.encode("utf-16-le")) // 2}
right = coordinates["tokens"][1]
assert right["lspRange"]["start"]["character"] != int(right["location"]["start"]["column"]) - 1

expected = {
    "use-after-move": "R0200", "missing-width": "R0003", "non-bool-expect": "R0101",
    "nested-expect": "R0102", "nominal-mismatch": "R0100", "borrowed-spawn": "R0302",
    "incomparable-overloads": "R0303", "duplicate-body": "R0304", "protected-overlap": "R0500",
}
assert len(list((ROOT / "rejections").glob("*.rill"))) == len(expected)
for name, code in expected.items():
    assert code in codes and f"Expected {code}" in (ROOT / "rejections" / (name + ".rill")).read_text()
for name, count in [("valid.rill", 8), ("expect-failures.rill", 3), ("locations.rill", 2)]:
    assert len(re.findall(r"^\s*expect ", (ROOT / name).read_text(), re.M)) == count
for path in ROOT.rglob("*.rill"):
    text = re.sub(r'//[^\r\n]*|"(?:\\.|[^"\\])*"', "", path.read_text())
    pending = []
    pairs = {"(": ")", "[": "]", "{": "}"}
    for char in text:
        if char in pairs:
            pending.append(pairs[char])
        elif char in ")]}":
            assert pending and pending.pop() == char, path
    assert not pending, path

# Negative schema examples are mutations of contract data, not Rill rejection runs.
invalid = []
x = deepcopy(expectations[0]); x["schemaVersion"] = 2; invalid.append(x)
x = deepcopy(expectations[0]); x["sequence"] = 1; invalid.append(x)
x = deepcopy(expectations[0]); x["expectation"]["location"]["start"]["line"] = "0"; invalid.append(x)
x = deepcopy(expectations[0]); x["expectation"]["observations"][0]["status"] = "notEvaluated"; invalid.append(x)
x = deepcopy(expectations[-1]); x["summary"]["outcome"] = "passed"; invalid.append(x)
x = deepcopy(expectations[-1]); x["summary"]["outcome"] = "passed"; x["summary"]["analysisComplete"] = False; invalid.append(x)
for record in invalid:
    assert not validator.is_valid(record)
x = deepcopy(expectations[0]); x["futureOptionalField"] = "ignored"; validator.validate(x)
x = deepcopy(reports["check-report.jsonl"][0]); x["context"] = None; x["diagnostic"]["primary"] = None; x["diagnostic"]["code"] = "R0601"; validator.validate(x)
print("PASS: schema v1 and 6 negative data cases; 5 illustrative reports, source hashes/ranges/counts, CRLF/scalar/UTF-16 mapping, 8 positive declarations and 9 rejection fixtures.")
print("NOT RUN: Rill parser/type checker/expectation execution, trusted snapshot effects, LSP/backend/OpenIDEA acceptance. Delimiter/header checks are not semantic validation.")
