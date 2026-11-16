# T3's actual Pi integration and model-configuration path

Read-only investigation, **2026-11-16**. Follow-up to [Pi and `access_programs.cyber`: configuration versus authorization](0045-pi-access-programs-configuration.md).

## Result: integration identified; effective request route not yet verified

**The installed T3 host implements Pi as an external CLI subprocess controlled through JSONL RPC, not as an in-process `createAgentSession()` embedding.** Its normal session launcher runs the configured `binaryPath` (or `pi`) with `--mode rpc`. Current process-name/parent metadata is consistent with that implementation: this research runs below a process named `pi`, below T3 Code (Beta). [M1, T1–T3]

**This does not prove which Pi JavaScript entry point/package the live process loaded, or its effective model API/authentication route.** The shell resolves `pi` to global Pi **1.5.2**, but T3 supports a different binary per provider instance. The live Pi-named process's executable image is Node, not the JavaScript module path. No process arguments, private settings, runtime registry, sessions, or request payloads were read. Thus the remaining verification gate is real: **do not install an `access_programs` override based on this investigation alone.** [M1–M3, T1–T3]

### Verified / conditional / unknown

| Question | Finding and evidence status |
| --- | --- |
| What does the installed T3 Pi adapter launch? | **Verified static source:** external Pi CLI, JSONL RPC; configurable binary and launch arguments. **Observed:** Pi-named Node process below T3. [T1–T3, M1] |
| Does T3 embed global Pi 1.5.2? | **No such embedding is established.** Installed T3's normal adapter uses a CLI boundary. **Verified installed candidate:** global Pi coding-agent and Pi AI packages are 1.5.2. **Unknown:** live process's JS entry point/package version; parent's package; any other child's package. [M1–M3] |
| Current model identity? | **Observed in this research tool environment:** `PI_PROVIDER=openai`, `PI_MODEL=gpt-6.1-sol`. These are identification metadata, not a read of the live selected model object. The task supplies `openai/gpt-daybreak-blue-latest` as another available identifier; its API implementation was not observed. [M2] |
| Parent/child effective API adapter? | **Unknown for both identifiers and both roles.** Neither a T3 Pi instance label nor an `openai/...` model slug proves `openai-responses`, `openai-codex-responses`, or a custom implementation. [T4–T5, P4–P6] |
| Exact safe configuration location? | **Conditional source-proven lookup:** inspected Pi 1.5.2's normal CLI uses `<agentDir>/models.json`; with its stock package metadata and the observed unset directory override, that is `/Users/reilley/.pi/agent/models.json`. **Not proven to have been read by the live process.** [M2–M3, P1–P3] |
| Does that standalone file exist? | **Observed path metadata only:** yes. Its contents, validity, configured models, and whether any live runtime loaded it are unknown. [M3] |
| Are model-level overrides lost because T3 sends only an ID? | **Not on the inspected stock Pi 1.5.2 RPC path:** Pi resolves the ID to its full registry model, then selects that object. Model-level sampling defaults survive this selection. **Unknown live applicability** until actual package/registry/implementation are identified. [T4, P3–P6] |
| Can T3 child `options` pass arbitrary nested `samplingParams`? | **Not through the inspected selection path.** T3 selection values are strings/booleans, and its Pi adapter consumes `thinking` and `serviceTier`, not arbitrary body fields. [T4–T6] |
| Does a new child or refresh reload the file? | **Source-proven conditional lifecycle:** a fresh stock Pi process loads it; stock 1.5.2 CLI RPC `new_session`/`switch_session` recreates services and a model runtime. **Unknown:** which transition a particular T3 parent/child uses, its instance/config, and whether it uses this Pi package. Catalog discovery alone does not refresh the current runtime. [T7–T8, P2, P7] |
| Is Daybreak Blue authorized for this request route? | **User reports authorization. Not independently verified**, including which project, endpoint, authentication method, or model it covers. No credentials or account services were inspected. |

## 1. Scope and method

Only this new document was written. Existing untracked research notes were left alone. There were no configuration edits, patches, installed extensions, application launches, package installation/unpacking to disk, inference/account calls, experiment retries, credential commands, issue changes, commits, pushes, or subagents.

Inspected inputs were:

