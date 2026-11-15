#!/usr/bin/env python3
"""Darwin reference toolchain; builds/runs only in disposable /tmp storage."""
import argparse
import hashlib
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import zipfile

URL = "https://sqlite.org/2026/sqlite-amalgamation-3540100.zip"
SHA3 = "92281c86e2957f9b9305890f9ac04204caa7b677cc868dc326d942689df28e30"
SOURCE_ID = "2026-10-16 14:45:20 555e07ef33f69a0957d24d8e555b702464a131d6f1607f4c3c1e4551e8e6ca76"
MACROS = ["SQLITE_THREADSAFE=1", "SQLITE_OMIT_LOAD_EXTENSION", "SQLITE_OMIT_SHARED_CACHE", "SQLITE_TRUSTED_SCHEMA=0", "SQLITE_MAX_MMAP_SIZE=0"]
HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="reuse a local archive, still hash verified; otherwise download afresh")
    parser.add_argument("--report", type=Path, default=HERE / "results.md")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("this nm/SDK isolation derivation is Darwin-specific; other cells are untested")
    commands = []
    def run(cmd, timeout=30, cwd=None):
        commands.append(f"[timeout {timeout}s" + (f", cwd={cwd}" if cwd else "") + "] " + shlex.join(map(str, cmd)))
        p = subprocess.run(list(map(str, cmd)), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True, timeout=timeout, check=False, cwd=cwd)
        if p.returncode:
            raise RuntimeError(f"{commands[-1]}\nexit={p.returncode}\n{p.stdout}")
        return p.stdout
    cc = shlex.split(os.environ.get("CC", "cc"))
    compiler = run(cc + ["--version"])
    os_info = run(["uname", "-a"]) + run(["sw_vers"])
    # No checked-in engine, archives, headers, objects, or executables.
    with tempfile.TemporaryDirectory(prefix="rill43-reference-", dir="/tmp") as tmp:
        root = Path(tmp)
        archive = root / "sqlite.zip"
        if args.archive:
            shutil.copyfile(args.archive, archive)
        else:
            run(["curl", "--fail", "--location", "--max-time", "90", "--output", archive, URL], 100)
        actual = hashlib.sha3_256(archive.read_bytes()).hexdigest()
        if actual != SHA3:
            raise RuntimeError(f"archive SHA3-256 mismatch: {actual}")
        with zipfile.ZipFile(archive) as z:
            for name in ("sqlite3.c", "sqlite3.h"):
                (root / name).write_bytes(z.read("sqlite-amalgamation-3540100/" + name))
        base = cc + ["-std=c11", "-O2", "-fvisibility=hidden"] + ["-D" + m for m in MACROS]
        run(base + ["-c", root / "sqlite3.c", "-o", root / "unprefixed.o"], 120)
        def symbols(obj, defined=False):
            options = ["nm", "-g", "-j"]
            if defined:
                options.append("--defined-only")
            return {line.removeprefix("_") for line in run(options + [obj]).splitlines() if line}
        original = sorted(s for s in symbols(root / "unprefixed.o", True) if s.startswith("sqlite"))
        if not original or "sqlite3_open_v2" not in original:
            raise RuntimeError("nm did not discover a complete candidate SQLite symbol set")
        prefix = root / "prefix.h"
        prefix.write_text("/* Derived from ALL global sqlite* definitions in the actual profile object. */\n" +
                          "".join(f"#define {s} rill43_private_{s}\n" for s in original))
        run(base + ["-include", prefix, "-c", root / "sqlite3.c", "-o", root / "private.o"], 120)
        private_defs = symbols(root / "private.o", True)
        expected = {"rill43_private_" + s for s in original}
        if not expected.issubset(private_defs):
            raise RuntimeError("prefix rebuild lost definitions: " + repr(expected - private_defs))
        run(cc + ["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-I", root, "-include", prefix,
                  "-c", HERE / "checks.c", "-o", root / "checks.o"], 60)
        for obj in (root / "private.o", root / "checks.o"):
            leaks = sorted(s for s in symbols(obj) if s.startswith("sqlite"))
            if leaks:
                raise RuntimeError(f"unprefixed SQLite definitions/references in {obj}: {leaks}")
        # In Mach-O hidden definitions appear as 'private external' in nm -m.
        visibility = run(["nm", "-g", "-m", "--defined-only", root / "private.o"])
        visible = [line for line in visibility.splitlines()
                   if "_rill43_private_sqlite" in line and "private external" not in line]
        if visible:
            raise RuntimeError("SQLite definitions not hidden: " + repr(visible))
        # No -I and no -include: the host TU uses the Apple SDK SQLite header/library.
        run(cc + ["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c", HERE / "host.c", "-o", root / "host.o"], 60)
        run(cc + [root / "checks.o", root / "host.o", root / "private.o", "-lsqlite3", "-o", root / "checks"], 60)
        dependencies = run(["otool", "-L", root / "checks"])
        output = run([root / "checks", root / "disposable.db"], 20, cwd=root)
        summary = re.search(r"SUMMARY checks=(\d+) failures=0", output)
        if not summary:
            raise RuntimeError("missing successful fixture summary\n" + output)
        manifest_hash = hashlib.sha256(("\n".join(original) + "\n").encode()).hexdigest()
        report = f"""# SQLite #43 standalone native reference results

## Scope and outcome

**PASS: {summary.group(1)} countable native fixture checks**, plus archive, prefix-definition/reference,
and hidden-visibility build audits below. This is source/reference/model evidence only.
**#43 remains OPEN; actual embedded support is not delivered and #30 is uncleared.**
No Rill compiler/runtime/bridge exists here. No SDK/ABI, lease, queue, adoption,
worker-migration, cancellation-protocol, resource-safety, or support test is claimed.
The native authorizer/setup and trusted bypass are fixtures, not product enforcement.

## Reproduce

From the repository root, Python 3, Apple SDK/cc, curl, nm and otool available:

```sh
python3 prototypes/26-sqlite-provider/reference/run.py
# Offline/reuse variant (still validates the full archive hash):
python3 prototypes/26-sqlite-provider/reference/run.py --archive /tmp/rill43-sqlite-audit.whHl4S/sqlite-amalgamation-3540100.zip
```

This report's invocation: `{shlex.join(sys.argv)}`.
Fresh temporary build directory: `{root}` (deleted on exit, including database,
engine, generated prefix header, objects, executable and archive).
Every subprocess has an explicit timeout; the test process has 20 seconds.
Download has both a curl 90-second total bound and a 100-second process bound;
compiles have 60/120-second bounds. Failure/timeout raises an error and does not
write a new passing report. No archive/engine/binaries are checked in.

## Pin, toolchain, exact profile

- Official URL: {URL}
- SHA3-256 verified over archive bytes: `{actual}`
- Expected native sourceID: `{SOURCE_ID}` (checked at runtime).
- C flags for engine: `-std=c11 -O2 -fvisibility=hidden`
- Exact engine preprocessor defines: `{' '.join('-D' + m for m in MACROS)}`
- `SQLITE_ENABLE_BATCH_ATOMIC_WRITE` is **not defined**, not even as zero.
- No optional extension enable macros are supplied. Core JSON and pragma vtables
  remain enabled. Runtime compile-option inventory follows with the checks.
- Global `SQLITE_CONFIG_SERIALIZED` before private initialize; opens use
  `READWRITE|CREATE|FULLMUTEX|PRIVATECACHE`; defensive ON, trusted schema OFF,
  foreign keys ON, busy timeout 0, internally controlled query_only, sync FULL.
  File fixture confirms delete journal; no automatic WAL conversion.

```text
{compiler.rstrip()}
{os_info.rstrip()}
```

## Host-specific build-isolation audit

PASS: compile the exact unprefixed profile object first; use host `nm -g -j
--defined-only` and strip Mach-O's leading underscore to derive **all global
`sqlite*` definitions**, including data/internal globals, not a hand-picked API
list. Generated macros rename all {len(original)} discovered definitions to
`rill43_private_*`. Rebuild with hidden visibility. PASS: all {len(original)}
expected renamed definitions are present. PASS: `nm -g -j` (defined AND undefined
symbols) on private engine and fixture objects finds no unprefixed `sqlite*`
symbols. PASS: host `nm -m` identifies all private SQLite globals as private
external (hidden). Sorted original-symbol manifest SHA-256:
`{manifest_hash}`. The temporary manifest/header is regenerated each run.

Host TU is separately compiled without the prefix header or amalgamation include
path and linked with `-lsqlite3`. The fixture attempts host SINGLETHREAD before
initialization; **this host returns SQLITE_ERROR (1), so successful host
SINGLETHREAD reconfiguration is UNTESTED, not passed**.
The observed host db_mutex is null; private FULLMUTEX db_mutex is nonnull, but
we cannot attribute the host result to the rejected configuration request.
Host remains live during all private tests and still executes a query after
private shutdown. Distinct exact native
versions/sourceIDs are printed below. This proves basic separate-TU coexistence,
not successful host threading reconfiguration or its independence.
This is **one Darwin arm64 executable's coexistence evidence**, not complete
link/export, dynamic-loader, allocator, Rill, ABI, platform, or supply-chain isolation.
The derivation is host-toolchain-specific, not an actual product symbol generator.

```text
{dependencies.rstrip()}
```

## Native checks (each PASS/FAIL separately counted)

```text
{output.rstrip()}
```

## Interpretation and remaining gates

- Initial storage classes are read before conversions. SQLite coercion and NaN
  conversion to NULL are observations, not allowed product checked conversions.
  Product must reject unsupported floats before native bind. Copied row/error
  bytes survive invalidation only because this C fixture explicitly copies them;
  it never dereferences an invalid native view.
- Constraint extended codes classify UNIQUE/PRIMARYKEY/ROWID. There is no
  structured failing-key identity/complete key-details API in these diagnostics.
  The fixture does not infer key identity by parsing English messages or by
  attaching a later schema observation. The error-copy check merely recognizes
  its deliberately controlled fixture message, not a product classifier.
- PRAGMA policy accepts object arguments only for the seven enumerated object
  introspections, noarg database_list and seven enumerated noarg observations;
  denies unknown names and assignments to observations. Case is insensitive.
  Both direct and table-valued paths are exercised; a denied virtual-table
  PRAGMA can fail at step rather than prepare, with an authorizer denial observed.
  Unknown table-valued names fail lookup, not an invented authorizer event.
  Cached-statement reprepare is forced via sqlite3_set_authorizer (SQLite expires
  statements), and denied on reauthorization. This does not exhaust every schema,
  view/trigger, virtual-table, malicious SQL, or engine-policy interaction.
  Prepare alone can mutate cache_size without policy; readonly classification or
  checking only step is insufficient. Trusted fixture operations leave the
  callback installed but bypass its rules; this is not a specified product gate.
- Reset retains bindings and may return a previous step error. Finalize consumes
  the statement even when reporting error. close BUSY leaves a usable connection;
  close_v2 creates a zombie retained by outstanding statements. The zombie handle
  is not used after close_v2. No allocator instrumentation proves a leak theorem.
- Native autocommit, cached query_only enforcement, and post-interrupt usability
  do not establish managed transaction outcomes, pool reuse/reset or replay safety.
  Busy timeout 0 plus an externally bounded process establishes immediate native
  BUSY rather than a configured busy retry; no timing/performance claim is made.
- Progress cancellation has a deterministic first-callback abort and finite
  recursive SQL upper bound; it can fire during prepare. Interrupt is synchronous
  after a first row of a finite two-row query. These are native references only,
  not cross-thread cancellation/abort joining, deadline latency or reuse evidence.
- Shared contract defaults (16 MiB parameter/field, 64 MiB row/frame, 1 MiB
  metadata/decimal, 4096 columns, 32767 parameters) are **not implemented here**.
  Native defaults/lowered tests demonstrate that lower native limits win: this
  engine caps columns at 2000 and parameters at 32766. Tiny temporary native
  limits exercise parameter, column and bind-payload rejection without large
  allocations. They do not test owning-copy aggregate budgets or every native limit.
- Development probe corrections: an initial fixture incorrectly expected the
  compile-option list to enumerate TRUSTED_SCHEMA=0 (it does not; exact compile
  command plus fresh default check are used instead), and tried length limit 8
  (SQLite clamps it to minimum 30; final test uses 32 and 33 bytes). The host
  SINGLETHREAD attempt genuinely failed and remains unavailable, as above.
- Other three native matrix cells and WASI are **UNTESTED**. No cross-compilation,
  target support, SDK ABI, lease/epoch/queue/adoption, worker migration, release
  ordering, Wasm memory stability, sanitizer/fault injection, or product runtime
  tests were possible. #43 OPEN and #30 uncleared, regardless of these passes.

## Executed commands

Absolute temporary paths reflect this run; the runner regenerates them.

```text
{chr(10).join(commands)}
```
"""
        args.report.write_text(report)
        print(output, end="")
        print(f"PASS build audits: {len(original)} prefixed globals; report={args.report}")


if __name__ == "__main__":
    main()
