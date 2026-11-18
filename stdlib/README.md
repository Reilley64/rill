# Standard-library source

This directory owns Rill-language standard-library implementations. `core.rill` declares the fixed prelude families but supplies no providers for them. `option.rill` defines `Option` and five Rill helpers. The complete standard library and its `std.rill` facade are not present.

Keep language-level algorithms here. Target-runtime support belongs in [`runtime/`](../runtime/README.md); compiler analysis and intrinsics belong to their implementation owners. This boundary does not settle production ABI or provider integration details.

Public behavior remains defined by the [standard-library contracts](../docs/standard-library-contracts.md) and the [contract index](../docs/README.md#contracts). This README does not duplicate their signatures or rules.

Future library-local tests belong with their implementation. Cross-component standard-library acceptance is owned by [`tests/`](../tests/README.md) through `test:stdlib`, which currently exits nonzero as unavailable. Existing [prototypes](../docs/prototypes/README.md) remain design evidence, not passing library tests.

The compiler's [installed test](../crates/compiler/tests/m4-8-acceptance.md#installed-option-checks) stages these files in `lib/rill/std/`. The loader takes that path from the running executable, not the working directory. See [development](../docs/development.md#future-installed-toolchain) for the distribution layout.
