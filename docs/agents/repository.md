# Repository maintenance

Use the [documentation index](../README.md) to find a document's owner before changing it. Apply `writing-for-agents` to agent instructions and `unslop` to reader-facing prose. Editorial cleanup preserves approved semantics, RFC metadata/checklists and the distinction between expected and observed behavior.

## Artifact locations

- Keep Rill sources and their input/output fixtures in `prototypes/`.
- Put their explanations and case inventories in `docs/prototypes/`.
- Keep runnable backend/provider experiments, local runbooks and recorded artifacts in `experiments/`.
- Put primary-source investigations and experiment summaries in `docs/research/`.
- Keep contracts in `docs/`, decision rationale in `docs/adr/` and proposals in `docs/rfcs/`.

Use full filenames in references. Several historical numeric prefixes repeat; a number alone is not artifact identity. Keep published identities unless a separate change explicitly renames them.

## Moving artifacts

1. Check the working tree and other worktree/branch ownership. Leave another session's uncommitted files untouched.
2. Record an old-to-new path map. Move complete source/fixture groups without changing their bytes.
3. Repair Markdown links, current reproduction commands, script paths and generated-report templates.
4. Preserve recorded evidence JSON, original command transcripts, source/tool hashes and byte-sensitive fixtures. Historical commands may retain old paths; explain that distinction in the runbook.
5. Run affected structural/model tests and check links and whitespace. Report which checks ran and which native/Rill checks did not.

A move is complete when every old path is accounted for, current references resolve and preserved artifacts match their recorded bytes. Moving a report does not rerun or upgrade its evidence.

## Branch cleanup

Check merge ancestry and worktree use before deleting local branches. Keep checked-out or unpublished work. Pre-rewrite research histories may have no merge base with main; import the useful report-only commits rather than merging obsolete snapshots. Retain original refs until their history can be retired safely. List remote deletion candidates separately.
