# GPT-6.1 Sol and Daybreak request eligibility

**Research date:** 2026-11-16 (UTC). Public, unauthenticated documentation/source GETs only. Follow-up to [Pi configuration research](0045-pi-access-programs-configuration.md) and [T3 adapter research](0045-t3-pi-adapter-and-config-path.md).

## Verdict: Eligible, with a decisive authorization qualification

**The exact OpenAI model `gpt-6.1-sol` explicitly supports `access_programs.cyber=daybreak_blue` in the Responses API, but the documented access requirement is Daybreak Red approval plus the required project access—not Daybreak Blue approval alone.** This is an explicit first-party model/program rule, not an inference from its name, capabilities, SDK enum membership, or a client catalog. [O1]

OpenAI's **“Choose the model and access program”** section states:

> Reduced refusals on `gpt-6-astra` and `gpt-6.1-sol` require Daybreak Red access, but the request value is `daybreak_blue`. Both models reject `daybreak_red`. Daybreak Blue approval alone doesn't authorize reduced refusals on either model. Your project must also have the required access enabled.

Its **“Send a request”** section separately confirms the exact ID:

> If your organization has Daybreak Red approval and access is enabled for your project, you can also use `gpt-6.1-sol`. The request still selects `daybreak_blue`:

The published example names `gpt-6.1-sol`; it was read, **not executed**. [O1]

| Distinct question | Evidence-backed answer |
| --- | --- |
| Does the exact model support the Blue request value? | **Yes, conditionally:** the model/program guide explicitly documents it. [O1] |
| Is reported Daybreak Blue authorization alone sufficient for reduced refusals on this model? | **No:** the quoted rule explicitly requires Red access and project enablement. This does not question the user's reported Blue authorization. [O1] |
| Does this establish the user's full entitlement or the effective T3 request behavior? | **No; unverified here.** No account, project, credentials, request, or live effective adapter was inspected. The earlier T3 note's API/auth/config uncertainty remains. |

**No override, authorization change, model/API-route switch, or blocked-content retry is justified by this note.** A supported parameter is not a guarantee that any particular prompt or action is allowed. OpenAI says other cybersecurity safeguards continue to apply and a selected program does not guarantee every safety check or prompt succeeds. [O1]

## Exact IDs and aliases are not interchangeable

- **`gpt-6.1-sol`:** OpenAI's exact model page identifies this ID and lists it under “Snapshots.” The API changelog's **September 29, 2026** entry announces “GPT-6.1 Sol (`gpt-6.1-sol`).” Official Node SDK **v7.57.0** also includes it in `ChatModel`, which is included in `ResponsesModel`. These establish public model identity; the explicit eligibility authority is O1, not the enum. [O2–O4]
- **`openai/gpt-6.1-sol`:** the supplied Pi/T3 slug combines a provider label and that model ID. This research establishes OpenAI's contract for the ID, not what a custom provider, alias, or proxy actually routes it to. No live T3 catalog/account request was made.
- **Bare `gpt-6.1`:** the guessed official model-page URL returned **404**. No source inspected establishes it as an alias for Sol. A missing page is not proof that no alias exists anywhere. Do not substitute a family/base name for the exact documented ID. [O5]
- **`gpt-daybreak-blue-latest`:** its official model page is marked **Deprecated**, describes an alias for general-purpose models, displays `gpt-5.6-sol`, and lists both identifiers under “Snapshots.” It does **not** establish a mapping to `gpt-6.1-sol`. The Daybreak guide warns that `-latest` resolution can change and depends on approved access. No alias resolution was tested. [O1, O6]

The previously reported installed Pi static-catalog absence and live T3 catalog presence are client-registry observations, not OpenAI eligibility authorities. No additional client inspection was needed.

## Explicit selection versus defaults

**Could an eligible, authorized request already receive Daybreak behavior without an explicit parameter? Yes, under the model-specific documented defaults—not necessarily in this T3 session.** [O1]

The owning guide's **“Understand defaults when omitted”** section states:

- Mainline models **such as `gpt-6-sol`** receive Daybreak Blue treatment when the organization and project have the required access; otherwise standard safeguards.
- For **`gpt-6-astra` and `gpt-6.1-sol`**, omission means “Reduced refusals for eligible callers with Daybreak Red access enabled for their project; otherwise, standard safeguards.”
- “Starting with `gpt-6.1-terra`,” omission results in `standard` treatment instead of defaulting to the highest available Daybreak tier. This does not supersede the guide's separately named Sol rule.
- Explicit Daybreak selection fails if incompatible with the model or if required access is missing. With no program specified and standard safeguards, response `access_programs` is `null`. [O1]

The public Responses reference and SDK **v7.57.0** accept `standard`, `daybreak_blue`, and `daybreak_red`. Their general rule resolves omission from model Cyber tier and organization/project access, “subject to model-specific eligibility restrictions”; models without a Cyber tier default to Standard, authorized Blue-tier models use Blue, and requests requiring unavailable Daybreak access return **403**. The model-specific guide above supplies the crucial restriction for Sol. [O1, O7–O8]

