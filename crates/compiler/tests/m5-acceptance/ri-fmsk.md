# ri-fmsk (R-O5, review SP-F5): blocked, 2026-11-23

City bead `ci-5lzj`, branch `m5-ownership-cleanup/r-o5-ri-fmsk`, parent `7002855e`. **Outcome: blocked.** The O5 row of `m5::rfc_cases` stays `Incomplete`. This branch is a work-in-progress commit that must not merge as it stands: `m5::rfc_cases::o05_disjoint_field_loans_check_and_field_arithmetic_blocks` fails on it, because the blocker has moved.

## Finding: the ticket's premise is wrong

The ownership checker builds a body backwards, so the `return` is lowered before the statements above it. `pair.left + pair.right` was only the first stop. With field arithmetic lowered, `separate` still stops `Incomplete`, now at `*right += 2i32;`. M5 does not lower writes or reads through `&mut i32`, nor compound assignment outside the counter closure (`build.rs`, the `Assignment("+=")` match in the cell-closure path). Probes through the installed compiler at the parent (all in `main.rill` with `type Pair = { left: i32; right: i32 }`, and a typed receipt in every case):

| Probe body | Outcome at the parent | Blocker |
|---|---|---|
| `let mut x = 1i32; let r = &mut x; *r += 1i32; return x;` | Incomplete | `*r += 1i32;` |
| `let mut x = 1i32; let r = &mut x; *r = 2i32; return x;` | Incomplete | `*r` |
| `let left = &mut pair.left; *left = 5i32; return pair.left;` | Incomplete | `*left` |
| `let left = &mut pair.left; let v = *left; return v;` | **Checked** | none |
| `let left = &mut pair.left; let v = *left + 1i32; return v;` | Incomplete | `*left + 1i32` |
| `pair.left += 1i32; return pair.left;` | Incomplete | `pair.left += 1i32;` |
| `let left = &mut pair.left; *left += 1i32; return pair.left;` | Incomplete | `*left += 1i32;` |
| O5 `separate` with the return replaced by `return 3i32;` (no field arithmetic) | Incomplete | `*right += 2i32;` |

Getting O5 to `Checked` needs new ownership lowering for compound assignment and for reads and writes through an exclusive reference to a scalar field, with the matching transfer rules in `flow.rs` and disjoint-field loan state. That is the same `*value += 1i32` write-through-`&mut` work that `ri-1up` (O6, O8) needs on the ownership side, not one proof path. `m5-status.md` attributes O5's stop to field arithmetic alone; that row is wrong and is left unedited here.

## What the branch holds

- `scalars.rs`: a new `I32Operand::Field` class: an `i32` field read of a local record (`Access::Owned`, one `Field` projection, root a parameter or binding, through groups). It is an operand of `+` only. It is not an operand of `*` or of equality. The screen reads facts already paid for, before any charge (rule 1).
- `ownership/build/i32_arithmetic.rs`: the addition reads a field operand into a scratch slot through the ordinary expression path (no new keyed lookup, rule 3 unaffected), adds, then ends the scratch slots, left before right. Locals keep their charge: the `arithmetic_local` split charges nothing new (a first draft double-charged one step; `counter_expectation_checks_past_its_incomplete_cap` caught it, and it passes).
- `m5/o05_field_arithmetic.rs`: `unsupported_field_arithmetic_keeps_its_parent_caps_and_sites` and `field_addition_checks_with_exact_and_one_short_caps`. Both pass in both load modes.

## Caps

Fixed caps left unchanged (all pass): `o15_arithmetic::*`, including the counter and equality caps.

Parent caps of the unsupported siblings, measured at `7002855e` before any source change, and pinned unchanged on the branch (targeted exact and one short; graph root `Incomplete` at the same site):

| Source | Work / entries / depth | Blocker |
|---|---|---|
| `nested`, `nestedGrouped` (`outer.inner.left + outer.extra`) | 2,746 / 223 / 3 | the `+` expression |
| `wide`, `wideGrouped` (`i64` fields) | 2,383 / 164 / 3 | the `+` expression |
| `product` (`pair.left * 10i32 + pair.right`), `literal` (`pair.left + 1i32`) | 2,383 / 164 / 3 | the `+` expression |
| `lent`, `lentGrouped` (`pair: &Pair`) | 2,025 / 105 / 3 | the `+` expression |
| whole graph of that module | 195,115 / 4,770 / 5 | `Incomplete` |

Supported control `bare` (`let pair = Pair { left: 1, right: 2 }; return pair.left + pair.right;`): `Checked`, 2,711 / 328 / 4, MIR order copy `left`, copy `right`, add.

**Cap movement that blocks the ticket.** The O5 row moves although it stays `Incomplete`: `separate` 2,281 / 116 / 3 to 2,302 / 123 / 4 (blocker `pair.left + pair.right` to `*right += 2i32;`), whole graph 188,006 / 4,283 / 5 to 188,027 / 4,290 / 5 (the expectation root follows). This is not a ticket-mandated flip (rule 11, third case) and the old-cap-gives-`ResourceLimit` assertion would assert an unflipped row. No cap was re-pinned on this branch.

## Not established

Nothing about `separate` or its expectation being `Checked`. Grouped field operands (`((pair.left)) + ((pair.right))`) and mixed local and field sums are implemented by the same path but not pinned. The hosted check did not run.