- The preceding research note and official documentation/source shipped with the installed Pi packages: SDK, CLI integration, model/configuration, environment, and provider documentation, plus relevant generated JavaScript.
- Nonsensitive process metadata: PID/parent/name and executable-image paths only. **No process argument or environment dump.**
- Explicitly selected path/model metadata variables only: `PI_CODING_AGENT_DIR`, `PI_PACKAGE_DIR`, `PI_PROVIDER`, `PI_MODEL`.
- T3's installed application plist/package metadata and selected shipped server-source members. The reverse-engineering skill was read first. A read-only ASAR-header/member reader inspected source directly in the archive; no application code was evaluated, no archive was extracted, and no debugger/runtime instrumentation was used.
- Existence metadata for the candidate `models.json`, **not its contents**.

No `auth.json`, private session/prompt/log file, API-key/token variable value, or endpoint secret was read. Public code describing credential handling is source evidence, not inspection or execution of credentials. No network fetch was needed for this follow-up; the versioned upstream Pi links below identify the official release corresponding to the local package.

## 2. Observed host versus the installed Pi candidate

### Current process observations [M1–M3]

At approximately **2026-11-16 15:01 +11:00**:

- The research command's ancestor chain contained process **3995**, named `pi`, parent **87218**, whose executable-name path was `/Applications/T3 Code (Beta).app/Contents/MacOS/T3 Code (Beta)`; that T3 process's parent **87208** had the same application path.
- macOS `proc_pidpath(3995)` returned `/opt/homebrew/Cellar/node/26.13.0_2/bin/node`. `proc_pidpath(87218)` returned the T3 application executable path. These identify executable images, **not the Node script or loaded module version**. PIDs are an observation snapshot, not durable identities.
- The tool environment had `PI_CODING_AGENT_DIR` **unset**, `PI_PACKAGE_DIR` **unset**, `PI_PROVIDER=openai`, `PI_MODEL=gpt-6.1-sol`. This is the environment inherited by the research command; it is not a direct read of T3's parent/other child environment or the Pi registry. A host/extension could also change environment after startup.
- `command -v pi` returned `/Users/reilley/.bun/bin/pi`; resolving the symlink yielded `/Users/reilley/.bun/install/global/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js`.
- That installation's `pi-coding-agent/package.json:1–15` identifies **1.5.2**, stock `.pi` config directory, and the bundled CLI entry point. Its `pi-ai/package.json:1–4` identifies **1.5.2**. `/Users/reilley/.pi/agent/models.json` exists; its contents were not read.

**Limit:** shell command resolution is not proof of what an already-running T3 instance selected. Even a matching installed version is not proof that a process started earlier loaded the current on-disk bytes.

### Installed T3 artifact

`/Applications/T3 Code (Beta).app/Contents/Info.plist` reports bundle ID **`com.t3tools.t3code`**, version/build **0.1.9**. Its `Contents/Resources/app.asar!/package.json:1–5` reports package `t3code`, version/build **0.1.9**, and build commit **`24e77473ecf4`**. The package's main entry is the desktop boot script; it does not establish an embedded Pi package version.

These files are from the same application path seen in the process metadata. This identifies the installed artifact at that path, **not a byte-level attestation of code already loaded into the running process**.

## 3. Installed T3 source: launch, selection, and filtering

### CLI, not direct SDK embedding

The shipped `provider-pi` implementation:

1. Accepts provider-instance `binaryPath`, `launchArgs`, and custom model identifiers. Its settings description explicitly says launch arguments are passed to **`pi --mode rpc`**. [T1]
2. Merges host environment with provider-instance environment; instance variables can replace inherited variables, including a Pi directory override. It does not special-case or remove `PI_CODING_AGENT_DIR` in the inspected RPC launch builder. [T2]
3. Builds the RPC launch, adds T3's normal bridge extension, and opens the subprocess using `binaryPath || "pi"`, the session working directory, and the launch environment. **T3 does not supply an SDK `agentDir` or `modelsPath` at this boundary.** Pi CLI owns that lookup. [T3]
4. Validates launch arguments. T3 reserves RPC/session-control flags and prohibits positional prompts. Explicit extension arguments are supported; discovery/one-shot modes have different extension behavior. Reading these branches did not execute them. [T3, T8]

This narrows the earlier note's generic SDK possibilities: `modelsPath: null` is a real Pi SDK capability, but **not a setting seen in T3's normal Pi RPC launcher**. A configured wrapper/fork could behave differently; its identity is unknown here.

### What crosses model selection

T3's live adapter obtains `get_available_models`, resolves the slug, and sends:

- `set_model` with `provider` and `modelId`;
- `set_thinking_level` for its recognized `thinking` option;
- a bridge command to select/reset `serviceTier` when needed. [T4]

It **does not serialize the entire T3 catalog model into Pi**, nor does it forward every selection option into the provider request. Its catalog projection keeps slug/name/sub-provider and capabilities, not the full `api`, endpoint, or sampling object. Consequently a model being selectable in T3 is not evidence of its adapter. [T5]

