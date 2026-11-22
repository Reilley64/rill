# M5 ticket set

Related: [resume log](m5-resume-log.md), [rules and traps](m5-rules-and-traps.md), [owner preferences](owner-preferences.md).

- 2026-11-22: the M5 definition of done was the finite matrix of gap-analysis §1 (owner decision D1 (a)); the owner's D1 to D10 answers are a comment on `ri-2s0`.
- 2026-11-22: 31 open tickets existed as children of `ri-2s0`, `ri-785`, `ri-vgz`, `ri-px1`, all `ready-for-agent`. The source for ids, edges and suggested agents is `/Users/reilley/Repositories/city/notes/m5-ownership-cleanup/tickets/INDEX.md` (read its addendum); ticket bodies are beside it; each body points at its row in gap-analysis §3.
- 2026-11-22 22:26 AEDT: frontier of 16: H1, H2, ENV1, A1 to A6, B1, B2, B4, C2, C4, D1, D2. Critical path: A6, A7, B5, D6, U2. **Superseded 2026-11-23: see the lint entry of 09:20 AEDT below.**
- 2026-11-22: 13 deferral beads (`needs-triage`, related to `ri-2s0`) held what M5 does not deliver; list in INDEX.md.

## Points the tracker does not carry (2026-11-22)

- Serial merges: tickets editing `ownership/build.rs` or `ownership/flow.rs` conflict textually (gap-analysis §3 "Order and parallelism"); integrate one at a time and run the full M5 binary after each.
- ENV2 (`ri-px1.5`) overlaps H1: only `check:m5:release` is likely left for it. **Superseded 2026-11-22 (22:50 and 22:57 AEDT): ENV2 was rewritten with no `check:m5:release`; hosted `Test: Scaffold checks` is the release gate ([resume log](m5-resume-log.md), body of `ri-px1.5`).**
- D3 (`ri-px1.8`) may re-pin a whole-graph cap under metering rule 11; it still stops on a targeted cap.
- U2 (`ri-2s0.3`) closes the slices and needs the owner's acceptance of the deferred list through the chief.
- Two gap-analysis §1.6 rows had no deferral bead (C3 value `12`, K9 native completion); both are M6 work under `ri-qlh`. D6's acceptance asks for a bead per row, so D6 needs them named.
- The integration branch for M5 is `implementation/m5-ownership-cleanup` (owner's choice, bead `ri-bf3`), not `integration/<spec-slug>`; its worktree is `worktrees/integration-m5-ownership-cleanup`.
- 2026-11-22 (22:50 AEDT): "run the full M5 binary after each" in the serial-merges point above is superseded: push and read CI instead (owner policy).

## Later state

- 2026-11-23 (09:20 AEDT), wiki lint (bead `ri-cnh`): the 22:26 frontier of 2026-11-22 no longer described the ticket set. Measured: `origin/implementation/m5-ownership-cleanup` = `5882113b` (merge of ENV1, committed 2026-11-23 00:30 AEDT); PR #73 open, draft, same head; `ri-px1.4` (ENV1) and `ri-px1.12` (ENV3) closed. Reported by `/Users/reilley/Repositories/city/notes/m5-ownership-cleanup/planner-log.md` (last entry 2026-11-23 00:39 AEDT), not re-verified: B4, A1, H2, A2, A6, H1, A4, A3, A5 shipped on ticket branches and queued for merge through the `merger` agent ([environment](environment.md)); B4's merge was bead `ci-i7l`; D3 slung as `ci-r23`. The planner log is the source for the frontier after this date.
- 2026-11-23 (09:20 AEDT), wiki lint: the planner bead `ci-6ok` was `in_progress` under `ci-wisp-3rkra` with its lease expired and its last heartbeat about 8 hours earlier; the planner log had no entry after 00:39 AEDT. Why the build stopped was not established.
