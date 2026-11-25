# Why analysis caps rise with every installed standard-library module

City bead ci-fm1i, 2026-11-25. Serves ri-qlh.23. Context: the rulings on ri-qlh.22.7 and ri-qlh.4.7, and the handoffs of city beads ci-u5cs, ci-d2dk and ci-p762. This report amends no rule and no RFC.

## Answer

1. A module costs every program because the loader puts it in every graph and the analysis charges for the whole graph. For programs that never call `log`, the presence of `stdlib/output.rill` is 98% of the typed delta and all of the graph and body delta; the compiler code that came with it is +196 to +231 typed work. Most of the cost is charged at linear scans and path-keyed probes of graph-wide tables. A targeted body cap moves because each `lower_body` builds its fact index by walking every typed node of every module.
2. No single option makes cost follow reach. Loading on explicit import works today for a module the facade does not re-export (measured: zero movement). It cannot work for `log`, `panic` or `append`. A measured-baseline pin does not hold still: the delta is not one constant (29 distinct typed deltas over 72 rows).
3. Recommended: before ri-qlh.22.12, place `env.rill` as an explicit-import source and add a re-pin mode to the test support. After ri-qlh.5.17, split the meter by origin (option D), built together with the charged lookups of ri-qlh.3.

## Sources and method

