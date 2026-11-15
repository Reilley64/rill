# SQLite #43 standalone native reference results

## Scope and outcome

**PASS: 98 countable native fixture checks**, plus archive, prefix-definition/reference,
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

This report's invocation: `prototypes/26-sqlite-provider/reference/run.py --archive /tmp/rill43-sqlite-audit.whHl4S/sqlite-amalgamation-3540100.zip`.
Fresh temporary build directory: `/tmp/rill43-reference-ngsv31hf` (deleted on exit, including database,
engine, generated prefix header, objects, executable and archive).
Every subprocess has an explicit timeout; the test process has 20 seconds.
Download has both a curl 90-second total bound and a 100-second process bound;
compiles have 60/120-second bounds. Failure/timeout raises an error and does not
write a new passing report. No archive/engine/binaries are checked in.

## Pin, toolchain, exact profile

- Official URL: https://sqlite.org/2026/sqlite-amalgamation-3540100.zip
- SHA3-256 verified over archive bytes: `92281c86e2957f9b9305890f9ac04204caa7b677cc868dc326d942689df28e30`
- Expected native sourceID: `2026-10-16 14:45:20 555e07ef33f69a0957d24d8e555b702464a131d6f1607f4c3c1e4551e8e6ca76` (checked at runtime).
- C flags for engine: `-std=c11 -O2 -fvisibility=hidden`
- Exact engine preprocessor defines: `-DSQLITE_THREADSAFE=1 -DSQLITE_OMIT_LOAD_EXTENSION -DSQLITE_OMIT_SHARED_CACHE -DSQLITE_TRUSTED_SCHEMA=0 -DSQLITE_MAX_MMAP_SIZE=0`
- `SQLITE_ENABLE_BATCH_ATOMIC_WRITE` is **not defined**, not even as zero.
- No optional extension enable macros are supplied. Core JSON and pragma vtables
  remain enabled. Runtime compile-option inventory follows with the checks.
- Global `SQLITE_CONFIG_SERIALIZED` before private initialize; opens use
  `READWRITE|CREATE|FULLMUTEX|PRIVATECACHE`; defensive ON, trusted schema OFF,
  foreign keys ON, busy timeout 0, internally controlled query_only, sync FULL.
  File fixture confirms delete journal; no automatic WAL conversion.

```text
Apple clang version 21.0.0 (clang-2100.3.34.2)
Target: arm64-apple-darwin27.0.0
Thread model: posix
InstalledDir: /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin
Darwin Mac.localdomain 27.0.0 Darwin Kernel Version 27.0.0: Tue Aug 11 21:05:41 PDT 2026; root:xnu-13432.1.9~1/RELEASE_ARM64_T8122 arm64
ProductName:		macOS
ProductVersion:		27.0.1
BuildVersion:		26A434
```

## Host-specific build-isolation audit

PASS: compile the exact unprefixed profile object first; use host `nm -g -j
--defined-only` and strip Mach-O's leading underscore to derive **all global
`sqlite*` definitions**, including data/internal globals, not a hand-picked API
list. Generated macros rename all 276 discovered definitions to
`rill43_private_*`. Rebuild with hidden visibility. PASS: all 276
expected renamed definitions are present. PASS: `nm -g -j` (defined AND undefined
symbols) on private engine and fixture objects finds no unprefixed `sqlite*`
symbols. PASS: host `nm -m` identifies all private SQLite globals as private
external (hidden). Sorted original-symbol manifest SHA-256:
`0b32e886d0c525eab75bff451e542ace9abc0bb64039e208fd26a914e07eca3a`. The temporary manifest/header is regenerated each run.

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
/tmp/rill43-reference-ngsv31hf/checks:
	/usr/lib/libsqlite3.dylib (compatibility version 9.0.0, current version 406.0.0)
	/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1359.0.0)
