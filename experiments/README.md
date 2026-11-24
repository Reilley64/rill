# Standalone experiments

These programs test backend or provider mechanisms without implementing the complete Rill language.

- [Backend and stack experiments](backend-30/README.md) include native admission, tail transfer, cleanup/lifecycle and portable Wasm cases.
- [SQLite provider experiments](sqlite-provider/README.md) include a Python protocol model and a pinned native reference.
- [Statechart 1: the life of one owned value](statechart-1-owned-value/README.md) is a TLA+ model of single-routine cleanup, checked with TLC.

Each runbook gives its prerequisites, commands and evidence limits. Recorded JSON and results preserve the original runs and paths. Use current reproduction commands; do not rewrite historical logs to match the new layout.

[Rill source examples and case notes](../docs/prototypes/README.md) are a separate collection.
