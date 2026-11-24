# GitHub automation

This guide owns CI/CD terminology and YAML layout under `.github/`.
The root `AGENTS.md` still applies.

## CDF naming and grouping

Use the CD Foundation's [Pipeline Terminology](https://github.com/cdfoundation/sig-interoperability/blob/main/docs/pipelines-terminology.md) as reference terminology.
It is unfinished guidance, not a normative standard.
The prefixes and formatting below are project conventions.

Workflow and job display names use `Stage: Description`.
Choose stages by the work performed.

| Stage | Meaning |
| --- | --- |
| Build | Compile or assemble software, documentation or generated configuration. |
| Test | Test, scan or verify software and project rules. |
| Release | Package, version, sign or publish artifacts and release records. |
| Deploy | Change an external environment and verify deployment. |
| Operations | Monitor deployed assets. |

Step display names use `Type: Description`.
Use these distinctions:

- Source retrieves repository contents.
- Setup prepares tools, dependencies, caches, credentials or workspace inputs.
- Build compiles software or renders configuration.
- Test runs a test suite. Scan performs non-test verification, including linting.
- Policy enforces project rules.
- Package creates a distributable artifact. Tag applies a version label. Computing a tag string alone is Setup.
- Publish uploads artifacts or records.
- Deploy applies infrastructure or updates service configuration. Verify Deployment checks the rollout.
- Run covers automation spanning several types. Release is a stage, not a step type.

Use combined prefixes when one action spans categories.
Examples include `Build and release` for a job and `Build, package and publish` for an image action.
Separate jobs when execution gates or permissions differ.
Separate Setup and Deploy commands into steps when they share a runner and working directory.
Keep dependency order driven by execution needs, not the stage list.
Consult the linked CDF definitions for additional step types.

## YAML layout

- Use two-space indentation and double-quote display names containing `:`.
- Order workflow blocks as name, triggers (`on`), permissions, concurrency, `env` and jobs. Separate top-level blocks and jobs with one blank line.
- Group job name, dependencies (`needs`), condition (`if`), runner and timeout first. Follow with strategy, permissions, outputs, environment, `env`/`defaults`, then steps. Separate these groups with one blank line. Omit absent groups.
- Begin steps with name, then optional `id` and condition (`if`). Keep action `uses` beside its `with` inputs.
- Put shell, working directory and environment before a shell step's `run`. Preserve other keys beside the behavior they configure.
- Separate steps with one blank line. Keep explanatory comments directly above their step or setting.
- Expand multiple job dependencies as a block sequence. Short trigger filters may use inline lists, such as `branches: [main]`.
- Use `|` for multi-command scripts and `>-` for a single folded command. Preserve command order, expressions and quoting semantics.

## Rill tools and gates

Use mise for tools and task execution, Cargo for Rust dependencies and uv for Python dependencies.
Keep tool versions and action revisions pinned.
Use explicit runner-local Cargo and Rustup homes before mise installs tools.
These homes keep nested mise commands on the same Rustup provider.

Keep scaffold checks LLVM-free.
Preserve unavailable product gates and their nonzero exits until real implementations replace them.
A scaffold build or LLVM smoke does not establish Rill execution or target support.
Use the [development guide](../docs/development.md) for task scope and prerequisites.

## Changes and verification

Check required GitHub status contexts before changing job display names.
Report unavailable configuration checks rather than assuming they are clear.

For formatting-only edits, compare parsed YAML before and after.
Use a parser that preserves GitHub's `on` key and expression strings.
Completion requires identical values and sequence order.
Verify every intentional value change separately, including names.
For structural edits, verify dependencies, output references, event conditions, permissions and execution gates.

Run `mise run check:ci` for workflow changes. Full suites and gates run only in CI; follow step 3 of the [toolchain development guide](../docs/agents/development.md) for local scope.
Run `git diff --check` before completion.
Use the pinned Actionlint task instead of installing a separate copy.
Verify runner-label or action-metadata diagnostics against their definitions before applying narrow filters.
Report filtered diagnostics, disabled shell checks and commands that failed or were unavailable.