T3's general selection schema accepts string/boolean values. Legacy object options are coerced by retaining string/boolean entries, not nested objects. Pi model capabilities advertise thinking and supported service tiers. There is no `samplingParams`/`access_programs` option handler in the inspected Pi selection function. Supplying a JSON string under an invented option name does not make it a body override. [T4–T6]

**`serviceTier` is not Cyber program selection.** The shipped bridge's `before_provider_request` handler shallow-copies the built payload and conditionally sets **`service_tier`**, only when advertised by the active model. It also caps certain OpenRouter output-budget fields. Its inspected handler does not delete unrelated body keys or set `access_programs`. T3's label “Standard” here means omitting `service_tier`, not `access_programs.cyber=standard`. Other loaded extensions, wrappers, or upstream proxies remain uninspected. [T6]

### Why an ID-only selection can preserve sampling defaults

In stock installed Pi 1.5.2:

1. `ModelRuntime` composes provider models with file-backed `modelOverrides`. These are applied after custom model/extension model composition; unknown override IDs do not add models. Sampling objects merge per top-level key. [P3–P4]
2. RPC `set_model` searches `getAvailableSnapshot()` and passes the **full found model object** to `session.setModel()`. That assigns the object to agent state; it does not reconstruct a minimal provider/ID-only model. [P5]
3. SDK request-option construction spreads existing options; the stream calls `modelRuntime.streamSimple(model, ...)`. Request preparation retains the model or shallow-copies it with an auth-resolved base URL, rather than stripping sampling fields. [P6]
4. Provider composition chooses extension/base/API streaming implementations. The applicable stock adapter then determines whether sampling defaults reach the wire. A custom provider can behave differently. [P4, P6]

Thus **the T3 catalog projection is not itself a loss of Pi's model-level configuration**. That configuration remains inside Pi. This is a verified source dataflow for the inspected versions, **not proof that either live model currently carries such defaults**.

## 4. Model file resolution and refresh

### Candidate path, not a certified live read

Stock Pi 1.5.2 derives `APP_NAME`/`CONFIG_DIR_NAME` from its package metadata. `getAgentDir()` uses the derived `PI_CODING_AGENT_DIR` when set, otherwise `homedir()/.pi/agent` for this package. The normal CLI passes that directory to `createAgentSessionServices()`, which constructs `ModelRuntime` with `modelsPath: join(agentDir, "models.json")`. [P1–P2]

Given **this inspected stock installation**, the known user home, and the observed unset directory variable, the lookup target is:

**`/Users/reilley/.pi/agent/models.json`**

This is **the exact conditional lookup**, not permission to edit it and not evidence that its current contents loaded successfully. Project `.pi/settings.json` is not the model configuration file. A project worktree alone does not create a separate models lookup. Different provider instances, package forks, wrappers, or directory overrides can change the conclusion. [T1–T3, P1–P3]

For a direct SDK host, `createAgentSession({ agentDir })` derives a model path unless a custom runtime is supplied; `ModelRuntime.create({ modelsPath: null })` disables model-file loading. These remain useful SDK distinctions but must not be substituted for evidence about this T3 subprocess. [P3]

### Refresh and parent/child boundaries

- **Current RPC model inventory is not a reload:** Pi's `get_available_models` returns the current available snapshot; `set_model` selects from it. T3's `discoverModels()` calls that command. Neither of those functions reloads `models.json`. T3's provider-health catalog discovery launches a separate short-lived offline/ephemeral RPC process; refreshing that catalog does not itself update an existing live Pi runtime. [T4, T7–T8, P5]
- **Explicit runtime refresh exists:** Pi `ModelRuntime.refresh()` reloads the model file and recomposes providers. It can also refresh provider catalogs; no refresh was executed here. The standalone interactive `/model` behavior from the preceding note is not evidence that opening T3's model selector invokes it. [P7]
- **Stock CLI session replacement is stronger than generic SDK reuse:** T3 can issue `new_session`/`switch_session` when registering a different conversation in an existing Pi process. In inspected Pi 1.5.2, RPC forwards these to the runtime host; `AgentSessionRuntime` invokes its runtime factory, and the normal CLI factory constructs fresh services/model runtime. This path rereads the same agent-directory file. It does **not** require a new OS process in this specific version. [T7, P2, P7]
- **A fresh stock Pi process** also loads the normal model file during service construction. This is not a recommendation to launch one now: startup can load extensions, resolve credentials, or perform network activity. [P2–P3]
- **Actual T3 child behavior remains unobserved:** the driver supports multiple provider instances, with instance-specific binary/environment. This research did not inspect private orchestration state or exercise a child launch. A child using the same stock binary and directory through one of the above construction paths should read the same file; merely saying “new child” does not establish those premises, parent configuration equality, or that another already-running session refreshed. [T1–T2, T7]

