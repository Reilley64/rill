# Comments in Rust code

Policy for `//` and `/* */` comments in Rust sources under `crates/`, `runtime/` and `tools/`. A comment cannot be checked; a test, a type or a diagnostic can. Agents copy what they read, so a comment that excuses a workaround breeds more workarounds.

## Policy

Write no new comment unless it is one of these four forms. The list is closed: a comment that fits no form goes, however true or useful it reads.

1. A license header.
2. A doc comment (`///`, `//!`) on a public API contract: what a caller may rely on. A doc comment on a private item, or one that narrates the implementation, is not a contract.
3. `// SAFETY:` on an `unsafe` block or `unsafe impl`, stating why the invariants hold.
4. A comment that is only a link to a bead, RFC or ADR that explains what the code cannot express. The whole comment is the link: a bead id, a `docs/rfcs/...` path or a `docs/adr/...` path.

Nothing else is kept. In particular these are not forms 1 to 4 and go:

- A note on behavior forced by a dependency, platform or protocol.
- A lint suppression's justification.
- A `TODO`, `FIXME` or `NOTE`, a banner, narration, and commented-out code.
- A link with prose around it.

Existing comments are not swept. The policy covers comments added or changed in a diff against the merge base.

## Instead of a comment

- A workaround: fix the root cause, then delete the workaround.
- A rule or invariant: write a test or a type that enforces it.
- A decision: record it in a bead, RFC or ADR, and link it with form 4.

Code that needs a comment to excuse it is a defect, not a documented limitation. Fix the code, or open a bead for the defect and link the bead.

## When it applies

The policy applies to changes begun after M5 (bead `ri-2s0`) closes. A change begun earlier, including the M5 branch and its reviews, is not swept for comments.
