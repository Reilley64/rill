# ri-9o5 research: language proposal formats and processes

Research date: 2026-11-15 (UTC). This report covers Rust RFCs, Swift Evolution, Python PEPs, Go proposals/design documents, and OpenJDK JEPs. It presents research and advisory options, not agreed Rill conventions or RFC acceptance.

The user subsequently confirmed the ri-9o5 conventions now recorded in [docs/rfcs/README.md](../rfcs/README.md) and [TEMPLATE.md](../rfcs/TEMPLATE.md). Statements below about unapproved suggestions describe the research stage, not the current decision state. The findings about other languages' processes remain evidence. The RFC conventions, not this advisory comparison, define Rill's selected process.

## Rill context and limits

ri-9o5 asks for organization, a shared template, linkage, example checking, and review/acceptance conventions for `docs/rfcs/`. [Repository issue-tracker guidance](../agents/issue-tracker.md#wayfinding) separates that work from ri-48g's production of the complete, consistent, review-ready feature RFC set. Ticket answers, research, prototypes, and ADRs are inputs; neither producing the set nor accepting a source-ticket design automatically accepts an RFC.

Rill has no compiler. Existing hand-checked prototypes and standalone experiments can supply evidence, but are not compiler execution or proof of the proposed language. For example, the [ri-dej experiment report](0030-backend-experiments.md) explicitly leaves support gates open and distinguishes finite mechanism/model results from unimplemented compiler/runtime obligations. Provider/backend gates can remain open while documents become review-ready; their precise effects on eventual acceptance or implementation readiness need an explicit Rill decision. ri-9o5 also identifies the confirmed ri-3t8 diagnostic/LSP artifacts as inputs, not delivered tooling or accepted feature RFCs.

The parent agent suggested grouping related features, mapping source tickets to RFCs many-to-many, using flat independent stable IDs, and adopting `Draft`/`ReviewReady`/`Accepted`. These are unapproved proposals, not findings or constraints of this research. Nothing below selects them.

## Sources and provenance

All language sources below are first-party. Process/template pages were read alongside a representative accepted proposal, including ones now implemented or delivered. Retrieval succeeded except for the obsolete Swift template path `main/0000-template.md` (404); the current process links to `proposal-templates/0000-swift-template.md`, which succeeded. No access failure is treated as evidence of a rule.

For GitHub files, the table records the last commit touching that path, as returned by the official repository's commits API on the research date, not the repository HEAD, original acceptance date, or a claim that every linked page is immutable. Short revisions link to the observed full commit. Main-branch document links identify the current source; follow the revision link for historical context. The separately included Python template was retrieved but its individual revision was not queried.

| Key | Official source | Observed last path-change revision / UTC date |
| --- | --- | --- |
| R-P | [Rust process](https://github.com/rust-lang/rfcs/blob/master/README.md) | [895324ee01f0](https://github.com/rust-lang/rfcs/commit/895324ee01f0c579ae1de46ddddd55967aff6c5e), 2025-12-11 |
| R-T | [Rust template](https://github.com/rust-lang/rfcs/blob/master/0000-template.md) | [f17e8623ee2e](https://github.com/rust-lang/rfcs/commit/f17e8623ee2e2854570dcdb936a9f4ab08c0fcd4), 2025-11-29 |
| R-E | [RFC 2005: match ergonomics](https://github.com/rust-lang/rfcs/blob/master/text/2005-match-ergonomics.md) | [f17e8623ee2e](https://github.com/rust-lang/rfcs/commit/f17e8623ee2e2854570dcdb936a9f4ab08c0fcd4), 2025-11-29; document Start Date 2016-08-12 |
| S-P | [Swift process](https://github.com/swiftlang/swift-evolution/blob/main/process.md) | [9e93dbd9b6cb](https://github.com/swiftlang/swift-evolution/commit/9e93dbd9b6cb6e9ffb3bf40babfe00bbf23abb59), 2025-12-02 |
| S-T | [Swift language/standard-library template](https://github.com/swiftlang/swift-evolution/blob/main/proposal-templates/0000-swift-template.md) | [3d0f73e320d9](https://github.com/swiftlang/swift-evolution/commit/3d0f73e320d9b5b3598e6a4e32c7581484efc9af), 2026-08-18 |
| S-E | [SE-0296: async/await](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0296-async-await.md) | [e2efe275a18c](https://github.com/swiftlang/swift-evolution/commit/e2efe275a18cd3c77db09b968a8d6f1b475e4d36), 2026-02-17; status Implemented (Swift 5.5) |
| P-P | [PEP 1: process](https://peps.python.org/pep-0001/) | [dfc3d959d054](https://github.com/python/peps/commit/dfc3d959d05476349091722f92bcff9dc0d50528), 2026-11-08 |
| P-T | [PEP 12: formatting and template instructions](https://peps.python.org/pep-0012/), [actual included template](https://github.com/python/peps/blob/main/peps/pep-0012/pep-NNNN.rst) | PEP 12: [76b2b6a2aba2](https://github.com/python/peps/commit/76b2b6a2aba25f827c0f76a7492b1cf666b46243), 2026-03-04 |
| P-E | [PEP 634: pattern matching specification](https://peps.python.org/pep-0634/) | [963f564c718d](https://github.com/python/peps/commit/963f564c718d41a638ac089c8e8535eb183e5780), 2023-12-11; Created 12-Sep-2020, status Final, Python 3.10 |
| G-P | [Go process](https://github.com/golang/proposal/blob/master/README.md) | [58e952a6dd29](https://github.com/golang/proposal/commit/58e952a6dd29795163bdff3de8b2d77329ff995b), 2026-04-23 |
| G-T | [Go design-document template](https://github.com/golang/proposal/blob/master/design/TEMPLATE.md) | [8c4239d15609](https://github.com/golang/proposal/commit/8c4239d15609d224550b260b5cd46673a31915c7), 2023-05-10 |
| G-L | [Go language-change questionnaire](https://github.com/golang/proposal/blob/master/go2-language-changes.md) | [d4addabae1a3](https://github.com/golang/proposal/commit/d4addabae1a3bf748adaeafbe318b3c92a0c56da), 2021-09-20; internal "Last updated: January, 2020" |
| G-E | [43651: type parameters](https://github.com/golang/proposal/blob/master/design/43651-type-parameters.md) | [8b5daebee319](https://github.com/golang/proposal/commit/8b5daebee3198c7b6265b40c06b077be5e4c5b4b), 2022-01-21; document date August 20, 2021 |
| J-P | [JEP 1](https://openjdk.org/jeps/1), and its explicitly linked [JEP 2.0 process, draft 2](https://cr.openjdk.org/~mr/jep/jep-2.0-02.html) | Displayed Updated: 2018/03/30 15:46 and 2014/5/5 respectively |
| J-T | [JEP 2: body template v2.0](https://openjdk.org/jeps/2) | Displayed Updated: 2026/06/18 01:17 |
| J-E | [JEP 444: virtual threads](https://openjdk.org/jeps/444) | Displayed Updated: 2025/10/30 21:01; Created 2023/03/06 18:00; Closed / Delivered, release 21 |

JEP 1 says much of it is superseded by the linked JEP 2.0 document, although that document still labels itself a draft. The findings below follow that explicit pointer rather than repeat JEP 1's obsolete email/Mercurial submission mechanics. This documentation mismatch is real; it is not evidence that all old states or headers still apply. JEP 2 and the delivered JEP 444 corroborate the JBS/body-template model. [J-P, J-T, J-E]

## Rust RFCs

### Actual template and section strength

Exact top-level headings in the current template, in order:

`Summary`; `Motivation`; `Guide-level explanation`; `Reference-level explanation`; `Drawbacks`; `Rationale and alternatives`; `Prior art`; `Unresolved questions`; `Future possibilities`.

The process says to copy and fill in the template; it does not provide a separate REQUIRED/OPTIONAL schema for each heading. These are expected authoring sections, not evidence of a hard validator. Instructions expressly permit saying there is no prior art or no future possibility; examples, sample diagnostics, deprecation warnings, and migration guidance are conditional where applicable. [R-T, R-P]

- The guide-level section teaches the feature as if included. It introduces concepts, concrete examples, the user's mental model, and implications for readability, maintenance and adoption.
- The reference-level section specifies interactions and enough implementation detail to assess feasibility. It explains corner cases through examples and revisits the guide's examples.
- The motivation section asks for specific user problems and use cases. Drawbacks asks why not to make the change; alternatives includes doing nothing and library/macro approaches. Prior art asks about experience, not mere existence elsewhere. Unresolved questions distinguish pre-merge questions, implementation-before-stabilization questions, and independent future scope. There is no dedicated template heading for security, testing, benchmarks, or tooling; diagnostics/migration can appear in guide-level material. [R-T]

### Identity, granularity, amendments, governance

Metadata: `Feature Name`, `Start Date` (`YYYY-MM-DD`), `RFC PR`, `Rust Issue`. Start with `text/0000-my-feature.md`; the number is the RFC repository PR number, not an independently allocated feature or source-ticket number. The process first says renaming occurs on acceptance, then more specifically instructs authors to rename/update the link once the PR is open. Thus the concrete instructions permit an assigned PR-based number before acceptance. Accepted RFCs link an implementation tracking issue; the repository points to an RFC book and active-RFC list. [R-P, R-T]

The relevant sub-team makes the decision, not a community vote. A motion proposes merge/close/postpone; all sub-team members sign off before a final comment period of ten calendar days, covering at least five business days. Merging makes an RFC active: major stakeholders agree in principle, but implementation priority, staffing, eventual code merge, and stabilization are not guaranteed. Minor amendments may use follow-up PRs; substantial changes should be new RFCs with a note in the original, with the sub-team judging the boundary. [R-P]

### Accepted example versus today's template

RFC 2005 is a merged RFC with PR and Rust implementation-issue links. Its actual headings are `Summary`, `Motivation`, `Detailed design` (definitions, binding-mode rules, examples, backwards compatibility), `How We Teach This`, `Drawbacks`, `Future Extensions`, `Alternatives`. It therefore does not match today's guide/reference/prior-art/unresolved layout; it is a historical accepted design, not a reason to claim those current headings were always mandatory. It supplies before/after patterns, annotated error examples, inference/backward-compatibility reasoning, teaching/diagnostic implications, and a possible future extension for custom dereference types. Detailed semantics need not take the form of a literal patch to a language-reference document. [R-E]

## Swift Evolution

### Actual template and section strength

Current language/standard-library template headings, exactly:

`Summary of changes`; `Motivation`; `Proposed solution`; `Detailed design`; `Source compatibility`; `ABI compatibility`; `Implications on adoption`; `Future directions`; `Alternatives considered`; `Acknowledgments`.

The template tells authors to retain the section headers. The general process says the document body has no specific scraping-format requirements. This does not make the area-specific template irrelevant. In particular, adoption implications should remain even when there are none; an explicit no-impact statement is suggested. The current summary limit is 50 words, suitable for proposal lists. Other evolution areas have their own templates. [S-T, S-P]

`Detailed design` asks for grammar additions/changes for new syntax, or full APIs and documentation comments, detailed enough for a non-author to implement. `Source compatibility` separately considers parsing, names, conversions, overloads, type-checking cost, and existing clients. `ABI compatibility` covers separately compiled code/runtime/library interaction. `Implications on adoption` covers deployment/runtime requirements, library clients, reversibility of adoption, and minimum tools versions. Future directions should be clearly noncommittal and separate from current scope. Alternatives should fairly describe advantages as well as reasons for rejection. No separate security or testing heading is present. [S-T]

### Identity, linkage, lifecycle

Machine-readable leading bullets use a proposal link such as `[SE-NNNN](NNNN-filename.md)`, authors, review manager, status, and review-history links. The process makes proposal/authors/status required, review manager required for full review, and review history required for new proposals. Optional fields include vision, roadmap, bug, implementation, previous proposal, and previous revisions; template-specific conditional fields include experimental/upcoming feature flags. A roadmap can explain a large feature broken into focused related proposals; previous-proposal links distinguish extraction/supersession from revision, and previous-revision links preserve reviewed versions. Filenames should generally remain stable after addition. [S-P, S-T]

The number is sequential within the prefix, not the evolution PR number. The inspected current process/template do not clearly specify the exact allocator or allocation event; do not infer Rust's numbering timing. The process links a public [status index](https://www.swift.org/swift-evolution). [S-P]

The process starts with a pitch, followed by a formal document/PR. A workgroup appoints a review manager, opens review, and makes the decision through the appropriate evolution workgroup. Review lasts at least ten days including two consecutive weekends; substantial new revisions may need another review. The current language/standard-library process requires a usable prototype implementation before review, not necessarily production-ready code. Implementation review remains distinct from design review. States include Awaiting implementation, Awaiting review, Active review, Returned for revision, Withdrawn, Rejected, Accepted, Accepted with revisions, Previewing, and Implemented (Swift X.Y). The template's early implementation gate does not make acceptance equal to release. [S-P, S-T]

### Accepted example versus today's template

SE-0296 is Implemented (Swift 5.5), linking both acceptance rationale and an accepted amendment allowing overloads differing in `async`. It begins with `Introduction`, not today's `Summary of changes`; its compatibility headings are `Source compatibility`, `Effect on ABI stability`, and `Effect on API resilience`. It adds a table of contents, revision history, related proposals, and acknowledgments. Thus old accepted documents are not exact instances of the current template. [S-E]

The document combines callback-problem/user examples with precise async function/type/expression rules, valid and invalid annotated examples, autoclosure and protocol-conformance interactions, and compatibility/adoption discussion. Its related-proposals section separates Objective-C interoperability, structured concurrency, and actors rather than claiming async/await specifies all concurrency. Revision history and decision links preserve amendment provenance. [S-E]

## Python PEPs

### Actual template and section strength

PEP 12 is both formatting guidance and a wrapper around a separate included template file, not just a list of conventions. That actual template has these headings:

`Abstract`; `Motivation`; `Specification`; `Rationale`; `Backwards Compatibility`; `Security Implications`; `How to Teach This`; `Reference Implementation`; `Rejected Ideas`; `Open Issues`; `Acknowledgements`; `Footnotes`; `Change History`; `Copyright`.

PEP 1 says each successful PEP "should" have its enumerated parts, but qualifies several: backwards-incompatible proposals must describe severity and mitigation; security concerns should be explicit if present; teaching is helpful; change history is explicitly not mandatory. New PEPs must use the stated public-domain/CC0 dual licensing. Do not turn the whole suggested section list into equally mandatory acceptance gates. [P-P, P-T]

The specification should describe syntax/semantics sufficiently for competing interoperable implementations. Rationale includes alternatives, related work, objections, and evidence of consensus. Teaching can include migration and documentation changes. The final reference implementation must include tests and appropriate language/library-reference documentation; implementation need not be complete for acceptance. Security receives its own explicit prompt, unlike the Rust/Swift templates examined. [P-P, P-T]

### Identity, granularity, lifecycle, authority

PEPs are UTF-8 reStructuredText files, `pep-NNNN.rst`, with RFC-2822-style metadata, not Markdown RFCs. Required headers in PEP 1 are `PEP`, `Title`, `Author`, `Discussions-To`, `Status`, `Type`, `Created`, `Post-History`; optional/conditional headers include `Sponsor`, `PEP-Delegate`, `Topic`, `Requires`, `Python-Version`, `Replaces`, `Superseded-By`, `Resolution`. Resolution is required for resolved Standards Track PEPs. Title limit: 44 characters. `Created` records number assignment, not necessarily first private drafting. [P-P]

Current submission instructions let an author select the next available unused/unclaimed number for the draft; editors validate/assign the official number when approving publication, and repository writers can claim numbers directly while doing the editor's duties. Publication/number assignment is explicitly not substantive acceptance. This is more nuanced than "editors number it after acceptance." [P-P, P-T]

PEP 1 strongly recommends a single key proposal, while permitting linked documents. `Requires` records dependencies; replacement should have reciprocal `Replaces`/`Superseded-By` linkage. The [PEP index](https://peps.python.org/) groups documents by status/type/topic. After resolution PEPs are normally historical records, not living language specifications; formal behavior belongs in maintained documentation. Accepted/provisional designs may record authorized implementation-driven changes, and active process/informational PEPs can evolve. [P-P]

The Steering Council has final authority, potentially exercised through a PEP delegate; editors handle administration/publication, not acceptance. Draft, Deferred, Accepted, Provisional, Final, Rejected, Withdrawn, Superseded, and Active have distinct meanings. For a normal Standards Track PEP, Accepted precedes completion/integration of the reference implementation; Final follows that completion/integration, not necessarily the release date. Provisional allows additional feedback and exceptional reversal even after release; regular accepted-but-unreleased designs can also be reversed for fundamental flaws. [P-P]

### Accepted example versus the suggested template

PEP 634 is Final, with Python-Version 3.10, a resolution link, and `Replaces: 622`. Its actual top-level headings are `Abstract`, `Syntax and Semantics`, `Side Effects and Undefined Behavior`, `The Standard Library`, `Appendix A -- Full Grammar`, and `Copyright`. It intentionally delegates motivation/rationale to [PEP 635](https://peps.python.org/pep-0635/) and tutorial material to [PEP 636](https://peps.python.org/pep-0636/): three linked documents replace the earlier combined PEP 622. This is concrete evidence that one conceptual feature can have a linked specification/rationale/tutorial set, not a universal requirement to split proposals that way. [P-E]

Its reference detail includes soft keywords, full grammar, binding constraints, cases producing SyntaxError/TypeError/ValueError, and deliberately unspecified side effects or partial bindings. A "specification" can explicitly bound implementation freedom; it need not pretend every observable behavior is fixed. The current page links onward to canonical language-reference documentation, consistent with PEP 1's historical-record policy. [P-E, P-P]

## Go proposals and design documents

### Two different official formats

A proposal is a labeled GitHub issue. A design document is its expanded form only when needed; an issue can be accepted or declined without one. This is not Rust's mandatory merged-document path for substantial changes. [G-P]

The official design-document template's exact headings are `Abstract`, `Background`, `Proposal`, `Rationale`, `Compatibility`, `Implementation`, `Open issues (if applicable)`. Its title is `Proposal: [Title]`; metadata is `Author(s)`, `Last updated`, and a discussion URL. `Open issues` is explicitly omittable when empty; the process says the document should follow the template, without a separate mandatory-section schema. Implementation asks for steps, responsible people, timing, and fit with the release cycle. [G-T, G-P]

The separately named Go 2 language change template is an official questionnaire, not that document skeleton. Authors need not answer every question, but it asks for precise and informal descriptions, before/after examples, language-spec changes, backward compatibility, learning cost, affected tools (vet/gopls/gofmt/goimports), compile/runtime cost, possible implementation, feature orthogonality, prior proposals, and measured evidence for performance goals. It explicitly says a prototype is not required. It has no dedicated security heading and does not universally demand an invalid-example suite. [G-L]

### Identity, review, implementation

A design document uses `design/NNNN-shortname.md`, where NNNN is the Go proposal issue number, already available before writing the document. It is reviewed through Gerrit; design discussion belongs on the issue, while Gerrit comments should focus on preparation/editorial/procedural matters. Multiple checked-in revisions are expected. The issue/project records the proposal index and state, rather than a document header reproducing all governance. [G-P]

The proposal review group seeks consensus and guides issues through Incoming, Active, Likely Accept/Likely Decline, Accepted/Declined, with Hold and specific decline reasons available. Normally a week in a likely state precedes the decision; exceptions/fast-tracking are allowed. If the group cannot find consensus or a next step, the process escalates to Go architects, then an arbiter if needed. Acceptance adds Proposal-Accepted and repurposes the issue to implementation tracking in a work milestone, often Backlog: it does not promise a release or completed implementation. [G-P]

The inspected process does not give a Rust-like minor-amendment/new-RFC threshold or a PEP-like reciprocal supersession header scheme. It does discuss duplicates, obsolete/retracted proposals and reconsideration, and keeps design revisions. Lack of a universal rule here is not permission to erase decisions; local Rill change-history rules still need choosing. [G-P]

### Accepted example versus the template

The type-parameters document explicitly says its design was proposed and accepted through [issue 43651](https://go.dev/issue/43651); its historical status text still expects Go 1.18 in early 2022. That is a useful warning not to treat proposal prose as current release truth. [G-E]

Its top-level organization is `Status`, `Abstract`, `How to read this proposal`, `Very high level overview`, `Background`, `Design`, `Examples`, `Acknowledgements`, `Appendix`, rather than a literal instance of G-T. `Design` contains constraints, type sets, inference, reflection, implementation choices, omissions/issues, discarded ideas, and comparisons with Java/C++/Rust. It includes complete user examples and explicitly invalid examples, such as an impossible generic copy between unconstrained unrelated element types. Compatibility is discussed without a separate top-level Compatibility heading. The implementation section permits different compilation strategies rather than requiring one exact lowering. [G-E]

## OpenJDK JEPs

### Actual template, not obsolete header practice

JEP 2 defines version 2.0 of the Markdown body in JBS. Its headings, in order, are:

`Summary`; `Goals`; `Non-Goals`; `Success Metrics`; `Motivation`; `Description`; `Alternatives`; `Testing`; `Risks and Assumptions`; `Dependencies`.

Only `Summary` and `Description` are marked REQUIRED. All other sections are optional. Omit unused ones rather than write "None"; retain order and do not add sections beyond the template. Drafts need not initially fill everything. This is materially stricter about shape, but lighter about required sections, than simply importing a Rust template. [J-T]

Description covers what changes and intended implementation, modified Java APIs/command-line switches/library-JVM interfaces/file formats, failure diagnosis in development and production, and open design issues. Testing asks for work beyond mandatory unit tests and special hardware/platform needs. Success Metrics applies when measurable numerical goals exist. Dependencies should appear both in text and JBS issue links. The body evolves to describe the delivered result, potentially linking additional detailed documents. [J-T]

JEP metadata is held in JBS fields: summary/title, owner/assignee, reporter, dates, status, JEP type, author, discussion, scope, component, review/endorsement and release/planning information where applicable. The linked JEP 2.0 document marks required fields and introduces the JBS model; JEP 1's old email header/file submission recipe is not a current template to copy. JEP 444's rendered metadata includes a distinct JBS issue ID, JEP number, author/owner, type/scope, release, discussion and related JEP links. [J-P, J-E]

### Roadmap membership is not release approval

Under the linked JEP 2.0 process, a proposal moves from Draft to Submitted to Candidate. The OpenJDK Lead adds it to the roadmap. A separate JEP number, distinct from the JBS issue key, is assigned on first entering Candidate. The published location is `/jeps/<number>`; an author-chosen repository filename is not the submission identity. Candidate means worthy of consideration, not committed delivery. Targeting requires a credible engineering plan and rough consensus of project committers/reviewers as determined by the Project Lead. [J-P]

The later stages distinguish Proposed to Target, Targeted, Integrated (code/unit tests), Complete (functional/performance/conformance tests; mainly fixes remain), and Closed/Delivered (release ships). Drop, withdrawal and rejection paths also exist. Process/informational JEPs can be Active; the linked process describes minor amendments versus separate major-revision JEPs that may supersede/update the original. JEP 2 expects feature descriptions to evolve with implementation; that is not Python's normally historical resolved-PEP model. [J-P, J-T]

JEP acceptance/roadmapping does not supplant the Java Community Process for standard Java SE interfaces. A JEP is a high-level enhancement/engineering record, not automatically a complete normative Java language/VM/API specification change. [J-P]

### Delivered example versus the template

JEP 444 is Closed/Delivered in JDK 21. Its `History` records previews in JEP 425/JDK 19 and JEP 436/JDK 20, then finalization with feedback-driven changes. Its body includes Summary, History, Goals, Non-Goals, Motivation, Description, Alternatives, Testing, Risks and Assumptions. `History` is an observed extra heading despite the current JEP 2 instruction not to add sections; do not infer current permission from an older delivered document. [J-E, J-T]

The example ties user migration and thread-per-request code to concrete API/runtime changes; it covers JNI and debugging interfaces, thread dumps/JFR, security-manager permissions, and interactions with existing thread APIs. Testing names existing regression coverage, running tests in virtual threads, new API/stress tests, and larger server tests. Dependencies/security/tooling can be substantive inside Description even without dedicated top-level headings. Preview/finalization lineage is linked across several JEPs; that is evidence of one feature's staged evolution, not a rule that every language feature needs separate preview RFCs. [J-E]

## Synthesis: common core versus process-specific choices

### Common content, not one universal mandatory checklist

Across these official formats, the reusable center is problem/motivation; understandable proposed behavior; precise design; rationale/alternatives; impact/compatibility; and links to discussion or implementation. Exact partitioning and obligation differ: JEP's Motivation and Alternatives are optional; Go may decide an issue without a design document; PEP 634 moves motivation/tutorial to companion PEPs. No evidence here supports a universal mandatory heading set. [R-T, S-T, P-P, P-E, G-P, G-T, J-T]

| Concern | What the examined primary sources actually support |
| --- | --- |
| User-facing explanation/examples | Rust's guide level, Swift's proposed solution and Go's language questionnaire ask for these explicitly. PEP teaching guidance and the PEP 636 link also cover them. JEP 444 supplies examples, but JEP 2 does not mandate a separate user-example section. |
| Invalid examples and corner cases | Rust reference guidance asks for corner cases; RFC 2005, SE-0296 and Go 43651 contain invalid examples; PEP 634 specifies invalidity/error cases. A mandatory separately labeled valid/invalid example inventory would be a Rill addition, not a common official heading. |
| Normative/spec-level detail | Rust asks for reference-level detail; Swift asks for grammar/API detail; PEPs ask for interoperable syntax/semantics; Go asks explicit specification-change questions. JEP descriptions are high-level and may link external specification work. Detailed rules and fixed implementation strategy are different requirements. |
| Interactions/compatibility/migration | Rust covers these in its guide/reference sections and alternatives. Swift separates source, ABI and adoption effects. PEPs have a conditional backwards-compatibility requirement and teaching guidance. Go asks about its compatibility promise, orthogonality and tool costs. JEP Description and JEP 444 cover adoption/tooling. |
| Alternatives/prior art | Rust separates prior art and rationale. Swift has alternatives; PEPs have rationale/rejected ideas; Go has rationale and a questionnaire; JEPs have optional alternatives. Prior art is not itself proof that a change fits the host language. |
| Unresolved scope | Rust explicitly separates pre-acceptance, pre-stabilization and future questions. Swift has future directions, PEPs have Open Issues, and Go has optional Open issues. JEPs cover open design issues in Description and have Risks and Assumptions. |
| Evidence/implementation/tests | Rust links implementation tracking. Swift currently requires a usable prototype for language review. PEPs recommend a co-developed prototype and require implementation/tests/docs for Final. Go makes prototypes optional and asks for implementation planning. JEPs have optional Success Metrics/Testing and engineering-stage gates. These readiness requirements are not interchangeable. |
| Security/tooling | PEP has Security Implications; Go language questions name tool costs; Rust suggests diagnostic/migration examples; Swift compatibility covers toolchain/adoption effects; JEP Description asks about interfaces and failure diagnosis, with extensive tools/security detail in JEP 444. This does not establish that every template mandates dedicated security/tooling headings. |

Sources for the matrix: [R-T, R-E, S-T, S-E, P-P, P-T, P-E, G-T, G-L, G-E, J-T, J-E] in the source table above.

### Choices that cannot be imported as a package

- IDs: Rust uses proposal PR number; Go uses proposal issue number; PEP uses an independently administered number; Swift uses a prefix sequence with allocation timing not established here; JEP gets a number on candidacy distinct from its issue ID. Stable linkage is useful evidence; none establishes Rill's preferred namespace or allocation time. [R-P, G-P, P-P, S-P, J-P]
- Granularity: single-key-proposal guidance (PEP), focused but usable proposals/roadmaps (Swift), companion spec/rationale/tutorial documents (PEP 634-636), and preview/finalization lineage (JEP 444) coexist. There is no sourced universal "one ticket = one RFC" or "one feature = exactly one file" rule. [P-P, P-E, S-T, J-E]
- Authority: Rust sub-team/FCP, Swift evolution workgroup/review manager, Python council/delegate, Go consensus review/escalation, and OpenJDK roadmap/targeting/JCP roles solve different governance problems. Document headings do not choose an authority or voting rule for Rill. [R-P, S-P, P-P, G-P, J-P]
- Acceptance versus execution: all distinguish some form of design/roadmap decision from later delivery, but Swift's pre-review prototype gate is a significant counterexample to "no execution needed for review." Rill cannot honestly claim that gate today. Neither review-ready nor accepted should silently imply compiler correctness, provider readiness, tests passing, or release. [R-P, S-P, P-P, G-P, J-P; Rill context above]
- Record versus living specification: Python normally freezes a resolved proposal as history; Rust expects substantial changes in new RFCs; Swift records reviewed revisions/amendments; JEP bodies evolve toward the delivered result. Rill needs to decide where current normative truth lives and how amendments are approved rather than accidentally mix these models. [P-P, R-P, S-T, J-T]

## Plausible Rill options. Advisory, not approved

These are alternatives for ri-9o5 to evaluate, not RFCs or a decision interview. All would need explicit ownership of review/acceptance, a coverage index for ri-48g, typed links to supporting inputs, and honest evidence/readiness labels because ri-9o5 asks for those outcomes. An index can map source tickets and features to RFC sections without fixing the ID scheme or number of links in advance.

### A. Self-contained feature RFC, guide plus reference

Possible headings: Summary; Motivation and scope; Guide-level explanation; Syntax and reference semantics; Valid/invalid examples and diagnostics; Feature interactions; Compatibility and migration; Rationale/alternatives/prior art; Implementation implications; Evidence and checking plan; Unresolved/deferred questions; Decision/change history.

One possible convention would put one reviewable behavior unit in each document and link related RFCs. Documents would use `NNNN-slug.md` in a flat collection, with independently allocated IDs at publication and an index separate from ticket identity. That would be a conscious Rill choice resembling parts of Rust/Swift/PEP practice, not an approval of the parent's Q1/Q2 proposals. A decision-status field could be paired with a separate implementation/evidence field; exact vocabulary remains open.

This option produces a single readable review document with explicit corner cases and evidence boundaries. It risks repeating shared rules and creating very long cross-cutting RFCs. It needs a rule identifying the owning section of shared semantics. Reviewable granularity must be assessed per design, not assumed from ticket boundaries.

### B. Linked specification plus explanatory companions

Possible primary headings: Summary; Scope/dependencies; Grammar and semantics; Static errors/dynamic failures; Interactions/compatibility; Conformance examples; Open normative questions; Decision/change history. A companion rationale/guide holds Motivation; User examples; Alternatives/prior art; Evidence; Implementation implications/deferred work. This adapts the observed PEP 634-636 split, not a PEP-wide mandate.

Another possible convention would group individually identified documents in a family directory or index, with one declared normative owner for each rule. Acceptance would name the exact companion revisions it covers. Ticket-to-document links remain explicit, whether one-to-one or many-to-many in a particular family.

This option separates reference text and shared rules for related features. It requires more navigation and revision work, and accepted normative text may drift from its rationale/examples. It is probably excessive for small independent features but may help with large intertwined systems.

### C. Issue-anchored concise proposal with detailed annexes

Possible headings: Summary; Problem/goals/non-goals; Proposed behavior; Compatibility/interactions; Alternatives; Evidence and validation; Implementation/provider gates; Open issues; Decision record. Move long grammar, example catalogs and experiments into explicitly linked annexes. A `source-ticket-slug.md` identity could follow a Go-like issue anchor, with suffixes or independent child identifiers if one ticket yields multiple review units; amendments and merged-ticket cases need an explicit rule.

This option fits the existing ticket-heavy workflow and avoids duplicating research. It risks making the RFC depend on other documents and confusing ticket closure with RFC acceptance. Unlike Go's issue-only path, Rill's agreed destination still requires review-ready documents in `docs/rfcs/`. Annexes must distinguish normative material reviewed with the proposal from merely supporting evidence.

### Advisory preference and evidence boundary

The advisory preference is to evaluate A first because it is the easiest self-contained document to review. Permit B only for justified large families, and use C's explicit issue/evidence links without making issue closure an acceptance mechanism. This is a researcher's suggestion, not user approval of grouping, IDs, headings or statuses.

Regardless of option, a Rill evidence section could distinguish hand-checked against named rules, standalone experiment executed (exact environment/result/limitations), not checked, and future compiler test. Preserve both positive and negative examples with expected outcomes and the rule they exercise; label pseudocode/incomplete contexts. Future checks can cover parsing/type checking, expected diagnostics, runtime semantics, feature-interaction regressions, and provider/backend matrices once the relevant implementation exists. These are proposed documentation obligations, not implemented validators or proof claims. The existing ri-dej report is a concrete model for limiting experimental claims; current Swift's runnable-prototype gate should not be copied under a different name.

Accepted source-ticket designs should be linked with their actual decision provenance; RFC decision fields should remain independently unset until the authorized human/process accepts them. Review blockers, acceptance blockers and implementation/provider blockers can be recorded separately without asserting they all block the same stage. Whether a particular unresolved semantic question prevents review-readiness or acceptance is a policy choice, not answered by the foreign processes.

## What factual research cannot settle

1. What constitutes one Rill review/decision unit, when a family warrants splitting, and whether IDs should track tickets, publication order, or a separate registry; exact numbering/rename timing also remains a Rill choice.
2. Who declares readiness and who accepts/amends/supersedes an RFC, what decision evidence must be recorded, and whether status is one field or separate design/implementation dimensions. No proposed status vocabulary has been approved here.
3. Which open questions must be closed before review versus acceptance, especially provider/backend feasibility and unsupported mechanisms, and what hand-checked evidence is sufficient while no compiler exists.
4. Whether accepted RFCs remain historical design records or maintained normative text, and where cross-cutting definitions, glossary terms, diagnostic contracts and future language-reference text have their authoritative home.

Research establishes concrete precedents and their limits. It does not accept Rill designs, create the ri-9o5 conventions, complete ri-48g coverage, or close any implementation gate.
