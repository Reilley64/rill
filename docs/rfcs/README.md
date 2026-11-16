# Rill feature RFCs

**Conventions: explicitly human-confirmed in [#26](https://github.com/Reilley64/rill/issues/26), Q1–Q16 and the consolidated recording plan.** This directory is the destination for review-ready v0 language-feature and approved standard-library proposals. [#28](https://github.com/Reilley64/rill/issues/28) writes the substantive set, populates coverage and performs its final consistency review. [Map #1](https://github.com/Reilley64/rill/issues/1) defines the destination; producing or merging that set is not human acceptance, compiler/library delivery or a safety proof.

Start from [TEMPLATE.md](TEMPLATE.md). The [primary-source format research](../research/0026-language-rfc-formats.md) compares Rust, Swift, Python, Go and Java. Rill deliberately selects a Rust-inspired guide/reference structure with explicit compatibility/adoption, security/resource and evidence prompts; it does not import those languages' governance or require Swift's runnable prototype when Rill has no compiler.

**Collection state:** no feature RFCs have been created or assigned IDs by #26. The register below is empty and complete source coverage has **not** been established. Confirmed ticket designs, contracts, ADRs, research and prototypes are inputs, not automatically Accepted RFCs.

## Organization and identity

One RFC covers a cohesive feature or tightly related contract that reviewers can understand and decide together. Do not force one issue/ADR/prototype into one file or place the entire language in one proposal. One input may feed several RFCs, and a proposal may reconcile several inputs. Give every shared rule one owning RFC section; reference it elsewhere rather than maintain competing copies.

Use flat filenames `NNNN-short-feature-name.md`, starting at `0001`, with stable independently allocated decimal IDs. Issue, PR, ADR and prototype numbers are separate namespaces. Check both the register and existing numbered files when choosing the next unused ID; coordinate concurrent creation with the responsible owner. Allocate only when an actual draft enters the collection, and register it at the same time. Do not reserve a whole feature sequence or create empty stubs merely to establish a plan.

Never reuse an assigned ID. Preserve filenames after publication unless a rename is necessary; preserve the ID and repair links when renaming. Keep resolved/replaced proposals and their decision/history links discoverable rather than recycling their identity. A title change is not a new proposal identity. `README.md`, `TEMPLATE.md` and planning entries are not numbered RFCs.

### Planning outline — not an RFC partition

These are navigation/work-planning themes, not six mandatory RFCs, allocations, accepted features or complete coverage:

| Theme | Examples of review boundaries to consider |
| --- | --- |
| Syntax, values, types and modules | Grammar, numeric/string values, nominal records/enums, module identity/privacy/aliases. |
| Functions, overload families and conversions | Invocation, generic evidence, candidate selection, canonical conversion keys and error outputs. |
| Ownership, mutation, borrowing and cleanup | Source dependencies, mutable access, property evidence, disposal and protected cleanup. |
| Errors, diagnostics and testing | Typed errors/causes, diagnostic meaning/output contracts, expectations and test behavior. |
| Concurrency and structured work | Goroutines, transfer/sharing, channels, joins, cancellation and scoped work. |
| Standard-library and platform/tooling contracts | Collections/iteration, I/O/files, environment/randomness, JSON/time/network/TLS/HTTP/SQL, runtime/backend and editor contracts. |

#28 chooses the exact split using reviewability and single-rule-owner criteria. The last theme especially needs several coherent proposals, not one universal library/platform RFC. Cross-cutting rules live in their owning sections with dependencies elsewhere. Large families can justify linked companions; a companion does not silently become normative merely because it is linked.

## Document shape and metadata

Retain these main body headings, in this order; add feature-specific subsections as needed:

1. Summary and scope
2. Motivation
3. Guide-level explanation
4. Syntax and reference semantics
5. Valid/invalid examples and diagnostics
6. Feature interactions
7. Compatibility and migration
8. Security/resource implications
9. Rationale, alternatives and prior art
10. Implementation implications
11. Evidence and checking plan
12. Open/deferred questions
13. Decision and change history

Use a short **“Not applicable, because…”** where a concern genuinely does not apply. Do not leave an empty heading, invent evidence or add repetitive filler. Compatibility can address the current design even without a released compiler. Implementation/evidence can explicitly report absence; lack of implementation does not make those sections inapplicable.

Each RFC begins with a title and the compact Markdown metadata table in TEMPLATE:

| Field | Meaning |
| --- | --- |
| ID | Stable assigned RFC ID, matching filename and register. |
| Status | Exactly one proposal status below; never implementation status. |
| Owner | Responsible proposal owner, not automatic acceptance authority. |
| Source inputs | Identifiable tickets/contracts/ADRs/research/prototypes and their decision/revision provenance. |
| RFC dependencies | Owning normative RFC sections/revisions and any named joint review set. Identify implementation/provider gates separately. |
| Replaces / superseded by | Reciprocal RFC/section links with decision provenance, or an explicit none. |
| Decision | Explicit human decision link, or **Not yet decided**. Merging, assigning an ID or checking a file is not acceptance. |
| Reviewed revision | Exact reviewed commit or explicitly identified content snapshot; distinguish readiness checking from human acceptance. Before either occurs, say Not yet reviewed. |

A content snapshot must unambiguously identify the reviewed text (for example a published snapshot plus content digest), not just a moving branch or mutable latest file. An acceptance record can identify the prior reviewed draft; a subsequent status-only edit need not pretend to reference its own not-yet-created commit. Keep earlier decision/revision records in history when the current table changes. Do not mistake a source-ticket approval for an RFC decision.

## Status and authority

| Status | Meaning |
| --- | --- |
| Draft | Work in progress; missing design, source reconciliation or checks are explicit. |
| ReviewReady | Complete proposed in-scope semantics and the recorded readiness checklist are available for substantive review. Not accepted or proved. |
| Accepted | The project owner, or an explicitly delegated human authority, accepted the identified revision/scope. |
| Rejected | An explicit decision did not adopt this proposal; preserve rationale and revision. |
| Withdrawn | The proposal was withdrawn, not silently accepted or declared technically disproved. |
| Superseded | Its entire normative scope was replaced by identified accepted RFCs/amendments. |

Normal drafting proceeds Draft → ReviewReady → Accepted, but rejection/withdrawal and revision paths are explicit alternatives. Materially changed/incomplete ReviewReady proposals return to Draft until the affected checklist is satisfied and the new revision is recorded. Reopening or revisiting a resolved decision must be explicit, not a silent overwrite of status/history.

An agent explicitly tasked with producing review-ready proposals may prepare and mark ReviewReady **after** performing and recording the checklist, actor and revision. File creation, merge or a passing structural script alone is insufficient. The responsible owner may return it to Draft. Agents cannot infer acceptance; human authority and any delegation of that authority must be explicit.

Acceptance, implementation, validation, release and safety are separate facts. Do not add Implemented as a replacement for proposal status. Record actual implementation state and gate/evidence links in the corresponding body sections and index. An Accepted RFC can still have unimplemented code, unpassed provider/backend checks or no released implementation.

### Normative ownership and dependencies

Accepted RFCs are the authoritative **v0 design baseline**, with one owning section per rule, until a separate language/library reference is explicitly adopted. This is authority over design, not certification that its mechanisms work. Drafts/ReviewReady documents propose rules; they are not yet binding acceptance records. Existing confirmed inputs must be incorporated faithfully or proposed departures clearly identified for human review.

Research, tickets, ADRs and prototype links are **supporting by default**. Do not hide essential semantics behind an evidence link. Put core rules in the RFC, reference their owning RFC sections, or explicitly identify a normative annex with its revision and scope included in review/acceptance. Later supporting-file edits do not silently amend accepted rules. Identify full paths/titles and decision links, not just ADR/prototype numbers; historical number collisions do not establish identity.

Related proposals can be reviewed/accepted as a named set of **exact revisions**. Record the joint review boundary and each rule owner, without duplicating rules or forcing artificial acceptance order. A dependency need not already be Accepted for a coordinated review, but it must provide the complete proposed rules needed to understand the set. A planned/unwritten RFC is not an answer to missing semantics; another Draft is not implicitly Accepted.

Distinguish:

- **Normative dependency:** another proposed/accepted rule the RFC relies on, with owning section/revision and review-set relationship.
- **Supporting input:** provenance, rationale, prior art or evidence, not automatically current normative text.
- **Implementation/provider gate:** work or validation required before implementation/support claims. It can remain open without becoming a missing v0 design choice, provided the proposed guarantee and limits are fully stated.

If feasibility exposes an unworkable mechanism or requires changing promised semantics, return the change to design review/map #1. Do not silently substitute a weaker guarantee, assumed provider, new FFI/target policy or an unspecified future answer.

## Examples and evidence

Provide representative valid, rejected and boundary/interaction cases. For each, state context, expected outcome, owning rule and evidence label. Mark complete programs, excerpts, pseudocode and incomplete implementation sketches distinctly. Large fixture catalogs may be linked rather than copied, but identify revision and the portions used. A path existing is not evidence that its contents were checked.

Keep expected outcomes distinct from observations. A rejected program needs its rejection rule and expected diagnostic meaning; exact diagnostic codes/messages are required only when a named diagnostic contract actually fixes them. A runtime failure is not automatically a compile-time rejection. Do not fabricate output from an absent compiler or count hand-checked expectations as executed tests.

| Evidence label | Meaning / required record |
| --- | --- |
| NotChecked | No check claimed. State the expected outcome and work still needed. |
| HandChecked | Manual check against named rules/revisions; record checker, scope, reasoning/result and limits. Not compiler execution. |
| ToolChecked | Limited structural validation, such as syntax/schema/link checks; record tool, command, revision, observed result and what was not checked. |
| ReferenceExecuted | A standalone experiment/model/native reference ran, not the relevant Rill implementation. Record exact commands/environment/artifacts, positive/negative/unavailable outcomes and limits. |
| RillExecuted | The relevant Rill compiler/runtime/library actually exercised the case. Record exact implementation revision, target, command and result, with the scope of the claim. |

One artifact can have several evidence rows for distinct checks/layers. A Python state model can be ReferenceExecuted while a declaration-only C header is ToolChecked; neither establishes real Rill native ownership. Source inspection is not execution. Aggregate counts or green rows do not erase untested/failing layers or prove soundness/security.

**Planned checks are separate**, not another pass or an observed result. Name later parsing/type/borrow/diagnostic/runtime/interaction tests and target/provider matrices once their implementations exist. Preserve relevant failed/inconclusive/unavailable experiments, not just successes. Review-readiness need not invent executable evidence where only HandChecked or NotChecked material exists; it must provide an honest, relevant evidence and checking plan.

## Canonical ReviewReady checklist

This is the single checklist owner; TEMPLATE links here rather than maintaining a second version. The readiness record identifies the checker, date/revision, each criterion's result and any evidence/gate links. A blank checked box or tool pass does not perform a semantic review.

- [ ] **Identity and responsibility:** assigned ID/filename/register/metadata agree; responsible owner and proposed scope are clear; placeholders have been replaced.
- [ ] **Source reconciliation and coverage:** each relevant in-scope input requirement has a section-level mapping/disposition; confirmed decisions are preserved or departures explicitly proposed. No conflicting historical syntax or same-number artifact is silently rehabilitated.
- [ ] **Understandable proposal:** motivation, guide, examples, boundaries and non-goals form a coherent review unit. Shared rules have one owner; unjustified duplication/splitting is resolved.
- [ ] **Complete proposed semantics:** syntax, APIs, static rules, errors, observable behavior and implementation freedom are specified as applicable. No unanswered in-scope v0 semantic choice or contradictory alternative is left for implementers to invent. Reviewers may reject the proposed answer; that is different from no answer being proposed.
- [ ] **Dependencies/annexes:** required proposed rules exist at identified sections/revisions, joint review scope is explicit, and normative annexes are marked/versioned. Supporting links do not conceal normative requirements.
- [ ] **Cases and diagnostics:** representative valid/rejected/boundary/interaction cases have context, expected outcome, rule and truthful evidence. Diagnostic claims match the actual diagnostic contract, not imagined output.
- [ ] **Compatibility and risk:** source/design/API/ABI/adoption/migration/tooling effects and security/resource implications are considered, with justified non-applicability where appropriate. No unearned rollback, lifetime, bounded-return, support or safety guarantee.
- [ ] **Implementation and evidence honesty:** implementation implications/constraints and actual evidence are stated; executed reference checks are not called Rill tests. Relevant failure/unavailability and future check/target/provider gates remain visible.
- [ ] **Open/deferred work:** future scope and implementation freedom are separated from in-scope semantics. An implementation gate can remain open, but unresolved promised behavior cannot be hidden inside it. Unworkable mechanisms return to design review.
- [ ] **Readability and traceability:** all main headings remain; links/anchors, revisions, terminology and index rows are consistent; rationale, alternatives and prior art are substantive rather than popularity claims.
- [ ] **Decision discipline:** readiness actor/revision/check results are recorded; status does not imply acceptance. Decision/history identifies earlier approvals and the exact revision of any new acceptance or amendment without claiming one has occurred.

The complete #28 set also needs an across-RFC consistency and coverage review. Passing each local checklist does not establish that all v0 features/source tickets were included or all cross-feature interactions agree. Structural checks can assist but cannot replace those reviews.

## Amendments and replacement

Editorial corrections can stay in the same RFC without claiming a semantic change. Keep identifiable history and repair affected links. Semantic amendments require explicit human approval identifying the scope and reviewed revision; preserve previous decisions. Substantial redesign normally uses a new RFC. The human authority decides amendment versus new review unit rather than an automated size threshold.

Replacement links go both ways and name exact affected sections. For partial replacement, keep the original RFC Accepted for its remaining normative scope and mark the replaced sections with their new owner/decision; do not set the whole file Superseded. Entire-scope replacement changes status only when the accepted replacements cover it. Ordinary edits to evidence/annexes cannot bypass the acceptance record.

Accepted text may be maintained through these explicit approvals, with earlier revisions recoverable. It is not an invitation to silently make an old proposal describe different semantics. A future consolidated language/library reference requires its own explicit adoption and authority/supersession mapping.

## Proposal register

**No RFCs assigned or created yet.** This table intentionally has no proposal rows. A formatting example or planning theme is not a proposal, status or acceptance.

| ID | Proposal | Status | Normative dependencies / joint review | Implementation/provider gates | Decision / reviewed revision |
| --- | --- | --- | --- | --- | --- |

#28 registers actual drafts at creation and keeps file/status/dependency/decision links consistent. Link implementation/support work without equating open gates with proposal status.

## Source-to-RFC coverage map

**Not populated; coverage is not complete.** #28 must account for every in-scope feature and relevant input, including closed source decisions and research. Map requirement groups to exact owning sections, not just a ticket to a filename. One row may cite several supporting artifacts; one source may need multiple rows/RFCs.

| Source / identifiable revision | Requirement or scope | Owning RFC section(s) / revision | Disposition and rationale | Supporting decision/evidence links |
| --- | --- | --- | --- | --- |

Use explicit dispositions in prose: incorporated at the linked sections, pending/missing, or deferred/out of scope with the authorized boundary and rationale. A source link alone is not incorporated coverage; a source ticket's Closed state is not RFC acceptance. Distinguish rule provenance from supporting experiments. Pending in-scope semantics prevents the affected ReviewReady proposal/set; unpassed implementation tests remain separately labeled gates, not falsely completed coverage.

## #26 versus #28 completion

#26 records these human-confirmed conventions, the template and this honest index structure, links the research, and checks structural consistency. It creates no feature RFC, does not allocate IDs or assert complete coverage, and does not clear backend/provider/tooling gates. No generic Markdown parser or CI framework is introduced just to validate an empty collection.

#28 owns substantive drafting, exact grouping/ID assignment, coverage population, source reconciliation and the complete-set consistency review. It must return any missing v0 design decision to map #1 rather than publish an unanswered question as ReviewReady. Producing the review-ready set is the map's writing destination; acceptance, implementation and validation remain separate.