## 5. API/auth route is still the decisive missing fact

The installed stock `openai` provider declares the Responses implementation and offers API-key **and ChatGPT OAuth** authentication. The separately installed stock `openai-codex` provider declares the Codex Responses implementation and ChatGPT OAuth. Provider/model replacement, per-model API selection, native/legacy extensions, virtual routing, and auth-resolved endpoint changes are additional possibilities. **These source defaults do not identify the actual adapter or authentication method for the supplied live slugs.** [P4, P6, P8]

The relevant adapter difference remains independently confirmed in installed Pi AI 1.5.2:

- `openai-responses` resolves model/thinking/request `samplingParams` and merges them into the built body after named fields.
- `openai-codex-responses`' complete inspected body builder does **not** merge `samplingParams`, although shared base-option code can carry them internally.
- Both have payload hooks; the existence of a hook does not establish that this field is supported or authorized on that route. No hook/extension was written, installed, or run. [P9]

The preceding note documents the official public Responses contract and the distinction between program selection and server-side authorization. This follow-up makes **no new entitlement claim**. The user's reported Daybreak Blue authorization is not evidence of which T3 instance, organization/project, model, endpoint, or subscription/API route is in use. No tier selection, endpoint/auth change, content-block retry, or program override is proposed.

## 6. Minimal nonsensitive evidence that would close the gap

The missing facts do **not** require credentials, raw request bodies, transcripts, or account retries. An authorized host maintainer could provide an existing sanitized diagnostic/build report containing only:

1. **For the relevant parent and child separately:** provider-instance identity, selected driver/model identifiers, resolved Pi CLI entry/module/package path and loaded package version/build. Do not supply full launch arguments or environment values.
2. Resolved **agent directory and model-file path**, whether model-file loading is enabled, and whether that file parsed successfully in that runtime. Report paths/status only, not configuration contents or credential-source details.
3. Effective selected/routed model **`provider`, `id`, `api`**, virtual-routing status, and whether the implementation is stock or custom. To verify override preservation, only whether model sampling defaults are present and the relevant nonsecret field names is necessary—not their unrelated values or any payload.
4. A nonsecret **authentication-method label** (API key versus ChatGPT subscription/OAuth versus custom/proxy), plus an administrator's confirmation of the applicable endpoint/project authorization. Do not expose tokens, headers, credential commands, full URLs with queries, or endpoint secrets.
5. Which existing runtime transition occurred: fresh process, stock CLI runtime replacement, explicit refresh plus model reacquisition, or reuse of an old runtime/model. A T3 catalog refresh alone is insufficient.

This research did not collect or add runtime diagnostics: doing so may require new instrumentation outside the permitted read-only source/metadata scope. If those sanitized facts are unavailable, the correct answer remains **unknown**, not “configure the global file and try again.”

**Practical legitimate next step:** verify those metadata facts with the T3 integration owner, and have the account/provider administrator confirm the documented request contract for the existing model/authentication route and project authorization. Only after both are known should the owner evaluate a provider-supported configuration mechanism. If a content restriction appears mistaken, use provider support/review; configuration is not an authorization or policy bypass.

## Source ledger

### Installed T3 source citations

Archive root **A**: `/Applications/T3 Code (Beta).app/Contents/Resources/app.asar`.

References use **archive member line numbers**, counted from 1 in decoded UTF-8 source, not byte offsets or upstream TypeScript line numbers. Generated source comments identify original module paths; citations concern the shipped JavaScript, not an assumed upstream checkout.

