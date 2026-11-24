# K3: a lock guard is released once on a propagation exit (ri-2s0.13, 2026-11-24)

Ticket R-K3P (bead `ci-iq6w`). Required row K3 of the gap analysis: the canonical provider releases a lock guard exactly once. The statechart experiment `ri-h4ec` found no witness on a propagation exit. The two bodies that own a guard across a propagation, `protectedArg` and `protected`, are asserted `Incomplete` with no MIR.

## Result: the base already lowered the shape; this ticket adds the missing witness

No compiler source changed. A bare `LockGuard<i32>` parameter that exits through a postfix `?` is `Checked` at the parent `b6c93f30`, as a targeted query and as an ordinary graph root, for `Option` and `Result` carriers. The gap was a missing test, not missing lowering. The same figures hold at the merged base `af420b18`.

Consequences:

- No `Incomplete` expectation flipped, so no cap moved and metering rule 11 was not used. Every figure below was measured at the parent with no source change, so each is a parent cap (rule 2), not a re-pin.
- No standing rule-1 exception was used. The ticket adds no source path, so nothing charges before it rejects.
- The two existing `Incomplete` pins stay as they are and unedited: `protectedArg` in `propagation::compound_propagating_results_stay_outside_other_call_routes` (the guard is passed into a call after a propagating argument) and `protected` in `records::interrupted_record_initialization_keeps_unreached_and_unsupported_shapes_separate` (the guard is a record field). Neither is a bare-parameter propagation exit, and this ticket does not cover them.

## Test

[`propagation::guard_release::bare_guard_is_released_once_on_each_propagation_exit`](../m5/propagation/guard_release.rs), in both installed load modes, asserting the targeted query (`lower_body`) and the ordinary graph root (`root.mir()`) against one list per exit (metering rule 9). The child log shows `running 1 test` for this exact name (`installed` rejects a child that runs none).

```rill
function forward(input: Result<bool, i32>, lock: LockGuard<i32>): Result<bool, i32> {
    let value = input?;
    return Result.Ok(value);
}
```

Whole MIR sequence of `forward`, as `(operation, place)` pairs. Both exits share one release block.

| Failure exit (`?` forwards the error) | Normal exit |
| --- | --- |
| `Move input: Result<bool, i32> -> input` | `Move input: Result<bool, i32> -> input` |
| `TestVariant input` | `TestVariant input` |
| `ForwardResultFailure input -> input?` | `TakeResultSuccess input` |
| `Export input?` | `Copy value -> value` |
| `Cleanup []` | `ConstructVariant Result.Ok(value) payload=true` |
| `Cleanup [ReleaseGuard(lock: LockGuard<i32>)]` | `Export Result.Ok(value)` |
| | `Cleanup [EndStorage(value)]` |
| | `Cleanup [ReleaseGuard(lock: LockGuard<i32>)]` |

The failure is exported before cleanup, as RFC 0008 requires, and the guard is released once on each exit. Every `ReleaseGuard` carries the `implementation` and `entry` of the clear required guard-release provider (pointer-equal), and each function has exactly one release block.

Other fixtures in the same source:

- `forwardGrouped` (`let value = ((input))?;`, rule 5): the same lists with the place `((input))`. Depth is 5 against 4 for the bare form, and one short of 5 gives `ResourceLimit`.
- `released` (`dispose(lock);` before the `?`, rule 9 "never twice"): `Cleanup [ReleaseGuard(lock)]`, `Initialize dispose(lock)`, then the same exits with no release in either tail (`Cleanup []` on failure, `Cleanup []` after `Cleanup [EndStorage(value)]` on success).
- `movedLocal` (`let held = lock;` first): the targeted query is `Checked` and releases `held` on both exits. Its graph root stays `Incomplete` with no MIR, the D3 local-binding gap that `moved` shows in `cancellation::installed_guard_release_on_budget_delivery`. Not fixed here.

## Caps

Work / entries / depth. Each exact cap holds with the stated outcome. Each independently one-short cap (work, entries, depth) gives `ResourceLimit` with no MIR and no finding. The fixture is one source, so these figures are for this fixture only.

| Function | Exact | Outcome at exact |
| --- | --- | --- |
| `forward` | 3,771 / 640 / 4 | `Checked`, root `Checked` |
| `forwardGrouped` | 3,775 / 640 / 5 | `Checked`, root `Checked` |
| `released` | 3,967 / 744 / 4 | `Checked`, root `Checked` |
| `movedLocal` | 3,914 / 703 / 4 | `Checked`, root `Incomplete` |
| `viewed` | 3,284 / 371 / 4 | `Incomplete` at `&lock`, root `Incomplete` |
| `returned` | 3,210 / 339 / 3 | `Incomplete` at `Result.Ok(lock)`, root `Incomplete` |
| `protectedArg` | 3,236 / 345 / 4 | `Incomplete` at `withGuard(input?, lease)`, root `Incomplete` |
| Whole graph of the fixture | 205,742 / 7,315 / 5 | `Incomplete`; `forward`, `forwardGrouped`, `released` roots `Checked` |

`viewed` (a shared view of the guard across the `?`), `returned` (the guard returned on success) and `protectedArg` (the guard passed to a call after a propagating argument, the `Result` form of the existing pin) are the nearest unsupported siblings. They keep their parent outcome and blocker site.

No fixed cap moved and no existing expectation was edited: the diff adds a test module and registers it. Local runs of `legacy_caps`, `graph::inventory` and the two `m4_8` binaries are listed in the handoff.

## Not established

- A guard in a record field or a wrapper, a guard passed into a call, a shared view of the guard across the `?`, and a guard returned on success remain without a propagation proof.
- `?` on a carrier other than a bare parameter carrier, and delivery on a propagation exit with a guard installed, are not witnessed here.
- This is compiler MIR evidence. No Rill program ran.
