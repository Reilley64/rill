# ri-vgz.5: a `string` error forwarded unchanged and retired once, 2026-11-23

Ticket ri-vgz.5 (gap-analysis C3). M5 now admits one exact owning error type, `string`, through propagation. It covers `Result<bool, string>` and `Result<string, string>` (`build.rs`, `text_error_result_role`). Before this change, M5 stopped at the first parameter of either type.

- **Forwarding.** A postfix over a `Result<bool, string>` moves the `bool` on `Ok`. On `Err` it moves the existing wrapper (`ForwardResultFailure`) unchanged into a `Result<bool, string>` return and exports it before lexical cleanup. Returning the whole carrier exports it as it is.
- **Retiring the held error.** In `input ? (source?)`, `input` is a held `Result<bool, string>.Err` and `source` a `Result<string, string>.Err`. The RHS always fails, so its wrapper is forwarded and exported unchanged. Then `DropResultFailure` retires the held `input` once, using the pinned `OwnedErr` storage drop, before lexical cleanup. No new wrapper is built, so no second origin is added: the only exported origin is the one `source` carries. The `OwnedErr<String>` drop signature is bound in the source next to the existing `OwnedErr<i32>` one.

All results hold in both installed load modes, targeted (`lower_body`) and as the ordinary graph root. Each runs at the exact caps below. One short on any axis gives `ResourceLimit` with no MIR, and targeted queries publish no finding. Tests are in [`propagation/text_err.rs`](../m5/propagation/text_err.rs):

- `string_error_is_forwarded_unchanged_and_the_held_error_is_retired_once` asserts the whole ordered effect list of each path, in targeted and graph-root MIR. For `retires`, the list is: move `input`, move `source`, forward `source`, export, retire `input`, drop `tail`. That is exactly one `DropResultFailure`, after the export, with the pinned storage source.
- `string_error_cases_keep_their_outcomes_at_exact_caps` pins the table below.

| Source (`probe`) | Outcome | Targeted | Graph |
| --- | --- | --- | --- |
| `let flag = input?;` | `Checked` | 3,521 / 522 / 4 | 187,456 / 4,483 / 4 |
| `((input))?` | `Checked` | 3,529 / 524 / 5 | 187,584 / 4,485 / 5 |
| `(input?)` | `Checked` | 3,524 / 523 / 4 | 187,519 / 4,484 / 4 |
| alias `held?` | `Checked` | 3,650 / 586 / 4 | 187,706 / 4,547 / 4 |
| statement `input?;` | `Checked` | 3,372 / 444 / 4 | 187,306 / 4,405 / 4 |
| known `.Err` | `Checked` | 3,204 / 353 / 3 | 187,139 / 4,314 / 3 |
| known `.Ok` | `Checked` | 3,316 / 409 / 4 | 187,251 / 4,370 / 4 |
| `return input;` | `Checked` | 3,077 / 279 / 3 | 186,771 / 4,240 / 3 |
| `return (Result.Ok(flag));` | `Checked` | 3,526 / 523 / 5 | 187,521 / 4,484 / 5 |
| if/else | `Checked` | 4,374 / 991 / 5 | 188,730 / 4,952 / 5 |
| `while (true)` | `Checked` | 3,636 / 596 / 5 | 187,811 / 4,557 / 5 |
| `let _ = input ? (source?);` | `Checked` | 4,794 / 668 / 4 | 189,029 / 4,629 / 4 |
| `let flag = input ? (source?);` | `Checked` | 4,885 / 722 / 4 | 189,181 / 4,683 / 4 |
| `((input)) ? ((source)?)` | `Checked` | 4,806 / 671 / 5 | 189,221 / 4,632 / 5 |
| `input ? ((source?))` | `Checked` | 4,797 / 669 / 5 | 189,092 / 4,630 / 5 |
| retire in both branches of if/else | `Checked` | 5,828 / 1,075 / 6 | 190,603 / 5,036 / 6 |
| second `input?` | `R0200` at `input`, related `input` | 3,473 / 509 / 4 | 187,591 / 4,471 / 4 |
| `source ? (source?)` | `R0200` at the RHS `source`, related the LHS `source` | 4,602 / 567 / 4 | 188,658 / 4,529 / 4 |
| `input?` under a live `&input` | `R0203` at `input`, related `&input` | 3,279 / 382 / 4 | 187,396 / 4,344 / 4 |
| `input ? tail` (new `string` wrapper) | `Incomplete` at `input ? tail` | 2,996 / 240 / 4 | 186,991 / 4,201 / 4 |
| `input ? (carryFailure(source)?)` | `Incomplete` at `carryFailure(source)` | 4,481 / 504 / 4 | 190,721 / 4,722 / 4 |
| `Result<string, string>` postfix that can continue | `Incomplete` at `input?` | 4,081 / 430 / 4 | 188,016 / 4,391 / 4 |
| `return Result.Ok(input ? "outer");` | `Incomplete` at the return value | 2,949 / 215 / 3 | 186,762 / 4,176 / 3 |

At the parent `a95e04e8`, every positive row stopped at its first `Result<_, string>` parameter (for example `forwards` at 1,852 / 17 / 0, graph 185,787 / 3,978 / 3). Both new tests fail there; log `red-without-fix.log`. The last row is the parent-cap regression (metering rule 2). It was measured at `a95e04e8` and passes there unchanged (`parent-cap-row-at-a95e04e8.log`). A `Result<_, string>` return whose producer can itself return is screened from paid flow facts before its return storage is proved. So the installed capture's `wrap` keeps its targeted and graph caps. Without that screen, the 17-source graph rose from 202,698 / 4,871 / 3 to 203,802 / 5,068 / 3, and 22 `legacy_caps` graph rows moved. With it, nothing moves.

**Fixed caps unchanged.** These pass with no edited expectation:

- `legacy_caps`, `rfc_cases` and `graph::inventory`. The 17-source graph stays 202,698 / 4,871 / 3 and the 18-source graph 243,206 / 25,960 / 10; no root flips.
- All of `propagation::`, `m4_8_scoped_join_caps` and `m4_8_match_boolean_snapshots`.

The scalar `DropResultFailure` case `wrapsAfterRhs` keeps 12,574 / 2,472 / 6. `returning_wrap_rhs_forwards_instead_of_wrapping` now asserts that triple; the test already checked it exact and one short.

**Not established:**

- **A new wrapper.** No new wrapper is built with a `string` payload (`input ? tail` stays `Incomplete`), so there is no heterogeneous wrap, canonical `display`, or erasure evidence for a `string` cause.
- **Calls.** A call RHS and a call producer of a `Result<_, string>` stay `Incomplete`.
- **Owning success payloads.** A `Result<string, string>` whose `Ok` can continue stays `Incomplete`.
- **Live carriers.** Lexical cleanup of a live `Result<_, string>` carrier is not lowered (`StorageCleanup::Unlowered`). M4 issues no receipt for a body that leaves one unconsumed (`Containment`), so variant-aware disposal of a live carrier remains open.

This is LLVM-free host ownership analysis; no Rill program ran.