- Parent `A`: `origin/integration/m6-1-hello` at `60d7d061`. Candidate `B`: `m6-1-hello/std-log-panic` at `1666eeb3`, whose merge base with the integration branch is `60d7d061`. Code citations are `path:line` at `1666eeb3`; the cited files under `analysis/ownership/graph.rs`, `analysis.rs` and `analysis/declarations.rs` are identical at both revisions.
- **Measured**, on `aarch64-apple-darwin`, in two scratch worktrees, with a debug build:
  - The caps replay at `A` (3 passed) and at `B` (368 rows fail, section 1.1). `mise run test:m5:caps` was killed by SIGTERM after about 140 seconds three times on this host, cause not found, so both runs are the task's own test binary with its own filter (`legacy_caps`), started directly.
  - `mise run probe:m5` for the placement experiment `X` (section 2, option A).
  - A per-call-site tally. An uncommitted instrument adds `#[track_caller]` to the two `step` and `reserve` pairs (`analysis/declarations.rs:201`, `analysis.rs:463`) and to `module_index`, `charge_key` and `find_root` (`analysis/ownership/graph.rs:488`, `:498`, `:511`), and records each successful charge under its caller's file and line. It changes no charge: every total below equals the uninstrumented statistics (`baseline` typed 371,288 / 54,059 / 12 at `A` is the manifest row, `crates/compiler/tests/m5/legacy-caps.tsv:13`). A probe test runs `analyze_typed`, `analyze_ownership` and `lower_body` for each function, in both load modes, on seven programs: an empty one (`min`: `function f(): bool { return true; }`, no imports), two with one import, one with the test harness imports (`IMPORTS` in `crates/compiler/tests/m5.rs`), and the fixtures `baseline`, `slot-escape-good` and `fwcap-unrelated`.
  - Three more states of `B`: `Bm` (`B`'s compiler, `output.rill` removed from the pins, `std.rill` and the staged sources), which separates the module from the code that came with it; `X` (`output.rill` as an explicit-import source, not re-exported by `std.rill`); and the instrument alone.
  - Both load modes gave equal numbers in every row. The tables give one.
- **Read, not measured**: the loader, RFC 0002, the metering standard, `compilable.rs`, `typed/completion.rs`, and every statement about an option that was not built.
- Instrument, sources file, raw tallies and scripts: `~/.t3/evidence/rill/ci-fm1i/` (`instrument-on-1666eeb3.patch`, `sources.txt`, `tally-{A,Bm,B,X}.tsv`, `caps-{A,B}-direct.log`, `diff.py`, `classify.py`, `passes.py`).
- Not run: the full `m5` binary, the `m4_8*` binaries, any other platform. Not established: how the cost scales for a second module (sections 1.5 and 1.6 extrapolate from code).

## 1. Where the cost comes from

### 1.1 Totals

`work / entries / depth`. "Code" is `A` to `Bm`; "module" is `Bm` to `B`.

| Program | Query | `A` | Code | Module |
| --- | --- | --- | --- | --- |
| `min` | typed | 306,507 / 47,402 / 12 | +196 / 0 / 0 | +10,057 / +734 / 0 |
| `min` | graph | 185,530 / 4,132 / 5 | 0 | +7,319 / +361 / +1 |
| `min` | body `f` | 1,891 / 37 / 3 | 0 | +42 / 0 / 0 |
| harness imports | typed | 322,629 / 47,583 / 12 | +196 / 0 / 0 | +10,429 / +734 / 0 |
| `baseline` | typed | 371,288 / 54,059 / 12 | +230 / 0 / 0 | +11,127 / +734 / 0 |
| `baseline` | graph | 203,014 / 5,005 / 5 | 0 | +7,407 / +361 / +1 |
| `slot-escape-good` | typed | 487,992 / 55,867 / 12 | +231 / 0 / 0 | +13,499 / +734 / 0 |
| `slot-escape-good` | graph | 233,975 / 20,557 / 7 | 0 | +7,511 / +361 / 0 |
| `slot-escape-good` | body `joined` | 26,381 / 15,659 / 7 | 0 | +46 / 0 / 0 |
| `fwcap-unrelated` | body `outer` | 3,952 / 162 / 5 | 0 | +84 / 0 / 0 |

Three facts follow.

- **An empty program already costs 306,507 typed work and 185,530 graph work.** The standard library is nearly the whole of every cap. `min` loads all 17 pinned modules with no import (18 at `B`).
- **The code that came with the module costs +196 to +231 typed work and nothing else.** It is the `step(1)` in `InstalledOrdinaryScope::contains` (`analysis/typed/core/inventories.rs:36`; +178 in `scalars/generics/ordinary_calls.rs`, +18 in `typed/core.rs` for `min`). This settles ci-p762's concern (1).
- **The module delta depends on the program**: typed +10,057 to +13,499, graph +7,319 to +7,511. Section 1.5 says why.

The caps replay at `B` fails on 368 rows: 72 typed, 44 graph, 24 body, 156 root and 72 module counts. No outcome differs at default limits; depth moves in eight graph rows and nowhere else. ci-p762 reported the same counts.

### 1.2 The loader

The loader charges `Host::charge` against the load `Limits` of the request, not `AnalysisLimits` (`loader/resolve.rs:178`, `loader/filesystem.rs:53`). It adds nothing to a typed, graph or body cap. What it decides is membership.

- In installed mode every source in `InstalledProfile::sources()` that is not the path of a metadata edge is a "baseline" module and is added before any import is read (`loader/installed.rs:155`, `loader/resolve.rs:177` to `:196`). `output.rill` was added to that list.
- `std.rill` is in the baseline, and its `export import output::{log, panic};` is discovered like any import. So is every other module a baseline module imports. This is why `min` loads `option.rill`, `collection.rill`, `iter.rill`, `format.rill`, `string.rill` and `bytes.rill`, which are metadata-edge paths.
- One source is outside the baseline: `routine.rill` is returned by `InstalledProfile::source` only (`loader/installed.rs:147`: "Explicit import discovery can capture this extension. It is not a member of the baseline required by every admitted role."). `slot-escape-good` imports it and loads 18 standard modules where `baseline` loads 17.
- At `B`, any `import std::X` also adds `std.rill` as a candidate start (`loader/resolve.rs:989`), so a module that `std.rill` re-exports is loaded by every program.

### 1.3 Typed: +10,057 work and +734 entries for `min`

By pass (files charged; `Bm` to `B`):

| Pass | `min` before | Work | Entries | `slot-escape-good` work |
| --- | --- | --- | --- | --- |
| Declaration types and names (`analysis.rs`, `lexical.rs`) | 134,369 | +6,028 | +241 | +6,915 |
| Declarations (`declarations.rs`) | 3,946 | +90 | +24 | +90 |
| Ordinary inventory (`ordinary.rs`, `ordinary/`) | 49,327 | +369 | +82 | +482 |
| Conversions (`conversions.rs`) | 900 | +29 | 0 | +29 |
| Bodies (`scalars.rs`, `scalars/`) | 72,186 | +2,497 | +272 | +2,720 |
| Installed core roles, providers, completion (`typed/*`) | 26,962 | +723 | +9 | +796 |
| `result_use` (`typed/result_use*`) | 19,013 | +321 | +106 | +2,467 |
| Total | 306,703 | +10,057 | +734 | +13,499 |

By kind of charge. 187 distinct call sites moved; the classification is by the loop each `step` sits in.

| Kind | `min` before | Work delta | Share of `min` before |
| --- | --- | --- | --- |
| Linear scan of `graph.bindings()`, `step(1)` per binding | 95,521 | +5,625 | 31% |
| Linear scan of `graph.implementations()` | 27,690 | +1,388 | 9% |
| Linear scan of `graph.modules()` | 20,888 | +450 | 7% |
| Scan of the prelude intrinsics | 5,521 | +222 | 2% |
| Scan of a module's declarations | 9,021 | +167 | 3% |
| Insert of every typed node | 1,823 | +42 | 1% |
| Per node, declaration or body | 146,239 | +2,163 | 48% |

The largest sites:

| Site | Work | What it is |
| --- | --- | --- |
| `Analyzer::namespace_binding`, `analysis.rs:710` | +4,740 (83,039 before) | Walks all bindings of the graph for one type name. 2,310 is `output.rill`'s own lookups; 2,430 is the other 17 modules' lookups walking six more bindings (four in `output.rill`, two in `std.rill`). |
| `evidence_module`, `scalars/generics/evidence.rs:500` | +1,056 | Walks the modules to find one. `output.rill` sorts before `result.rill`, `shared.rill`, `std.rill` and `string.rill`, so each search for those takes one more step. |
| `value_binding`, `scalars.rs:1462` | +518 | The same walk of all bindings, for a value name. |
| `index_lexical`, `lexical.rs:21` and `:29` | +480, +161 entries | Per syntax node of the new module (151 nodes, and nine more in `std.rill`). Own cost. |
| `export`, `typed/core.rs:131` | +309 | Walks all bindings per installed role. |
| `named_target`, `analysis.rs:807` | +222 | Intrinsic scan for `output.rill`'s own type names. Own cost. |
| `bridge_header`, `typed/collections.rs:816`; `check_region`, `:571`; `eligible_captured_family`, `ordinary/selection.rs:387` | +136, +52, +84 | Walk all implementations (130 to 134). |
| `target_declaration`, `analysis.rs:724`; `owner`, `:559`; `collect`, `ordinary.rs:664` | +135, +64, +66 | Walk the modules. |

All 734 entries are per node or per declaration of the new source (`retain_type`, `lexical.rs:21`, the typed-node tables). Entries are own cost.

So about 7,900 of the 10,057 is charged at scans whose length is the installed set, and at least 2,430 of that is paid by modules that have nothing to do with `output.rill`. The typed stage cannot skip the module either: `accounted_for` requires a checked body for every body root of every loaded module before it grants `Checked` (`typed/completion.rs:105`), which is ci-u5cs's finding that typed completeness is global.

### 1.4 Graph: +7,319 work and +361 entries for `min`

| Site (`analysis/ownership/graph.rs`) | Work | What it is |
| --- | --- | --- |
| `module_index` per typed node, `:621` (loop at `:619`) | +3,024 (129,708 before) | 42 new typed nodes, each `(path length + 1) × probes` = 12 × 6 = 72. |
| `charge_key(&id.module, family_count, ..)`, `:667` | +2,520 (11,700 before) | Each family with implementations pays its path width times the number of module-scope families. Families 75 to 79: 4 × 156 for the old families, 2 × 12 × 79 for `log` and `panic`. Quadratic. |
| `module_index` per checked body, `:629`; per implementation, `:655`; `find_root`, `:672` | +360, +288, +292 | 5 bodies and 4 implementations at 72 or 73 each. |
| `step` and `reserve` per typed node, `:620`, `:622` | +84, +42 entries | |
| Lowering the five new roots (`flow.rs`, `build.rs`, `scalar.rs`) | +644, +292 entries | `log(string)` 179, `log(&string)` 174, `panic(string)` 138, `panic(&string)` 133, startup 20. |
| Root list (`:585`), closure (`:849`) and publication | +107, +27 entries | |

Path-keyed module probes are 172,391 of the 185,530 graph work of an empty program (93%), and 6,532 of the 7,319 delta. Lowering the new roots is 644. The roots are every startup and every function, implementation and expectation of every loaded module (`graph.rs:585`: "No typed facts define membership"), and the node index is built for every typed node before any root is lowered.

The delta rises to +7,511 for `slot-escape-good` because more application implementations pay the larger `family_count` and more scans of `graph.implementations()` (+32 against +4).

### 1.5 Body: why a targeted cap moves at all

`Build::new` builds the fact index of a targeted query by walking every typed node of the capture and keeping those of one module (`analysis/ownership/build.rs:505`, one `step(1)` per node). For `f` in `min` that walk is 1,823 of the 1,891 work; `output.rill` adds 42 typed nodes, so it adds 42.

- A query that lowers callees builds the index again for each: `outer` in `fwcap-unrelated` calls `middle` and pays +84.
- The remainder is `eligible_captured_family` walking four more implementations (`ordinary/selection.rs:387`): +4 in `joined`, +12 in `mapSuccess`.

The replay at `B` agrees: 76 of 156 `root` rows move by 42, 36 by 84, 34 by 126, and the `body` rows by 46 to 58. No entry and no depth moves, and the code charge is zero.

### 1.6 What the next modules will cost (read, not measured)

- Each new baseline module of this size: about +10,000 to +13,500 typed and +7,300 graph again. The family term in 1.4 grows with the square of the family count.
- `append` goes into `string.rill`, which every program already loads. No module is added, and every cap still moves: new bindings lengthen every failing binding scan, new typed nodes pay 72 each in the graph and 1 in every targeted fact index.
- **A cliff at 32 loaded modules.** `module_index` charges `usize::BITS - modules.len().leading_zeros() + 1` probes (`graph.rs:493`): 6 for 16 to 31 modules, 7 from 32. Programs load 19 to 21 modules at `B`. The 32nd adds one probe to every typed node of every module, about a sixth of the probe work (some 26,000 graph work for an empty program). `declaration_at` (`analysis.rs:480`) has the same step at 1,024 declarations (668 at `B`).

## 2. Options

Rule numbers are those of [the metering standard](../agents/compiler-metering.md). "Installed contract" is [RFC 0002](../rfcs/0002-modules-and-program-loading.md). "Compilable rule" is `analysis/compilable.rs`. "Ownership" asks whether the option can be built without touching `crates/compiler/src/analysis/ownership/`.

### A. Load a standard module only when the program imports it

**What changes.** A new module is returned by `InstalledProfile::source` (as `routine.rill` is) and is not in `sources()`. `std.rill` does not re-export it. Its typed roles bind only when it is present, as `output.rill`'s already do (`typed/core/scalar.rs:350`, `pinned_module` returns `None`).

**Measured (`X`).** With `output.rill` placed that way, all 92 statistics rows of the seven programs equal `Bm`: no cap moves except by the +196 to +231 code charge. `import std::output::log; log("hello world");` loads 18 modules, is typed `Checked`, and `mise run probe:m5` reports `root main.rill startup Checked mir yes`.

| | |
| --- | --- |
| Passes | Loader only. |
| Rules 1, 3, 11 | None. No published cap moves; tests that import the module pin new caps. |
| Installed contract | Conforms. "Use only explicit imports, fixed prelude roots and declared metadata edges" (`0002:422`). `routine.rill` is the precedent. |
| Compilable rule | None. An absent module has no bodies. |
| Still moves caps | Anything `std.rill` re-exports (`log`, `panic`: the contract's path is `std::log`, and `import std::log` fails to load in `X`). Anything added to a module every program loads (`append` in `string.rill`). Any new charge in compiler code. |
| Size | About five lines in `loader/installed.rs` and one in the staged source list. |
| Ownership | Not touched by the placement. |

It fits `env.rill` exactly: the acceptance program writes `import std::env::{inputs, args};`, and the contract says "All new types/functions are exported from `std::env`; none enters the fixed prelude" (`docs/standard-library-contracts.md:666`).

The same idea at the grain of a declaration (analyse only the bodies a program reaches, in modules that stay loaded) is option B.

### B. Analyse only the bodies a program reaches

**What changes.** Every module still loads. Typed checks and ownership lowering of a standard body run only when an application root reaches it.

| | |
| --- | --- |
| Passes | Every typed pass that loops over modules or declarations; `typed/completion.rs:105`; the root list and node index in `ownership/graph.rs:585` and `:619`. |
| Rules 1, 3, 11 | One re-pin of every cap, downwards (rule 11, third case, with the "lowers the cost" note). Rule 1 unchanged. |
| Installed contract | Changes it. "Errors in an unused loaded body still reject the program" (`0002:207`), "Check all loaded implementations, including unused bodies" (`0002:396`) and the completion gate (`0002:457`) would hold for pinned first-party bodies only through a check at toolchain build. That is an RFC amendment, so an owner decision. |
| Compilable rule | Its reachability walk is unchanged. "The source carries a finding, in a reachable body or not" (`compilable.rs:54`) would hold for application bodies only. |
| Still moves caps | Laziness does not remove the table scans of 1.3: six more bindings lengthen every failing lookup whether or not `output.rill` is analysed. Every cap still moves until those lookups are indexed. |
| Size | Large: the typed drivers and the graph. Weeks. |
| Ownership | Must touch `ownership/graph.rs`. |

### C. A cached or precomputed standard receipt

**What changes.** The typed facts and MIR of the pinned sources are computed once per toolchain and reused.

| | |
| --- | --- |
| Passes | All of typed and the graph: both are built over one capture, and ids are bound to it (`validate_request`, `ownership.rs:1246`). |
| Rules 1, 3, 11 | Rule 7 governs: the key must name the pinned bytes, the target, the options, the load mode, and every application implementation that joins a standard family (collection, R0304 duplicates, R0500 overlaps). A cap must not depend on a cache hit, so the cached work is either charged as a constant (every cap still moves when it changes) or excluded (which is option D). |
| Installed contract | Changes it: "Attach private evidence only to that graph" and "The checker consumes the capture without reloading files" (`0002:426`). |
| Compilable rule | Standard bodies come from the cache; the walk is unchanged. |
| Still moves caps | As D if excluded, as today if charged. The table scans of 1.3 remain. |
| Size | Very large: a serialisable receipt and MIR, and an invalidation proof. |
| Ownership | Must touch it. |

It buys wall-clock time, which nobody has asked for. For caps it reduces to D.

### D. A separate budget for the standard library

**What changes.** `Statistics` and `AnalysisLimits` carry two meters. Work is charged to the origin of what it touches: a scan step to the origin of the element scanned, a `charge_key` to the origin of the key, per-node and per-body work to the origin of the module being analysed. Tests pin the application meter. The standard meter is pinned for one reference program per load mode and otherwise has a ceiling.

| | |
| --- | --- |
| Passes | `Budget` (`analysis/declarations.rs:196`) and `Analyzer` (`analysis.rs:440`); about 20 scan sites in typed (1.3); the module loops of each typed pass; in ownership, `charge_key` (the key carries its origin), the root loop (`graph.rs:688`) and `Build::new` (`build.rs:505`). |
| Rule 1 | Still checkable on the application meter. A new path that pays on standard-attributed work before it rejects is invisible there, so the parent-cap assertion must run with both meters at the parent's values. |
| Rule 3 | Unchanged; the charge goes to the key's origin. |
| Rule 11 | Needs an amendment: a cap is two triples, `ResourceLimit` is either meter running out, and a standard-library addition moves one reference row. A change to a rule set for the work is the chief's to make (owner policy 5). |
| Installed contract | Unchanged: every loaded body is still checked for every program. |
| Compilable rule | Unchanged. |
| Still moves caps | The reference row. Application pins move only if an attribution leaks. Known leaks to close: the logarithmic charges on whole-graph sizes (`graph.rs:493`, `analysis.rs:481`), prepayments by table length (`implementation_contract` in `scalars/generics/evidence.rs`, `rejected_input` in `typed/completion.rs`, `result_use/pairs.rs:334` and `:341`), and `family_count` (`graph.rs:667`). Depth is a maximum, so the application depth is the maximum over application-attributed bodies. |
| Size | Medium to large: about 40 charge sites, the public `Statistics` type, and one re-pin of every pin downwards. Its done-when is a test: adding a standard module moves no application pin. |
| Ownership | The typed half needs no ownership file. Graph and body pins need `graph.rs` and `build.rs`. |

The scan sites it touches are the ones ri-qlh.3 (charged lookup wrappers) will touch. Indexing those lookups first removes most cross-terms and about half of typed work, and makes the attribution a matter of which module is being analysed.

### E. Pins relative to a measured standard baseline, in the test support

**What changes.** The support measures a reference program in the same installation and each pin is written as that baseline plus a residual. No production change.

**Measured obstacle.** The delta is not one constant. Over the 296 cap triples the replay at `B` fails on (`A` to `B`, so the code charge is included):

| Query | Rows | Distinct deltas | Work delta | Largest group |
| --- | --- | --- | --- | --- |
| typed | 72 | 29 | +10,599 to +13,775 | 6 rows |
| graph | 44 | 8 | +7,319 to +7,519 | 12 rows (16 with the four that also move depth) |
| body | 24 | 3 | +46 to +58 | 12 rows |
| root | 156 | 7 | +42 to +222 | 76 rows at +42 |

The seven probe programs agree (1.1). With one baseline constant per query about a third of the rows hold still and every typed row but six still moves. A model that holds them all needs the application's count of failing lookups, family implementations and fact-index builds as parameters, which is the analysis again.

| | |
| --- | --- |
| Passes | None. |
| Rule 2 | Weakened: its check says sibling caps "cite the parent commit and are not re-measured on the candidate", and a relative pin re-measures the baseline on the candidate. A regression inside standard-library analysis hides in the baseline. |
| Rule 11 | Its "single line in the manifest" and literal exact caps become derived numbers. |
| Installed contract, compilable rule | Unchanged. |
| Still moves caps | About two thirds of the rows, by tens to thousands. |
| Size | Small in the support, plus rewriting about 500 pins once. |
| Ownership | Not touched. |

It becomes workable only after the cross-terms are gone, which is most of D.

### F. A re-pin mode in the test support

Not an answer to "cost follows reach", but the only thing besides A that lands without production code. ci-p762 stopped on scale: 237 of 587 `m5` tests pin caps in source, and about 95 of them hold a bare `AnalysisLimits` literal with no statistics to read the new value from. A report mode in `crates/compiler/tests/support/limit.rs` that prints, for every pin site, the measured triple at default limits turns the standing ruling's one-commit re-pin (Q2 on ri-qlh.22.7) into a mechanical step. Rules unchanged. Size: small to medium, test support only.

## 3. Recommendation

1. **Before ri-qlh.22.12: A for `env.rill`, and F.** Place `env.rill` as an explicit-import source so `inputs` and `args` move no cap. Build the re-pin mode first, then do the `output.rill` re-pin with it. `append` will need one more re-pin, because `string.rill` is in every graph. Neither step touches production analysis or `ownership/`.
2. **Do not adopt E as stated.** The measurement refutes "one constant moves".
3. **After ri-qlh.5.17: D, sequenced with ri-qlh.3.** Index the typed lookups under rule 3 first (one downward re-pin, about half of typed work), then split the meter. The typed half can start before ri-qlh.5.17. Its acceptance is a test that a new standard module moves no application pin.
4. **B and C are not recommended.** Both change what RFC 0002 says is checked per program, both must edit `ownership/graph.rs`, and neither removes the table scans by itself.
5. **Watch the cliff.** The 32nd loaded module adds a probe to every typed node in the graph stage. Slice 1's programs stay near 22.

## Not established

- The cost of a second or third module. Section 1.6 reasons from the code.
- Whether the origin attribution of D can be made exact at every site. The leaks listed are the ones found by reading; the acceptance test is what would prove it.
- Any number on another platform. The standard says M5 evidence covers both installed load modes; both were measured here, on one host.
- The in-test pins. I measured manifest fixtures and probe programs, not the 237 tests.