```

## Native checks (each PASS/FAIL separately counted)

```text
HOST config_SINGLETHREAD_rc=1 version=3.54.0 sourceID=2026-04-09 12:25:13 8fa8248e303219400c646a885e36dfc52eae33d83f4412e3f369b2be5373aapl threadsafe=2
UNTESTED host SINGLETHREAD/null-mutex discrimination: system config rejected
HOST initialize_rc=0 open_rc=0
HOST db_mutex=null (host remains live during private tests)
PASS 001 host separate-TU open (singlethread result reported separately)
PRIVATE version=3.54.1 sourceID=2026-10-16 14:45:20 555e07ef33f69a0957d24d8e555b702464a131d6f1607f4c3c1e4551e8e6ca76 threadsafe=1
PASS 002 pinned native identity
PASS 003 profile threadsafe serialized compiled
PASS 004 batch atomic write absent (not defined as zero)
PASS 005 omits and mmap profile
PASS 006 optional extension enable macros absent
OPTION ATOMIC_INTRINSICS=1
OPTION COMPILER=clang-21.0.0
OPTION DEFAULT_AUTOVACUUM
OPTION DEFAULT_CACHE_SIZE=-2000
OPTION DEFAULT_FILE_FORMAT=4
OPTION DEFAULT_JOURNAL_SIZE_LIMIT=-1
OPTION DEFAULT_MMAP_SIZE=0
OPTION DEFAULT_PAGE_SIZE=4096
OPTION DEFAULT_PCACHE_INITSZ=20
OPTION DEFAULT_RECURSIVE_TRIGGERS
OPTION DEFAULT_SECTOR_SIZE=4096
OPTION DEFAULT_SYNCHRONOUS=2
OPTION DEFAULT_WAL_AUTOCHECKPOINT=1000
OPTION DEFAULT_WAL_SYNCHRONOUS=2
OPTION DEFAULT_WORKER_THREADS=0
OPTION DIRECT_OVERFLOW_READ
OPTION MALLOC_SOFT_LIMIT=1024
OPTION MAX_ATTACHED=10
OPTION MAX_COLUMN=2000
OPTION MAX_COMPOUND_SELECT=500
OPTION MAX_DEFAULT_PAGE_SIZE=8192
OPTION MAX_EXPR_DEPTH=1000
OPTION MAX_FUNCTION_ARG=1000
OPTION MAX_LENGTH=1000000000
OPTION MAX_LIKE_PATTERN_LENGTH=50000
OPTION MAX_MMAP_SIZE=0
OPTION MAX_PAGE_COUNT=0xfffffffe
OPTION MAX_PAGE_SIZE=65536
OPTION MAX_SCHEMA=10000000
OPTION MAX_SQL_LENGTH=1000000000
OPTION MAX_TRIGGER_DEPTH=1000
OPTION MAX_VARIABLE_NUMBER=32766
OPTION MAX_VDBE_OP=250000000
OPTION MAX_WORKER_THREADS=8
OPTION MUTEX_PTHREADS
OPTION OMIT_LOAD_EXTENSION
OPTION OMIT_SHARED_CACHE
OPTION SYSTEM_MALLOC
OPTION TEMP_STORE=1
OPTION THREADSAFE=1
PASS 007 private FULLMUTEX nonnull independent of host
PASS 008 connection baseline
PASS 009 defensive on
PASS 010 core JSON retained
PASS 011 failed open returns nonnull cleanup handle
PASS 012 failed open handle closes
PASS 013 transient bind copied before caller mutation
PASS 014 empty BLOB differs from NULL by initial type
PASS 015 embedded NUL text length and bytes preserved
PASS 016 i64 extrema exact
PASS 017 native NaN becomes NULL (product must reject before bind)
PASS 018 reset retains bindings
PASS 019 clear removes bindings
PASS 020 explicit row copy survives reset/finalize
PASS 021 initial types captured before coercion
PASS 022 native coercion is not checked decoding
PASS 023 sparse ?5 and repeated named parameter
PASS 024 ?5 alone counts five including gaps
PASS 025 UNIQUE extended code, no structured key identity API
PASS 026 PRIMARY KEY extended code
PASS 027 ROWID extended code
PASS 028 explicit diagnostic copy survives native error invalidation
PASS 029 reset reports prior step error but retains statement
PASS 030 finalize consumes even when reporting error
PASS 031 PRAGMA TaBlE_InFo(t)
PASS 032 PRAGMA table_xinfo(t)
PASS 033 PRAGMA table_list(t)
PASS 034 PRAGMA index_list(t)
PASS 035 PRAGMA index_info(sqlite_autoindex_t_1)
PASS 036 PRAGMA index_xinfo(sqlite_autoindex_t_1)
PASS 037 PRAGMA foreign_key_list(t)
PASS 038 PRAGMA database_list
PASS 039 PRAGMA foreign_keys
PASS 040 PRAGMA query_only
PASS 041 PRAGMA trusted_schema
PASS 042 PRAGMA schema_version
PASS 043 PRAGMA journal_mode
PASS 044 PRAGMA synchronous
PASS 045 PRAGMA compile_options
PASS 046 BEGIN
PASS 047 COMMIT
PASS 048 ROLLBACK
PASS 049 SAVEPOINT x
PASS 050 RELEASE x
PASS 051 ROLLBACK TO x
PASS 052 ATTACH ':memory:' AS a
PASS 053 DETACH a
PASS 054 PRAGMA foreign_keys=OFF
PASS 055 PRAGMA QuErY_OnLy(1)
PASS 056 PRAGMA trusted_schema=ON
PASS 057 PRAGMA schema_version=4
PASS 058 PRAGMA journal_mode=WAL
PASS 059 PRAGMA synchronous=OFF
PASS 060 PRAGMA compile_options(1)
PASS 061 PRAGMA database_list(x)
PASS 062 PRAGMA unknown_43
PASS 063 PRAGMA cache_size=17
PASS 064 denied PRAGMAs did not change baseline
PASS 065 table-valued object PRAGMA accepted
PASS 066 table-valued noarg PRAGMA accepted
PASS 067 table-valued disallowed PRAGMA denied at execution
PASS 068 unknown table-valued pragma unavailable
PASS 069 cached statement reprepare rechecks policy
PASS 070 reprepare error finalize consumes
PASS 071 warning: without policy PRAGMA mutates during prepare
PASS 072 autocommit initially on
PASS 073 BEGIN clears autocommit
PASS 074 ROLLBACK restores autocommit
PASS 075 COMMIT restores autocommit
PASS 076 cached write blocked after internal query_only ON
PASS 077 readonly finalize consumes
PASS 078 query_only write did not insert
LIMIT native LENGTH=1000000000 SQL_LENGTH=1000000000 COLUMN=2000 VARIABLE_NUMBER=32766 EXPR_DEPTH=1000
PASS 079 native column cap lower than common 4096
PASS 080 native parameter cap 32766 is lower than common 32767
PASS 081 lower parameter limit rejects sparse ?5
PASS 082 lower column limit rejects three
PASS 083 lower payload limit rejects bind before step
PASS 084 bounded progress abort (may occur in prepare)
PASS 085 interrupt active native statement
PASS 086 interrupt finalize consumes
PASS 087 post-interrupt native connection usable
PASS 088 file connection stays rollback journal (no auto WAL)
PASS 089 busy timeout zero
PASS 090 contended write returns BUSY without automatic retry
PASS 091 explicit later write after unlock succeeds once
PASS 092 close BUSY leaves connection live
PASS 093 close succeeds after statement released
PASS 094 close_v2 accepts with live statement (zombie)
PASS 095 zombie statement can finish
PASS 096 zombie final statement finalize releases deferred connection
PASS 097 private shutdown
PASS 098 host connection works after private shutdown
SUMMARY checks=98 failures=0
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
[timeout 30s] cc --version
[timeout 30s] uname -a
[timeout 30s] sw_vers
[timeout 120s] cc -std=c11 -O2 -fvisibility=hidden -DSQLITE_THREADSAFE=1 -DSQLITE_OMIT_LOAD_EXTENSION -DSQLITE_OMIT_SHARED_CACHE -DSQLITE_TRUSTED_SCHEMA=0 -DSQLITE_MAX_MMAP_SIZE=0 -c /tmp/rill43-reference-ngsv31hf/sqlite3.c -o /tmp/rill43-reference-ngsv31hf/unprefixed.o
[timeout 30s] nm -g -j --defined-only /tmp/rill43-reference-ngsv31hf/unprefixed.o
[timeout 120s] cc -std=c11 -O2 -fvisibility=hidden -DSQLITE_THREADSAFE=1 -DSQLITE_OMIT_LOAD_EXTENSION -DSQLITE_OMIT_SHARED_CACHE -DSQLITE_TRUSTED_SCHEMA=0 -DSQLITE_MAX_MMAP_SIZE=0 -include /tmp/rill43-reference-ngsv31hf/prefix.h -c /tmp/rill43-reference-ngsv31hf/sqlite3.c -o /tmp/rill43-reference-ngsv31hf/private.o
[timeout 30s] nm -g -j --defined-only /tmp/rill43-reference-ngsv31hf/private.o
[timeout 60s] cc -std=c11 -O2 -Wall -Wextra -Werror -I /tmp/rill43-reference-ngsv31hf -include /tmp/rill43-reference-ngsv31hf/prefix.h -c /Users/reilley/.t3/worktrees/rill/t3-310756ec-4fdcfd06e5f26ff9/prototypes/26-sqlite-provider/reference/checks.c -o /tmp/rill43-reference-ngsv31hf/checks.o
[timeout 30s] nm -g -j /tmp/rill43-reference-ngsv31hf/private.o
[timeout 30s] nm -g -j /tmp/rill43-reference-ngsv31hf/checks.o
[timeout 30s] nm -g -m --defined-only /tmp/rill43-reference-ngsv31hf/private.o
[timeout 60s] cc -std=c11 -O2 -Wall -Wextra -Werror -c /Users/reilley/.t3/worktrees/rill/t3-310756ec-4fdcfd06e5f26ff9/prototypes/26-sqlite-provider/reference/host.c -o /tmp/rill43-reference-ngsv31hf/host.o
[timeout 60s] cc /tmp/rill43-reference-ngsv31hf/checks.o /tmp/rill43-reference-ngsv31hf/host.o /tmp/rill43-reference-ngsv31hf/private.o -lsqlite3 -o /tmp/rill43-reference-ngsv31hf/checks
[timeout 30s] otool -L /tmp/rill43-reference-ngsv31hf/checks
[timeout 20s, cwd=/tmp/rill43-reference-ngsv31hf] /tmp/rill43-reference-ngsv31hf/checks /tmp/rill43-reference-ngsv31hf/disposable.db
```
