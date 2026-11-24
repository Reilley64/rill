# ri-vgz.3: exclusive `Option<&mut bool>` across an ordinary relay, 2026-11-23

Ticket ri-vgz.3 (gap-analysis C1), commit `10a9def7`. `let view = forward(input)?;` over an owned `Option<&mut bool>`, where `forward` is the private one-formal relay `let view = input?; return Option.Some(view);`, now takes the selected shared relay route (`build/option_calls.rs`, A3) with the exclusive carrier. The named carrier is **moved** into the call stage, not copied; `Some` takes the staged descriptor with every actual input loan, including the exclusive one; `None` ends the empty stage and exports canonical `None`. The callee body is checked in the caller's frame, as for the shared relay. The screen is unchanged and runs before the first charge, so unsupported exclusive siblings keep their parent caps.

All results hold in both installed load modes, targeted (`lower_body`) and as the caller's ordinary graph root, at the exact caps below; one short on any axis gives `ResourceLimit` with no MIR and no finding (an axis measured at 0 has no one-short cap). Tests: [`exclusive_option_caller.rs`](../m5/propagation/exclusive_option_caller.rs).

| Caller | Outcome | Targeted | Graph |
| --- | --- | --- | --- |
| `callerPostfix`: `forward(input)?` | `Checked`; root `Checked`, one `OrdinaryCall` edge to `forward` | 9,404 / 1,651 / 4 | 195,872 / 6,109 / 4 |
| `callerGroup`: `((forward(((input)))))?` | `Checked` | 9,424 / 1,659 / 8 | 196,124 / 6,113 / 8 |
| `reuse`: `return input;` after the relay | `R0200` at `input`, related `input` | 5,537 / 960 / 4 | 195,468 / 5,967 / 4 |
| `conflict`: `let _ = &flag;` while the descriptor lives | `R0201` at `&flag`, related `&mut flag` | 6,970 / 1,336 / 5 | 197,442 / 6,343 / 5 |
| `escape`: returning the descriptor of a local | `R0202` at `return Option.Some(view);`, related `flag` | 5,931 / 1,182 / 5 | 196,283 / 6,189 / 5 |
| `release`: `dispose(flag)` while the descriptor lives | `R0203` at `dispose(flag)`, related `&mut flag` | 7,086 / 1,345 / 5 | 197,678 / 6,352 / 5 |
| `dead`: `dispose(flag)` after the last use | `Checked` | 13,752 / 2,461 / 5 | 198,511 / 6,655 / 5 |

At the parent `b5e361de`, `callerPostfix` was `Incomplete` at `forward(input)?` (3,063 / 271 / 4, graph 193,052 / 5,277 / 4). These stay `Incomplete` at their sites with the parent's caps: `caller` (`return forward(input);`, at `forward(input)`, 3,025 / 253 / 4), `otherPayload` (`Option<&mut i32>`, at its parameter, 1,919 / 51 / 0), and the computed, piped, grouped-piped, generic and two-input relays.

**Re-pin under metering rule 11.** A3's `callerExclusive` (`relayExclusive(input)?` in [`option_caller.rs`](../m5/propagation/option_caller.rs)) is the shape this ticket admits. It moves from `Incomplete` at targeted 3,089 / 284 / 4 and graph 195,691 / 5,792 / 4 (measured at `cb94538f`) to `Checked` at 9,466 / 1,677 / 4 and 198,521 / 6,624 / 4. The whole difference (+6,377 / +1,393 targeted, +2,830 / +832 graph, depth unchanged) is the `callerExclusive` root: its relay call, the checked callee frame and the success and failure edges. It is the only root in that fixture with a relayed call. `exclusive_relay_caller_exceeds_its_parent_caps` asserts that the old caps now give `ResourceLimit`. No other cap moved: `legacy_caps`, `rfc_cases`, `m4_8_scoped_join_caps` and `m4_8_match_boolean_snapshots` pass unchanged.

Not established: an exclusive relay whose callee is public, scoped, generic or has another signature (it stays `Incomplete` after the header lookup, as for the shared relay); `return forward(input);` without a postfix; any payload other than `&mut bool`. This is LLVM-free host ownership analysis. No Rill program ran.
