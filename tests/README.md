# Test ownership

The first scaffold tests infrastructure, not the Rill language. Manifest dependency checks and CLI help/version or unavailable-command tests can pass without a compiler, standard library, runtime or server. Successful empty product suites would hide that distinction; product test tasks currently exit nonzero as unavailable.

Keep component-local unit tests beside their Rust, standard-library or runtime implementation. This directory owns future cross-component acceptance inputs and runners. Current tool-infrastructure tests live beside their scripts in `tools/`; CLI process tests live in `crates/cli/tests/`. The task boundaries below reserve ownership, not implemented suites or new fixture formats.

| Future acceptance scope | Entry point and owner |
| --- | --- |
| Language conformance and diagnostics across compiler stages | `test:language`, compiler implementation and the owning language contracts. |
| Standard-library behavior exercised by Rill programs | `test:stdlib`, standard-library implementation and its public contracts. |
| Runtime, ABI and generated target behavior | `test:runtime`, runtime/backend implementation and existing production gates. |
| Complete applicable OpenIDEA client/protocol/UI behavior | `test:lsp-client`, IDE/LSP implementation and the [LSP acceptance contract](../docs/lsp-contracts.md). |

`check` remains LLVM-free infrastructure verification. `test:llvm` is a separate optional C-API compile/link smoke, not inkwell integration or backend acceptance. A fixture-schema validator can check fixture shape but cannot prove that Rill accepts, rejects or runs its source.

[Rill prototypes](../docs/prototypes/README.md) remain chronological design evidence. [Standalone experiments](../experiments/README.md) retain their own revisions, tools, outputs and limitations. Future conformance cases can reference that evidence without moving it or rewriting expected behavior as executed results. Explicit `experiment:*` reproduction tasks do not replace product acceptance.

See [development task status](../docs/development.md#task-profiles-and-current-status) for current commands and limits. [#28](https://github.com/Reilley64/rill/issues/28) retains production/formal backend acceptance, and [#42](https://github.com/Reilley64/rill/issues/42) retains complete client delivery. Infrastructure success closes neither gate.
