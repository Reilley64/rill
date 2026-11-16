<img src="assets/icon.svg" width="64" height="64" alt="">

# Rill

Rill is an experimental, functional-first language with TypeScript-style syntax, explicit ownership and goroutines. Its intended targets are native code and WebAssembly through LLVM.

This repository contains the language design, standard-library contracts, Rill source examples, standalone backend/provider experiments and an initial Rust toolchain scaffold. The scaffold implements CLI help and version output, not a working Rill compiler, standard library or language server.

- [Development](docs/development.md) covers mise setup, workspace ownership and available checks.
- [Documentation](docs/README.md) lists the contracts, decisions and research.
- [RFC conventions and register](docs/rfcs/README.md) track proposals and their review status.
- [Rill examples and case notes](docs/prototypes/README.md) distinguish source examples from executed checks.
- [Standalone experiments](experiments/README.md) contain backend and SQLite reference programs.
- [Language map](https://github.com/Reilley64/rill/issues/1) tracks design work and dependencies.

Created in [T3 Code](https://t3.codes).
