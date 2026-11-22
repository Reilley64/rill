# Toolchain development

1. Read the [development guide](../development.md) for source ownership and the current scaffold limits. Locate task definitions and tool pins in `mise.toml`; use `mise tasks ls` to discover entry points.
2. Use mise for tool selection and task execution, Cargo for Rust dependencies and uv for Python dependencies. Treat lock updates as intentional changes. Keep the compiler and IDE analysis independent of native LLVM installation.
3. Run `mise run check` and `mise run build:scaffold` for scaffold changes, and `mise run check:ci` when changing workflows. Select additional product checks by the changed component; for compiler analysis that spends work, entries or depth, follow the [compiler metering standard](compiler-metering.md). Run heavy provisioning or experiments only through their explicit profiles after reviewing the prerequisites.
4. Report each check's actual scope and result. Keep unavailable product checks nonzero until real implementations and suites replace them. Scaffold compilation, Python/model checks and an LLVM C-API smoke do not establish Rill execution or complete client acceptance.

Completion requires passing applicable implemented checks and an explicit list of unavailable or unrun acceptance gates. Release/support claims require every supported profile, not a successful skip or a configured CI matrix. Preserve prototype sources and recorded experiment evidence unless the task expressly changes them.
