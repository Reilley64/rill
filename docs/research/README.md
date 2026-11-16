# Research

These reports record primary sources, comparisons and experiment results. They support the [contracts](../README.md#contracts) and [RFCs](../rfcs/README.md); they do not amend accepted rules by themselves.

## Initial language surveys

These reports predate later Rill decisions. Their original report-only commits are now included on main.

- [GC-free memory management](memory-management.md)
- [Concurrency without async/await](colorless-concurrency.md)
- [Polymorphism without traits](polymorphism-without-traits.md)
- [Rust-to-LLVM bindings](rust-llvm-bindings.md)
- [References and mutation without lifetime annotations](safe-references-without-lifetimes.md)

## Repository and toolchain setup

- [Compiler, standard-library and LSP repository layouts](compiler-stdlib-lsp-repository-layouts.md) examines source ownership, semantic reuse, tests and releases across six language ecosystems.
- [mise toolchain management](mise-toolchain-management.md) examines tool/version/task ownership, platform prerequisites and unresolved LLVM 23 installation choices.
- [LLVM 23 provisioning options](0046-llvm23-provisioning-options.md) checks actual upstream and Conda artifacts, native prerequisites and static-link verification requirements.

These reports support the setup interview. [#46](https://github.com/Reilley64/rill/issues/46) records accepted answers and open decisions; research suggestions alone do not authorize implementation.

## Backend experiments

Start with the [runtime-validation summary](0030-runtime-validation.md). The earlier [backend experiment](0030-backend-experiments.md), [mechanism research](0030-backend-mechanisms.md), [portable candidate survey](0030-portable-suspension-candidates.md) and [portable experiment report](0030-portable-suspension-experiments.md) retain earlier findings and limits. The [runnable programs and recorded artifacts](../../experiments/backend-30/README.md) are separate from these summaries.

## Library and tooling investigations

- Time and cancellation have reports on [token models](0033-cancellation-token-models.md), [clock/zone boundaries](0033-clock-and-zone-boundaries.md), [Jiff's API](0033-jiff-api-model.md), [checked arithmetic](0033-jiff-checked-arithmetic.md) and [wire formats/differences](0033-jiff-wire-and-differences.md).
- Networking has [TCP/DNS and cancellation research](0034-tcp-dns-platform-and-cancellation.md).
- TLS has [provider/I/O research](0038-tls-provider-and-native-io-boundaries.md), [verification/protocol limits](0038-tls-verification-trust-and-protocol-limits.md), [trust/identity/time](0038-trust-sources-identity-and-certificate-time.md) and [crypto profiles/revocation](0038-openssl-crypto-profiles-and-offline-revocation.md).
- SQL has [standard interface/error research](0040-standard-sql-and-unique-errors.md) and [SQLite provider research](0043-sqlite-native-provider.md).
- Conversions have [Rust coherence/evidence research](0041-q19-rust-conversion-evidence.md).
- RFC conventions have [language-process research](0026-language-rfc-formats.md).
- LSP has the [OpenIDEA capability inventory](openidea-language-server-capabilities.md).

Check each report's date, pinned revisions and executed-versus-planned evidence before reusing a claim. Standalone/native/model execution is not Rill compiler execution.