**Standard/default treatment does not itself prove ineligibility.** The guide explicitly permits Blue selection for the named mainline example `gpt-6-sol`. Conversely, neither the generic schema nor this example establishes that **every** Standard-default/no-Cyber-tier model accepts explicit Blue. That requires a model-specific support rule. “Standard” processing/pricing is also not a Cyber entitlement classification. No general model-support claim is inferred from it. [O1, O7–O8]

## Scope and remaining limits

Read the two prior notes for context; investigated a bounded set of current official model/index pages, the Daybreak and cybersecurity guides, GPT-6 guidance, API changelog, and versioned official SDK source/changelog. Only this new research file was written. No configuration/extensions, inference, authenticated account/model-list requests, credential/environment/session inspection, blocked-experiment retries, tier manipulation, issue edits, commits, pushes, or subagents.

The exact-model public documentation gap is **closed** by O1; another support answer is unnecessary to establish its published request-value support. What remains unverified is application to the actual T3 route and full entitlement. Existing nonsecret provider/integrator documentation identifying the routed model and supported API/auth surface, together with an existing administrator/support statement of the applicable program/project access, could resolve those separate questions. No secrets, credential re-verification, new API probe, or access change is requested.

The previously blocked program-announcement URL was **not fetched again or bypassed**. The public Codex cyber-safety URL returned **308** in this client and was not used as evidence. No blocked content or provider operation was retried.

## Primary-source ledger

All successful sources below retrieved **2026-11-16 UTC**. Developer pages are live/unversioned; SDK links pin **v7.57.0**. Digests identify retrieved bytes, not an assertion of future page stability.

- **O1 — Owning model/program rule:** [Use Daybreak in the Responses API](https://developers.openai.com/api/docs/guides/daybreak), especially [Choose the model and access program](https://developers.openai.com/api/docs/guides/daybreak#choose-the-model-and-access-program), “Send a request,” “Understand defaults when omitted,” and “Handle errors.” HTML **200**, SHA-256 `771f55ad2c2ed57935d310b45c4623c4d986c2b7245afa0c79a28eab804244a8`. Official [Markdown representation](https://developers.openai.com/api/docs/guides/daybreak.md), **200**, SHA-256 `0bfe0d6a38804fb55f56623ddbd6ab0f499990c6b336e89e7fce2fcf09e20c38`: exact Sol/Red qualification lines **104–108**, exact-ID example introduction **128**, defaults **146–156**, response/alias semantics **158–172**. The guide distinguishes `400 invalid_access_program`, `400 unsupported_access_program`, and `403 access_program_not_enabled`; these were documentation reads, not observed request errors.
- **O2 — Exact model identity:** [GPT-6.1 Sol model page](https://developers.openai.com/api/docs/models/gpt-6.1-sol), **200**, SHA-256 `ba70edbb5e41f1b6d14af8e3eaf274be11c5917572865d8ba0fb60aa0e0d9590`. No Daybreak eligibility conclusion relies on its generic capability/pricing sections.
- **O3 — Official release history:** [API changelog](https://developers.openai.com/api/docs/changelog), September 29, 2026 Sol launch; August 7, 2026 Daybreak announcement describes general-purpose models **such as GPT-5.6 Sol**, not blanket eligibility for every Sol model. **200**, SHA-256 `ec5f83bfffd6688d615b62909660f626e421e8c25c4dc23d6b6813c4c701e213`.
- **O4 — Versioned model-ID types:** [`shared.ts` v7.57.0, lines 102–105](https://github.com/openai/openai-node/blob/v7.57.0/src/resources/shared.ts#L102-L105), `ResponsesModel` lines **592–621**. Public raw source **200**, SHA-256 `8feecf9ec262604ad883e8f8c4aaa49f4b12be067996c8f6df3baed323f06de7`.
- **O5 — Limited negative finding:** `https://developers.openai.com/api/docs/models/gpt-6.1`, **404**. No alternate route/retry performed.
- **O6 — Different alias:** [Daybreak Blue model page](https://developers.openai.com/api/docs/models/gpt-daybreak-blue-latest), **200**, SHA-256 `7cd811f0363e604f2a7a2cdfe417a196a7c3c004a7d3a1fb8d733d4021dc41a2`.
- **O7 — General Responses contract:** [Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create), request `access_programs.cyber`; **200**, SHA-256 `4d6434e5b29c5c6a0e7306f195ba81e0e6559b134353a67f39ee0c682ef50a33`.
- **O8 — Versioned general schema:** [`responses.ts` v7.57.0, lines 12445–12462](https://github.com/openai/openai-node/blob/v7.57.0/src/resources/responses/responses.ts#L12445-L12462), request-program documentation; base field **12106–12110**, response field **1647–1653**. Public raw source **200**, SHA-256 `b8069d29546925dcabf1bcf472cc76bdc05fc5522b302823b3e7635d8b9137ea`.
- **O9 — Entitlement separation corroboration:** [Cybersecurity checks](https://developers.openai.com/api/docs/guides/safety-checks/cybersecurity), “Authorized access and agentic workflows”: approval applies to the authorized person/service, workspace or API organization/project, model, and product surface; model ID alone does not grant access. **200**, SHA-256 `b04cc5b158e9714097f26a8c69d960a2554fa73fc23de3fd3464ea21d07d3256`.
