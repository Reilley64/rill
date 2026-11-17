# Pi and `access_programs.cyber`: configuration versus authorization

Read-only source/documentation research, 2026-11-16. **No provider inference/account API requests or blocked-experiment retries were performed.** No credentials, private sessions, request logs, or user configuration were inspected or changed. Network reads were unauthenticated public documentation/source GETs (including GitHub reference metadata). Only this research note was added.

## Conclusion

**Pi can represent and serialize this nested request field through its generic `models.json` `samplingParams` mechanism on specific adapters. That does not enroll an account in a program, establish model eligibility, or override a content restriction.** Installed Pi has no documented dedicated `access_programs.cyber` setting, environment variable, or CLI flag. There is no native typed cyber-program option in the inspected Pi schema/runtime; the generic mechanism accepts arbitrary body keys. [P1–P5]

The important qualifications:

- Pi **1.5.2** applies `samplingParams` to `openai-responses`, `openai-completions`, and `azure-openai-responses`. It **does not apply it to `openai-codex-responses`**, despite that adapter sharing some base-option code. [P2, P5–P7]
- Public OpenAI **Responses** documentation and official SDK **7.57.0** explicitly describe `access_programs.cyber` and accepted values: `standard`, `daybreak_blue`, `daybreak_red`. The field is real, not merely inferred from an automated block message. This is not evidence that every OpenAI-compatible API, Azure endpoint, subscription route, or model accepts it. [O1–O2]
- Installed `openai` SDK **7.19.0** lacks this named field in its Responses types, but the inspected runtime forwards the body to its generic JSON serializer, rather than selecting only recognized typed fields. Thus the old type surface alone is **not** a runtime obstacle to forwarding a JSON object supplied through Pi's generic mechanism. This is source evidence, not an end-to-end test. [P8, O3]
- Neither the supplied names `openai/gpt-6.1-sol` and `openai/gpt-daybreak-blue-latest`, nor a color in a model name, establish the active API/auth route or account entitlement. The current T3 parent's and children's effective model registry, configuration directory, provider implementation, and request options were not established. [P9–P10]

## Installed and upstream evidence scope

Local package roots used below:

- **CA** = `/Users/reilley/.bun/install/global/node_modules/@earendil-works/pi-coding-agent`
- **AI** = `/Users/reilley/.bun/install/global/node_modules/@earendil-works/pi-ai`
- **OA** = `/Users/reilley/.bun/install/global/node_modules/openai`

`CA/package.json:1–4` and `AI/package.json:1–4` report **1.5.2**; `AI/package.json` declares `openai: 7.19.0`; `OA/package.json:1–4` reports **7.19.0**. This identifies installed files, **not which package/bundle T3 actually embeds**.

