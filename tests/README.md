# Test ownership

The first scaffold tests infrastructure, not the Rill language. Manifest dependency checks and CLI help/version or unavailable-command tests can pass without a compiler, standard library, runtime or server. Successful empty product suites would hide that distinction; product test tasks currently exit nonzero as unavailable.

Keep component-local unit tests beside their Rust, standard-library or runtime implementation. This directory owns future cross-component acceptance inputs and runners. Current tool-infrastructure tests live beside their scripts in `tools/`; CLI process tests live in `crates/cli/tests/`. The task boundaries below reserve ownership, not implemented suites or new fixture formats.

| Future acceptance scope | Entry point and owner |
| --- | --- |
| Language conformance and diagnostics across compiler stages | `test:language`, compiler implementation and the owning language contracts. |
| Standard-library behavior exercised by Rill programs | `test:stdlib`, standard-library implementation and its public contracts. |
| Runtime, ABI and generated target behavior | `test:runtime`, runtime/backend implementation and existing production gates. |
| Language server conformance for supported capabilities | `test:lsp-client`, IDE/LSP implementation and the [language server contract](../docs/lsp-contracts.md). |

`check` remains LLVM-free infrastructure verification. `test:llvm` is a separate optional C-API compile/link smoke, not inkwell integration or backend acceptance. A fixture-schema validator can check fixture shape but cannot prove that Rill accepts, rejects or runs its source.

[Standalone experiments](../experiments/README.md) retain their own revisions, tools, outputs and limitations. Conformance cases can reference that evidence without moving it or rewriting expected behavior as executed results. Explicit `experiment:*` reproduction tasks do not replace product acceptance.

## Prototype build and test targets

Use the maintained [prototype sources](../docs/prototypes/README.md) directly. M6 ri-qlh tracks the first native targets.

- `build:prototypes` compiles the selected intended-valid entry files with the real compiler.
- `test:prototypes` runs those programs and checks their expected output, exit status and applicable expectations.

Start with Hello World on an available native profile. Add entry files as the backend and standard library support them. Keep intentional rejections in the diagnostic checks. Record unsupported entry files and their prerequisites separately; do not count them as passing. Keep these targets separate from the LLVM-free scaffold checks.

### Prototype performance baselines

After correctness checks pass, collect three separate timings for a fixed set of supported prototype inputs in native CI:

- **Build:** Compile with fresh output files. Exclude toolchain installation and the build of the compiler itself.
- **Test:** Measure the full test command. Label any compilation that the command does as part of this measurement.
- **Run:** Execute the already compiled prototype. Do not include compilation in this timing.

Repeat measurements and publish the samples and median. Record the compiler revision, prototype and inputs, target, build options and runner profile. Hello World measures startup, not throughput. Start with reporting only. Set regression thresholds after a stable baseline exists for comparable runs. Timing results do not replace pass or fail results from the correctness checks.

See [development task status](../docs/development.md#task-profiles-and-current-status) for current commands and limits. ri-48g retains production/formal backend acceptance, and ri-bv2 tracks language server work. Infrastructure success does not validate either scope.
