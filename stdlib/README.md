# Standard-library source

This directory owns Rill-language standard-library implementations. The approved layout includes a future explicit `std.rill` facade for the public library modules. The first scaffold contains this ownership note only, not that facade, placeholder modules or working implementations.

Keep language-level algorithms here. Target-runtime support belongs in [`runtime/`](../runtime/README.md); compiler analysis and intrinsics belong to their implementation owners. This boundary does not settle production ABI or provider integration details.

Public behavior remains defined by the [standard-library contracts](../docs/standard-library-contracts.md) and the [contract index](../docs/README.md#contracts). This README does not duplicate their signatures or rules.

Future library-local tests belong with their implementation. Cross-component standard-library acceptance is owned by [`tests/`](../tests/README.md) through `test:stdlib`, which currently exits nonzero as unavailable. Existing [prototypes](../docs/prototypes/README.md) remain design evidence, not passing library tests.

The future installed source lives in `lib/rill/std/`, selected through the installed toolchain rather than the working directory. See [development](../docs/development.md#future-installed-toolchain) for the distribution boundary and current limits.
