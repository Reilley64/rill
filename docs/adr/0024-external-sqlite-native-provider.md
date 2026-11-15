# Keep SQLite external with a closed, trusted native-provider contract

**Status: human-confirmed Q1–Q58 and consolidated recording plan; design recorded, implementation/integration validation outstanding. #43 stays open.** #40 already places shared execution in std::sql. Keep SQLite's driver, engine dependency and provider external rather than adding std::sqlite or assuming that an ordinary package can call private runtime/C functions. Reserve ADR 0024/prototype 26 without overwriting the separately prepared #33 ADR 0022 and #34 ADR 0023 artifacts.

Use the [closed provider contract](../sqlite-provider-contracts.md), versioned data-only binding metadata, generated marshalling and a fixed C shim with explicit byte frames, checked capabilities, resource adoption and runtime ownership hooks. This deliberately requires a new narrowly approved build/runtime mechanism; it does not authorize general FFI, arbitrary metadata-driven native packages, runtime symbol loading, Rill callbacks inside SQLite or public OS-thread pinning. Native code and generated/runtime glue are trusted and need actual implementation validation, not merely a matching signature/version.

Choose a controlled SQLite 3.54.1 reference build with complete private symbol isolation, serialized/FULLMUTEX/private-cache use, restricted authorizer policy and copying boundaries. Keep application-native settings fixed; preserve independently typed errors, operation-scoped interruption, known/unknown transaction outcome and actual cleanup before reuse/release. A bounded executor/reserved interrupt path and non-abandonable cleanup admission avoid blocking ordinary scheduler workers without promising forcible abortion or bounded completion. A generation-checked wake protocol and atomic Rill adoption prevent stale notification/ownerless handoff only when the future implementation actually satisfies them.

## Alternatives and consequences

- **std::sqlite:** rejected after the placement discussion; concrete backend maintenance does not belong in the shared standard seam.
- **Unspecified package FFI / arbitrary native plugins:** rejected. External packaging is not authority to bypass ownership, build or native-code trust decisions.
- **System SQLite / static linking as sufficient isolation:** rejected. Actual source/features/global state and symbol resolution must be controlled and verified.
- **Full SQLite pass-through / user callbacks / optional modules:** deliberately narrower initially. Native semantic authorization and the pinned VFS/source remain part of the trusted boundary, not a SQL sandbox.
- **Zero-copy buffers, automatic retries, generic fresh-session reset:** not initial guarantees. Copy owned data, preserve errors and discard when a documented reuse baseline is unavailable.
- **Close #43 on paper:** rejected. [Reference checks](../../prototypes/26-sqlite-provider/README.md) and source/schema/state models are inputs, not the missing compiler/runtime/native driver. #30/#43 support gates and the other target cells remain open; drafting #28's RFCs and the closed #40 shared design are not blocked by pretending this provider is delivered.