- **R** = `A!/apps/server/dist/runtimeLayer-BR0FRrjb.mjs`
- **C** = `A!/apps/server/dist/ManagedEndpointRuntime-B-62Mn5X.mjs`
- **T1:** `C:122385–122402` (`provider-pi/src/settings.ts`), `R:125273–125289` (multiple instances/config).
- **T2:** `R:20618–20633` (`provider-core/src/server/instanceEnvironment.ts`), `R:124899–124909` (adapter's host/instance merge), `R:121718–121748` (launch environment).
- **T3:** `R:122660–122715` (adapter session launcher), `R:121536–121658` (launch-argument restrictions), `R:121718–121748` (RPC args/bridge).
- **T4:** `R:124080–124085` (inventory), `R:124179–124250` (model/thinking/service-tier selection).
- **T5:** `R:124944–124976` (T3 model inventory projection); this projection does not establish the underlying API.
- **T6:** `C:5391–5415` (string/boolean selection schema/coercion), `R:122369–122404` (Pi capability descriptors), `R:121327–121366` (shipped bridge payload handler).
- **T7:** `R:124095–124126` (register/resume/new-session requests).
- **T8:** `R:124978–125016` (separate discovery process and inventory), `R:122222–122235` (one-shot text-generation route explicitly differs in extension handling).

SHA-256 of the inspected **member bytes**:

- `package.json`: `02942097722594515f7f35dbdc83b6a4031a8983887e59755dac3f1e6f8c2874`
- **R**: `12a122fbadaef5afd3e2a22d85b4d4ad2fb31b8e6f30293945524d96c44edb7c`
- **C**: `5fd72c095729cfa72bc1c8ebe48bbd1d8e1fc22ac203f5d10c92a06f17c68d73`

These digests identify inspected artifacts, not live-memory execution. No whole-application integrity/loaded-code claim is made.

### Official installed Pi documentation/source citations

**CA** = `/Users/reilley/.bun/install/global/node_modules/@earendil-works/pi-coding-agent`; **AI** = sibling `pi-ai`. Both package metadata versions: **1.5.2**. Official matching release: [`v1.5.2`, commit `b4afdddf44e059c67ccf5097e5317d913521b470`](https://github.com/earendil-works/pi/tree/b4afdddf44e059c67ccf5097e5317d913521b470). Local line references below are generated JavaScript/doc lines, not upstream TS line numbers.

- **P1:** `CA/docs/configuration.md:3–23`; `CA/docs/environment-variables.md:77–85`; `CA/dist/config.js:488–516`; `CA/package.json:1–15`. Official [configuration documentation](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/coding-agent/docs/configuration.md).
- **P2:** `CA/dist/main.js:516,646–664`; `CA/dist/core/agent-session-services.js:53–68,71–111` (CLI factory, model-file lookup, extension registrations, refresh).
- **P3:** `CA/docs/sdk.md:95–108`; `CA/dist/core/sdk.js:69–80`; `CA/dist/core/model-runtime.js:79–85`; `CA/dist/core/model-config.js:132–163` (load errors/validation/composition). Official [SDK documentation](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/coding-agent/docs/sdk.md).
- **P4:** `CA/docs/models.md:67,116–138`; `CA/docs/models-configuration.md:58–85`; `CA/docs/custom-provider.md:24–28`; `CA/dist/core/provider-composer.js:42–85,348–391` (override merge/order, stream dispatch). Official [models](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/coding-agent/docs/models.md) and [custom providers](https://github.com/earendil-works/pi/blob/b4afdddf44e059c67ccf5097e5317d913521b470/packages/coding-agent/docs/custom-provider.md).
- **P5:** `CA/dist/modes/rpc/rpc-mode.js:387–405`; `CA/dist/core/agent-session.js:2110–2126` (full registry model selection and snapshot-only inventory).
- **P6:** `CA/dist/core/sdk.js:193–210,224–229,252–273`; `CA/dist/core/model-runtime.js:459–490,504–529` (option/model preservation, routing and auth-resolved endpoint possibility).
- **P7:** `CA/dist/core/model-runtime.js:597–608`; `CA/dist/modes/rpc/rpc-mode.js:356–362,496–501`; `CA/dist/core/agent-session-runtime.js:128–172`; `CA/dist/main.js:646–664,830` (refresh versus runtime replacement, CLI passes its runtime host to RPC).
- **P8:** `CA/docs/cli-integration.md:3–7,55–83,91–105`; `CA/docs/providers.md:3–17`; `AI/dist/providers/openai.js:7–30`; `AI/dist/providers/openai-codex.js:6–20` (stock provider/auth/API defaults only).
- **P9:** `AI/dist/api/simple-options.js:26–57`; `AI/dist/api/openai-responses.js:134–154,307–313`; `AI/dist/api/openai-codex-responses.js:170–188,375–437` (sampling/body/hook distinction).

### Observation references

- **M1:** current-process ancestor `ps` fields limited to PID/PPID/command name; macOS `proc_pidpath` executable-image paths. No arguments or other-process environments read.
- **M2:** explicit tool-environment lookup of the four path/model metadata variables listed above. No wildcard enumeration.
- **M3:** shell executable lookup/symlink resolution, installed package name/version/config-dir metadata, and candidate model-file existence check only.
