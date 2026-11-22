# M5 ticket set

Related: [resume log](m5-resume-log.md), [rules and traps](m5-rules-and-traps.md), [owner preferences](owner-preferences.md).

- 2026-11-22: the M5 definition of done was the finite matrix of gap-analysis §1 (owner decision D1 (a)); the owner's D1 to D10 answers are a comment on `ri-2s0`.
- 2026-11-22: 31 open tickets existed as children of `ri-2s0`, `ri-785`, `ri-vgz`, `ri-px1`, all `ready-for-agent`. The source for ids, edges and suggested agents is `/Users/reilley/Repositories/city/notes/m5-ownership-cleanup/tickets/INDEX.md` (read its addendum); ticket bodies are beside it; each body points at its row in gap-analysis §3.
- 2026-11-22 22:26 AEDT: frontier of 16: H1, H2, ENV1, A1 to A6, B1, B2, B4, C2, C4, D1, D2. Critical path: A6, A7, B5, D6, U2.
- 2026-11-22: 13 deferral beads (`needs-triage`, related to `ri-2s0`) held what M5 does not deliver; list in INDEX.md.

## Points the tracker does not carry (2026-11-22)

- Serial merges: tickets editing `ownership/build.rs` or `ownership/flow.rs` conflict textually (gap-analysis §3 "Order and parallelism"); integrate one at a time and run the full M5 binary after each.
- ENV2 (`ri-px1.5`) overlaps H1: only `check:m5:release` is likely left for it.
- D3 (`ri-px1.8`) may re-pin a whole-graph cap under metering rule 11; it still stops on a targeted cap.
- U2 (`ri-2s0.3`) closes the slices and needs the owner's acceptance of the deferred list through the chief.
- Two gap-analysis §1.6 rows had no deferral bead (C3 value `12`, K9 native completion); both are M6 work under `ri-qlh`. D6's acceptance asks for a bead per row, so D6 needs them named.
- The integration branch for M5 is `implementation/m5-ownership-cleanup` (owner's choice, bead `ri-bf3`), not `integration/<spec-slug>`; its worktree is `worktrees/integration-m5-ownership-cleanup`.
- 2026-11-22 (22:50 AEDT): "run the full M5 binary after each" in the serial-merges point above is superseded: push and read CI instead (owner policy).
