# RFC NNNN: <Feature title>

> **Template only. It is not an assigned RFC or accepted proposal.** Copy to a newly assigned `NNNN-short-feature-name.md` only when creating an actual draft, register that draft in [README](README.md#proposal-register), and replace all placeholders before ReviewReady. Follow the [conventions](README.md), [status rules](README.md#status-and-authority) and single [readiness checklist](README.md#canonical-reviewready-checklist). Authoring prompts below are not proposed feature semantics; replace them with the actual proposal.

Use the [RFC writing style](README.md#writing-style) for all proposal prose.

| Field | Value |
| --- | --- |
| ID | NNNN, unassigned in this template |
| Status | Draft |
| Owner | <Responsible proposal owner; not automatic acceptance authority> |
| Source inputs | <Tickets/contracts/ADRs/research/prototypes with decision links and identifiable revisions> |
| RFC dependencies | <Owning normative RFC sections/revisions; named joint review set if any. List implementation/provider gates separately in the body.> |
| Replaces / superseded by | <Explicit none, or reciprocal RFC/section links and decisions> |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

Give a concise description of what this RFC proposes, its review boundary and its goals/non-goals. Identify whether it covers language behavior, a standard contract, tooling, runtime/platform constraints or a justified combination. This is one cohesive proposal, not a claim to cover every related source ticket.

If this is part of a larger family, identify the related proposals and what this document owns. Scope exclusions are explicit; do not hide required v0 behavior in a future-work sentence.

## Motivation

Describe concrete user/program problems and why the current design or doing nothing is insufficient. Identify the source decisions/use cases this proposal reconciles and any proposed departures. Distinguish measured evidence from illustrative scenarios and prior-art popularity.

## Guide-level explanation

Explain how users would use the feature and label it **proposed**, not already shipped. Introduce concepts/terminology, representative syntax or APIs, normal use and important limitations. Explain how users discover, adopt and reason about it.

Use concrete examples with the context/evidence record required below. Link shared rule owners rather than introducing slightly different ownership, error, concurrency or conversion rules here. Use [the glossary](../../GLOSSARY.md) consistently or explicitly propose a terminology change.

## Syntax and reference semantics

State the precise proposed rules so readers other than the authors can determine implementation work without inventing further in-scope v0 design decisions. As applicable, include:

- Grammar, declarations/import/module identities, public API/family signatures, input/output types and generic/evidence relationships.
- Static validity/selection/inference/ownership/dependency rules, aliases and interaction with existing constructs.
- Observable evaluation/resource/error/state behavior, including boundary cases and operation ordering/races where relevant.
- Unsupported cases, failures, limits, platform requirements, and what is unspecified or implementation-defined. Explicitly bound implementation freedom instead of implying a guarantee.

Identify this section's owned rules and the exact sections owning reused rules. If normative detail must be in an annex, explicitly identify its scope/revision and inclusion in review/acceptance. An ordinary research/prototype link is supporting by default. A literal grammar/reference patch is useful where appropriate; an implementation sketch is not a substitute for semantics.

## Valid/invalid examples and diagnostics

Include representative valid, rejected and boundary/interaction cases inline. Distinguish compile-time rejection, recoverable runtime failure and fatal behavior as applicable. Mark snippets as complete programs, excerpts with named context, pseudocode or incomplete implementation sketches.

For each example or clearly identified group, record:

| Case / artifact and revision | Context | Expected outcome | Owning rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |

No examples are supplied or checked by this empty template. Populate actual cases, not a row claiming a planned check passed. Use the [five evidence labels](README.md#examples-and-evidence), keeping expectations separate from observations.

For rejected cases, state the invalidity rule and expected diagnostic meaning; use exact codes/messages only where the diagnostic contract fixes them. Do not fabricate compiler output. Link revision-identifiable larger fixture inventories and the portions relevant to this proposal rather than copying them all.

## Feature interactions

Explain meaningful interactions with other language/library/tooling contracts, especially cases that change validity, inference, ownership, cleanup, error behavior or concurrency. Link each shared semantic owner; state the proposed resolution of conflicts rather than leaving two incompatible sources.

Name the proposed RFC revisions jointly required for review, if any. Dependencies can be reviewed as a set. An unwritten proposal or unaccepted draft does not supply binding semantics. Include interaction cases above or link their exact inventory rows.

## Compatibility and migration

Discuss effects on existing **designs and source examples** even if there is no released compiler. Cover parsing/name resolution/type checking, source/API/ABI compatibility, runtime/library/client adoption, tooling/documentation and reversibility as applicable.

Identify superseded syntax/contracts and migration steps, unsupported targets/versions and deployment constraints. Distinguish changed design, source breakage, implementation work and released behavior. Say "Not applicable, because..." for concerns that do not apply. Do not assume all compatibility is irrelevant before implementation.

## Security/resource implications

Discuss relevant safety boundaries, trust assumptions, authorization/data exposure, ownership/cleanup, CPU/memory/stack/budget use, cancellation, blocking and external effects. State what is **not** guaranteed and which evidence/implementation gates remain.

Do not infer security, soundness, rollback, prompt cancellation, safe reuse, target support or resource bounds from a signature, model, source inspection or unrelated test. If this concern genuinely does not apply, explain why briefly.

## Rationale, alternatives and prior art

Explain the chosen design and real tradeoffs, including doing nothing and library-level approaches where appropriate. Describe alternatives fairly, including their advantages and why they were not proposed. Distinguish rejected alternatives from unresolved design choices and noncommittal future possibilities.

Cite primary sources/identifiable revisions for prior-art facts and explain their relevance to Rill. Other languages' success or terminology does not prove that a rule fits this language. Refer to source-ticket decisions without treating their closure as this RFC's acceptance.

## Implementation implications

Describe mechanisms, constraints and foreseeable implementation work sufficient to assess feasibility without silently choosing new semantics. Distinguish fixed behavioral requirements from permitted lowering/algorithm choices.

State actual implementation status and link compiler/runtime/library/provider/tooling work and acceptance gates. If no implementation exists, say so. Do not add Implemented to the proposal-status field or imply that a reference harness implements the relevant Rill system. Return an unworkable mechanism that changes promised semantics to design review.

## Evidence and checking plan

Separate **actual evidence** from the **future checking plan**. Use multiple rows when the same artifact has different check layers.

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits, negative/unavailable results and gate links |
| --- | --- | --- | --- | --- | --- |

Allowed labels: NotChecked, HandChecked, ToolChecked, ReferenceExecuted, RillExecuted. See [their definitions](README.md#examples-and-evidence). Empty tables here record no evidence. Source inspection is not execution; standalone/native/model execution is not the relevant Rill compiler/runtime/library. Counts are not proof.

### Planned checks. Not executed evidence

Specify later parsing/static/borrow/diagnostic/runtime/interaction checks and target/provider matrices as relevant, with expected results and prerequisites. Name unimplemented or unavailable mechanisms honestly. Do not put planned passes in the evidence table or hide failed/inconclusive results.

## Open/deferred questions

Classify remaining work explicitly:

- **In-scope semantic blockers:** unanswered v0 behavior, contradictory requirements or missing normative dependencies. These must have concrete proposed answers before ReviewReady; reviewers can still reject those answers.
- **Implementation/validation gates:** mechanisms/tests/support evidence still needed without inventing or weakening the proposed behavior.
- **Future/out-of-scope work:** separately bounded possibilities, not commitments or missing required current semantics.

If there are no unanswered in-scope choices, state that after checking; do not pre-fill a false "None." An open provider/backend gate is not automatically a drafting blocker, but cannot conceal an unanswered semantic question.

## Decision and change history

### Readiness record

No readiness review has been performed for this template. In an actual RFC, identify the checker, date, exact checked revision and the result of **each criterion** in [the canonical checklist](README.md#canonical-reviewready-checklist), with evidence and open-gate links. Do not duplicate a divergent checklist here.

An agent explicitly assigned to produce review-ready proposals can record and apply ReviewReady only under that policy. File creation/merge/structural validation alone is insufficient. Material revision or incompleteness requires revisiting readiness and returning to Draft where appropriate.

### Human decisions and amendments

No RFC decision has occurred for this template. An actual proposal starts **Not yet decided**; source-ticket approvals are provenance, not acceptance of this document. Human acceptance or an explicitly delegated authority must identify the reviewed commit/content snapshot, scope and any exact jointly accepted revisions/annexes.

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |

Preserve earlier decisions. Editorial corrections differ from semantic amendments requiring explicit approval. Substantial redesign normally gets a new RFC; link replacements reciprocally and identify replaced sections. Partial replacement does not set the entire RFC Superseded. Keep accepted design distinct from actual implementation, validation, release or safety certification.