Matching official Pi release: [`v1.5.2`](https://github.com/earendil-works/pi/tree/b4afdddf44e059c67ccf5097e5317d913521b470), resolved to commit `b4afdddf44e059c67ccf5097e5317d913521b470`. The release's model documentation and selected TypeScript request-builder/composer sections agree with the installed JavaScript behavior described here. Local JS and upstream TS have different line numbers; references below identify both where relevant. This is not an assertion that every installed artifact was byte-compared.

## Actual dataflow and merge order

1. **Load/validate:** `ModelConfig.load()` reads `models.json`, parses it, validates it, clones/freezes provider configuration. `samplingParams` is `Record<string, unknown>`; its nested values have no cyber-specific allowlist. The generated JSON schema likewise permits arbitrary keys/values inside that object. This validates Pi configuration shape, not provider semantics. [P3]
2. **Compose models:** built-in/native models are combined with JSON model additions/replacements, extension model replacement, and legacy OAuth model projection. `modelOverrides` is applied last to the resulting **chat** models, by exact model ID. It does not create a missing model: unknown override IDs are ignored. Base and override `samplingParams` merge shallowly per key; each thinking-level entry also merges shallowly per key. [P4]
3. **Resolve request defaults:** Pi clamps the effective thinking level, then shallow-merges **model defaults → effective thinking-level defaults → per-request `samplingParams`**, with later top-level keys winning. Consequently, replacing `access_programs` replaces that whole nested object; this is not a recursive merge of `access_programs.cyber`. The simple-stream path resolves defaults in `buildBaseOptions`; the supported body builders also resolve them. [P2, P5]
4. **Build body:** the three supported adapters use `Object.assign(params, samplingParams)` **after** their named fields. There is no sampling-key allowlist in these paths. Then `onPayload` may replace the completed payload before transmission. A subsequent extension/host/proxy can still change or reject it. [P6]
5. **Serialize:** Responses calls `client.responses.create(params, requestOptions)`; SDK 7.19.0 passes the body to `POST /responses`, and its default encoder uses `JSON.stringify(body)`. Ordinary JSON-compatible unknown keys are not filtered by an SDK field-name allowlist on this path. This establishes source-level serialization capability, not server acceptance, successful authorization, or the actual payload of a T3 run. [P8, O3]

### API/auth applicability

| Effective adapter/integration | Source-backed result |
| --- | --- |
| `openai-responses` | Generic body merge exists. OpenAI's public Responses schema documents the field. Actual model, endpoint, organization/project and authentication eligibility still matter. |
| `openai-completions` | Generic body merge exists, but the checked official SDK 7.57.0 Chat Completions type source contains no `access_programs` declaration. Do not infer that OpenAI Chat Completions accepts this field merely because Pi can serialize it. |
| `azure-openai-responses` | Generic body merge exists. This research did not establish an Azure API-version/endpoint contract for this field. OpenAI's own schema does not prove Azure acceptance. |
| `openai-codex-responses` | `buildBaseOptions` can carry sampling defaults internally, but `buildRequestBody()` never merges them. Therefore `models.json` sampling defaults do **not** transmit this field on that adapter in 1.5.2. Its separate payload hook exists, but no provider contract for this field on the Codex backend was established. |
| Extension/native provider, proxy, virtual model, unknown route | A custom stream can implement different semantics; the composer may delegate to extension/base implementations rather than the generic adapter. No universal forwarding guarantee. |

Evidence: [P6–P10, O1–O4]. `openai` and `openai-codex` are not interchangeable labels: installed `openai` supports API-key **and ChatGPT-sign-in OAuth** authentication and uses the Responses adapter; installed `openai-codex` is OAuth-backed and uses the Codex adapter at the ChatGPT backend. Even `openai` therefore does not prove API-key authentication. The Responses adapter specially omits some normal named fields for ChatGPT sign-in, then performs the generic sampling merge; that technical merge is **not** evidence that subscription authentication is authorized to select a cyber program. [P7, P9]

## What configuration would look like, conditionally

The documented location is `<agent-dir>/models.json`, defaulting to `~/.pi/agent/models.json`; `PI_CODING_AGENT_DIR` or an SDK `agentDir`/custom model runtime can change the lookup. A project `.pi/settings.json` is not a substitute for that model configuration. [P1, P10]

The following is **only a schematic per-model override**, not an installed or tested configuration, not a suggested response to a content block, and not an access-tier change. `<already-registered-model-id>` is a placeholder: it must already exist under the effective provider. `standard` is a publicly documented request value, not a guess or an assertion that it is valid for either supplied model or account. [P2–P4, O1–O2]

```json
{
  "providers": {
    "openai": {
      "modelOverrides": {
        "<already-registered-model-id>": {
          "samplingParams": {
            "access_programs": { "cyber": "standard" }
          }
        }
      }
    }
  }
}
```

Only an authorization-compatible, provider-documented request on a supported effective adapter would justify choosing a program value. Do not silently add a model, change endpoints/authentication, select another tier/model, or retry blocked content to make this template apply. The two supplied model IDs were **not found in the installed static `AI/dist/providers/openai.models.js`**; remote catalogs or extensions could supply them. Their existence in the live registry remains unknown, so no concrete override for them is certified here.

### Extension/SDK alternative, not an authorization workaround

Pi documents `before_provider_request`: it receives a built payload, and a non-`undefined` return replaces the payload for later handlers and the request. The SDK connects this event through `onPayload`; the Responses and Codex adapters invoke that hook before sending. Custom streams are documented to honor it too, but an arbitrary implementation can fail to do so. [P6–P7, P11]

This is a legitimate integration mechanism **only where the provider's documented API and existing authorization permit the requested field**. Its availability does not establish support for `access_programs` on the Codex backend or authorize adding it to an otherwise unsupported route. No extension was written or installed. No payload/logging example was run.

## Reload, new session, and parent/child boundaries

- **Ordinary interactive Pi:** `docs/models.md:67` says opening `/model` reloads `models.json`; installed model-selector code refreshes the runtime, whose `refresh()` reloads the file and recomposes providers. Select the refreshed model rather than assuming a previously captured model object changed. A new conversation is not intrinsically required for model-file changes. This research did not execute `/model`; catalog refreshes can themselves involve network activity. [P2, P10, P12]
- **Extensions:** the documented lifecycle is `/reload` for changed, discovered provider extensions. That is distinct from proving model configuration was reloaded in every embedding. [P11]
- **SDK host:** `createAgentSession()` accepts a supplied `modelRuntime`/`model`, settings and resource loaders. Default creation reads the normal model file; `modelsPath: null` can disable it. A host retaining an old runtime/model must explicitly refresh/reacquire it. Merely creating a new conversation with a shared runtime does not prove a new configuration lookup. [P10]
- **T3 parent/child:** the installed Pi README explicitly says core Pi does not ship subagents. Its public SDK does not define T3's parent/child inheritance or child model-option translation. No T3 launch/runtime source was audited here. It is **not proven** that T3 child options expose arbitrary `samplingParams`, that parent and child use the same agent directory/runtime, that a Pi CLI reload affects either, or that a new child would read this file. A new child is neither demonstrated necessary nor sufficient. Do not infer support from a generic options object. [P1, P10]

## What the provider documentation establishes—and does not

The official Responses reference and SDK 7.57.0 describe `access_programs` as domain-specific access programs for a request. The `cyber` documentation says:

- Accepted values are `standard`, `daybreak_blue`, `daybreak_red`.
- If omitted, the server resolves the program from the model's Cyber tier **and organization/project access**, subject to **model-specific eligibility restrictions**.
- Models without a Cyber tier default to Standard. Blue-tier models use Daybreak Blue **when authorized**; otherwise Standard fallback can occur unless the model requires Daybreak access. Red-tier models require authorization.
- Requests requiring unavailable Daybreak access return **403**. Implicit Standard fallback is represented by `null` in the response's `access_programs`, not an explicit Standard selection. [O1–O2]

These are server rules, not a client-controlled entitlement. No account's enrollment, project authorization, model eligibility, or policy exception was established. The automated content-block message's exact context and correct remedy were not independently verified; a hint naming a field is not permission or evidence that setting it will unblock that content. Content/safety restrictions still apply to authorized requests. This note neither certifies permissible content nor recommends program selection as a way around a refusal.

**Practical legitimate next step:** have the provider/account administrator confirm the documented request contract for the existing model, endpoint and authentication method, and the organization's/project's actual program authorization. If the block appears incorrect, use provider support/review or the documented program application process. Only after that confirmation should the owning integrator decide whether the supported generic model override or a documented host integration is appropriate. No retries, alternate route, account changes or new provider calls are proposed here.

## Primary-source references and access limits

**Pi sources (all local paths relative to CA/AI above unless qualified):**

- **P1:** `CA/README.md:16–22`; `CA/docs/configuration.md:3–23,27–39`; `CA/docs/settings.md:1–24`; `CA/docs/cli.md:56–82`; `CA/docs/environment-variables.md:77–105`. Full documented settings/CLI/environment references and `CA/schemas/settings.schema.json`, `CA/dist/cli/args.js`, Pi AI runtime files were searched for `access_programs`/`accessPrograms`: no dedicated declaration found. This is a scoped negative finding, not a claim about all extensions or future versions.
- **P2:** `CA/docs/models.md:47–67,116–138`; `CA/docs/models-configuration.md:58–85`; matching official [release model documentation](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/coding-agent/docs/models.md#L116-L138).
- **P3:** `CA/dist/core/model-config.js:9–22,31–67,133–173`; `CA/schemas/models.schema.json:306–313`; official [model-config.ts:20–36,66–85](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/coding-agent/src/core/model-config.ts#L20-L85).
- **P4:** `CA/dist/core/provider-composer.js:42–85,348–369`; official [provider-composer.ts:166–210](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/coding-agent/src/core/provider-composer.ts#L166-L210) and [558–579](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/coding-agent/src/core/provider-composer.ts#L558-L579).
- **P5:** `AI/dist/types.d.ts:29–30,120–127,677–680`; `AI/dist/api/simple-options.js:26–57`; official [simple-options.ts:37–80](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/ai/src/api/simple-options.ts#L37-L80).
- **P6:** `AI/dist/api/openai-responses.js:134–154,257–310`; `openai-completions.js:187–202,736–742`; `azure-openai-responses.js:53–69,185–191` in the same directory. Official Responses [payload/send:195–228](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/ai/src/api/openai-responses.ts#L195-L228), [final merge:418–424](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/ai/src/api/openai-responses.ts#L418-L424).
- **P7:** `AI/dist/api/openai-codex-responses.js:170–188,356–370,375–440`; official [Codex body builder:541–621](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/ai/src/api/openai-codex-responses.ts#L541-L621), [hook:286–291](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/ai/src/api/openai-codex-responses.ts#L286-L291).
- **P8:** `OA/resources/responses/responses.mjs:17–28`; `OA/client.mjs:241,1235–1293`; `OA/internal/request-options.mjs:21–47`. These establish the default installed serialization path, not arbitrary custom SDK encoders/fetch implementations.
- **P9:** `AI/dist/providers/openai.js:7–30`; `AI/dist/providers/openai-codex.js:6–20`; `AI/dist/api/openai-responses.js:26–31,257–273`; `CA/dist/core/provider-composer.js:377–391`; `CA/dist/core/model-runtime.js:459–490,504–529` (auth can supply a base URL; custom/virtual routes can differ).
- **P10:** `CA/docs/sdk.md:95–108`; `CA/dist/core/sdk.js:69–80`; `CA/dist/core/model-runtime.js:79–85,597–608`; `CA/dist/core/model-registry.js:11–14`. Public SDK boundaries do not identify T3's actual choices.
- **P11:** `CA/docs/extensions.md:864–884`; `CA/docs/custom-provider.md:193–201,227`; `CA/dist/core/sdk.js:224–229,252–274`.
- **P12:** `CA/dist/modes/interactive/components/model-selector.js:96–114,127–151`; `CA/dist/core/model-runtime.js:597–608`; `CA/dist/modes/interactive/interactive-mode.js:4589–4594` (selection installs the chosen model). Model-file refresh is not assumed from the general `/reload` wording alone.

**OpenAI first-party sources:**

- **O1:** [Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create), request `access_programs.cyber` section. Public GET succeeded. Retrieved HTML SHA-256: `4d6434e5b29c5c6a0e7306f195ba81e0e6559b134353a67f39ee0c682ef50a33`. This page is live/unversioned; pin O2 for the corresponding text.
- **O2:** Official Node SDK **v7.57.0**, [`responses.ts:12104–12110`](https://github.com/openai/openai-node/blob/v7.57.0/src/resources/responses/responses.ts#L12104-L12110), [`12445–12462`](https://github.com/openai/openai-node/blob/v7.57.0/src/resources/responses/responses.ts#L12445-L12462), and [`12606–12618`](https://github.com/openai/openai-node/blob/v7.57.0/src/resources/responses/responses.ts#L12606-L12618) (streaming and nonstreaming share the base parameters). Retrieved file SHA-256: `b8069d29546925dcabf1bcf472cc76bdc05fc5522b302823b3e7635d8b9137ea`; it matched the retrieved upstream `master` alias. GitHub `main` metadata identified commit `a0719fc6c835756cdf76f6c38f19a2240f0f0426`; conclusions here use the versioned file rather than depending on a moving branch.
- **O3:** Official Node SDK **v7.19.0**, [Responses source](https://github.com/openai/openai-node/blob/v7.19.0/src/resources/responses/responses.ts) (whole file searched: no `access_programs`); [default encoder:115–143](https://github.com/openai/openai-node/blob/v7.19.0/src/internal/request-options.ts#L115-L143). Pi's installed version can serialize arbitrary JSON through this route without having a first-class field in its older SDK types. No claim is made about the first SDK release to add the field, or about every historical Pi version.
- **O4:** Official SDK [Chat Completions source](https://github.com/openai/openai-node/blob/v7.57.0/src/resources/chat/completions/completions.ts). The retrieved versioned 7.57.0 source had no `access_programs` occurrence; absence from that source is not a server acceptance test.
- **Access limits:** `https://openai.com/index/trusted-access-for-cyber/` returned **HTTP 403** on the public documentation fetch; no alternate access path was attempted. `https://developers.openai.com/api/docs/guides/safety-checks/` ended with **HTTP 308** in the fetch client; it was not used as evidence. Thus this note does **not** verify a full public enrollment procedure, detailed eligibility criteria, or program-specific policy relaxations beyond O1/O2. None of these documentation fetch outcomes were model/content-block experiment results.
