# #35 path and filesystem evidence

Read the source examples in [`prototypes/20-filesystem.rill`](../../prototypes/20-filesystem.rill).

[ADR 0019](../adr/0019-paths-files-and-filesystem-operations.md) is the human-confirmed Q1-Q44 decision. The [source companion](../../prototypes/20-filesystem.rill) has **16 intended-valid lexical expectations**, plus effectful application workflow functions. All are hand-checked: no Rill compiler, OS filesystem, WASI host, security suite or backend was executed. #28/#30 retain formal/runtime gates.

## Public API and ownership coverage

The companion checks native non-UTF-8 bytes, embedded-NUL offset reporting, checked text conversion, absolute-child replacement, separator spelling, empty-child joining, byte inequality, parent/filename edge cases, exact leading slash count, component kinds and explicit Path copying. It uses canonical eq rather than inventing operator overloading. Component names/text views retain Path dependencies; parent returns owned Path. Empty lexical values are not filesystem locations.

Effectful functions demonstrate bounded config/certificate byte loading with separately retained read/close errors; chunked file copying with explicit exclusive destination creation, bounded memory/output budget and retained unaccepted input; explicit finalization collecting every requested flush/sync/close outcome; and budgeted directory observation through canonical next. They are application examples, not extra standard helpers or executed fixtures.

## Bounded config and certificate workflow

`loadBounded(path,maximum)` opens ReadFile, uses readToEnd, closes on success/failure and returns owned bytes only after a successful read and close. Its LoadError retains both read and close failures when needed; IoError is noBorrow so failure does not retain the consumed handle/local probe. When rejecting a failed/oversize load, this application deliberately discards its local partial config bytes; the nested standard limit error still preserves its overflow byte and simultaneous probe failure. This application policy is not a change to readToEnd's destination contract.

Text configuration explicitly composes successful bytes with std::string::decodeUtf8; no automatic lossy path/config decoding. Certificate loading passes owned bytes to a separately designed #38 parser/provider; it does not prove certificate validity, trust or secure permissions. Metadata-before-read is not a security or stable-size check; the streaming budget is enforced during reading even if the file grows.

## Bounded copy workflow and partial effects

1. Open the source with openRead. Open destination with `newCopyDestination`, which requests NewOnly/Preserve. Do not preflight exists or silently truncate an existing destination/source alias. If destination opening fails, explicitly close the already-open source when its close error matters.
2. Call copyBounded with borrowed handles, a maximum output-byte budget and positive chunk size. Source/destination handles remain caller-owned on either result. The reusable chunk buffer grows only up to min(chunkSize, remaining); one separate probe byte can be retained at the limit. No whole-file allocation.
3. A read error retains newly consumed but unaccepted bytes as pending; a write error reports total accepted output and retains only the unaccepted suffix. It never resends a confirmed prefix. A limit error retains the excess byte and any simultaneous probe failure without writing past the output budget. EOF exactly at the limit succeeds.
4. Call finishCopy for both success and failure. It records flush, optional explicit sync, output-close and input-close outcomes independently rather than using an early `?` that skips the second close. Inspect all report fields as well as the copy result. Consuming close has no retryable handle.
5. No rollback or atomic publication: an error/cancellation can leave a newly created partial destination. NewOnly protects exclusive creation, not the safety of attacker-controlled ancestor paths. Any desired rename/directory-sync protocol is a separate explicit application sequence with backend-specific guarantees.

copyBounded uses concrete trusted standard file implementations, not an arbitrary adapter family. Standard primitives must satisfy #32 count/destination invariants; generic standard helpers validate impossible reports before arithmetic/slicing. It is not permission for unsafe native access based on an unvalidated arbitrary callback count. The copied counter cannot overflow: every successful step is bounded by maximum-copied. Failure progress is bounded by that step's requested extent.

finishCopy's example policy attempts every requested finalization operation even after another reports failure. File flush is not fsync; sync requests file persistence but does not sync the containing directory. Its owned typed error report retains originals; disposal/cancellation cannot promise the application receives this report.

## Directory and temporary-resource workflow

The following is signature-level application usage, not executable top-level startup or a new API:

```rill
// Explicitly obtain/validate a parent Path; no implicit env/global temp lookup.
// let directory = tempDir(&parent)?;
// let child = join(tempPath(&directory), &relativeChild);
// let temporary = tempFile(tempPath(&directory))?;
// writeAll(&mut temporary, &content) ... inspect progress/errors
// sync(&mut temporary) ... if explicitly wanted
// close(temporary) ... inspect TempFileCloseError including both failures
// close(directory) ... succeeds only if empty; no recursive deletion

// Keeping is not syncing, renaming or closing:
// let [ordinaryHandle, originalPath] = keep(temporary);
// let ordinaryDirectoryPath = keep(directory);
```

Actual code must handle each Result and preserve applicable errors; ellipses/comments are not Rill implementations. An absolute relativeChild would replace the base, and `..` could traverse: join is not confinement. To observe a path after guard consumption, explicitly copy(tempPath(&guard)) before close/keep. External name replacement/renaming can affect later path cleanup; use a trusted parent when that property matters.

readDir owns the enumeration resource. To observe its close error, retain an iterator binding, call next/observeSome under a budget, then explicitly close the owner; a consuming for-of loop's disposal cannot report close failure. Entries own one-component Path names, never `.`/`..`; callers can compose with a parent spelling but that is not reopening by a stable directory capability. EOF/error becomes terminal while the resource remains owned until close/disposal. No sorting/snapshot/recursive traversal or unlimited accumulation.

TempDir with children: explicit close returns DirectoryNotEmpty and consumes the guard. Caller-owned children must be cleaned explicitly; a copied path can support later direct removal attempts. Disposal also cannot recursively destroy children or promise deletion. TempFile close attempts both handle closure and unlink, retaining both typed failures; moving the handle into close prevents automatic field cleanup closing it again. Temporary construction interrupted before return must safely close owned native state and attempt best-effort deletion of its created name.

## Independent lexical/invalid cases

Each rejection case stands alone; do not assume another invalid case was accepted.

| Case | Expected behavior |
| --- | --- |
| Byte path [255,120] | Factory preserves both bytes; UTF-8 view fails; backend support is separately checked on use. |
| NUL inside text/bytes | PathError.EmbeddedNul reports byte offset; no native truncation. |
| Empty Path | Lexically valid; every filesystem-location operation returns InvalidInput, never substitutes `.`. |
| Absolute join child | Replaces base; no containment promise. Empty child preserves base. |
| Repeated separators / dot components | Stored spelling/equality unchanged; no lexical normalization or symlink resolution. |
| Root `//` | Component preserves exact leading count; do not claim native equivalence with `/`. |
| Parent of `a//b/c` | `a//b`; retained interior spelling remains. |
| Borrowed filename/text | Using view after destroying/moving its source Path rejects; copying view is not copying the owner. |
| Component iterator escape | Returning a component/name depending on a destroyed local Path rejects. |
| Path factory from string variable | Explicit borrow and owned copy; no implicit argument conversion or hidden move/copy of source. |
| Ordinary return-only open | A generic open selected solely by desired handle type is not introduced. |
| Write through ReadFile | Missing canonical write capability rejects before monomorphization. |
| Copy/duplicate owner | No implicit file-handle copy/public duplication or hidden shared cursor. |
| Concurrent whole-handle operations | Overlapping exclusive read/write/seek borrows reject; separate opens do not isolate file contents. |
| Use after close/keep | Owner is consumed even on recoverable failure; borrowed tempPath cannot outlive it. |
| Buffered seek/sync | No transparent implementation; decompose and account for unread/pending bytes. |
| Hidden generic evidence | A generic seek/sync/stream helper without the canonical requirement rejects before instantiation. |
| Wrong expected E | No return-error dispatch/implicit error conversion; preserve typed envelopes or explicitly adapt. |

## Independent native effect, symlink and durability traces

| Case | Expected behavior |
| --- | --- |
| NewOnly existing regular file | Atomically AlreadyExists; no check-then-create window. |
| NewOnly dangling symlink | Existing link is AlreadyExists; do not follow it to create a missing target. |
| IfMissing + Preserve existing file | No truncation; writes start at zero and can leave an old suffix. |
| Explicit Truncate | Destructive even if later setup/cancellation fails; no rollback. |
| Append after seek | Write remains append; read/write handle reads can use cursor positioning. |
| Concurrent multi-call append | Each write obeys backend append; writeAll is not one atomic record. |
| Negative/too-large seek | Error, never native-size wrap; beyond-EOF successful seek does not write/extend. |
| Pipe/device opened through path | Object access can work while seek/sync returns Unsupported; no universal device support. |
| Directory byte open | Reject with IsDirectory rather than expose ordinary byte-file semantics. |
| File changed after metadata | Later I/O can differ/fail; metadata has no reservation/snapshot guarantee. |
| metadata vs symlinkMetadata | First follows final link, second inspects it; neither prohibits intermediate links. |
| Relative symlink target | Resolved relative to link parent, not caller's current directory. |
| removeFile final symlink | Removes link, not target. removeDir does not traverse a link to delete a tree. |
| createDirs partial success | Created prefix may remain after later failure/cancellation; bounded-stack iterative traversal. |
| rename compatible existing target | Native replacement semantics only; no portable no-replace guarantee. |
| Cross-filesystem rename | Fail without copy-and-delete fallback or hidden partial-copy policy. |
| File flush / close | No durable sync implicitly added. |
| File sync only | No claim that containing-directory entry is persisted. |
| syncDirectory unsupported | Explicit Unsupported, not no-op success or universal crash transaction. |
| Temp close failures | Preserve close-only/remove-only/both envelopes; finish safe local cleanup. |
| Permissions / ACL | Conventional/private requests filtered by actual host policy; do not change umask or imply a universal ACL boundary. |
| No exists helper | Perform intended exclusive operation; do not translate permission failure into false/not-found. |

## WASI and unsupported-provider matrix (unvalidated obligations)

- Preopens `/data` and `/data/private`: `/data/private/a` selects the longer match, never falls back to `/data` after denial. `/database/a` does not match `/data` on a partial component.
- A relative path requires preopen `.`. With only `/data`, do not guess a base; report Unsupported. currentDir remains Unsupported on WASI v0.
- Once bound to a preopen, `..` remains host-resolved and cannot manufacture authority outside it. Do not lexically cancel it or retry against a broader descriptor.
- Ambiguous/unusable guest mappings fail explicitly; no enumeration-order choice. Root/mount spelling rules need formal #28 treatment, not a claim of host-tested resolution.
- Non-UTF-8 Path remains a lexical value but an incapable preview1 host returns InvalidInput, never replacement decoding. Preopen permissions/rights denial is PermissionDenied.
- Missing seek/sync/append/exclusive-create/symlink/private-temp support is Unsupported. Never emulate a guarantee using a weaker sequence or silently skip it.
- In particular, a provider unable to request/enforce the temporary private-creation policy must not pretend ordinary WASI creation meets it. Native permissions remain host-filtered requests, not universal guarantees.
- Capability mapping does not turn lexical join/prefix checks into a portable sandbox API. No new application manifest, host embedding API or ambient filesystem authority is implied.

## Cancellation and exactly-once cleanup obligations

| Sequence | Required ownership/result |
| --- | --- |
| Open/create commits; cancellation before return | Created/truncated effects may remain; registration owns handle until actual cleanup; no fabricated successful returned owner. |
| Temporary create interrupted before publication | Exactly one owner closes native state and attempts best-effort unlink/rmdir; no guaranteed deletion or recursive cleanup. |
| Read/write blocks; stop requested | Retain path/buffers/handle/callback state until completion or confirmed abortion. Abort request/blocking-pool transfer is insufficient. |
| Close error plus temp unlink error | Both envelopes retained; handle moved out before remaining-field disposal, never closed twice. |
| Enumeration EOF/error then close/dispose | Terminal next returns None; exactly one release of remaining native enumeration state. |
| Sync/rename interrupted | External effects not rolled back; no durability/atomicity proof from cancellation. |
| Cleanup cannot abort | May suspend indefinitely with cancellation deferred; timer race is not a hard return deadline. |
| Native close reports interruption | Provider must obey actual platform semantics; blindly retrying a reused numeric descriptor can close a different resource. |
| Wasm memory growth/native use | Owner retention alone does not prove pointer stability; #30 still gates backend behavior. |
| Fatal panic/program exit | Existing absence of universal cleanup/delivery guarantee remains. |

## Primary-source inputs and limits

These sources informed the interview; no source substitutes for Rill checking/provider validation:

- [POSIX filename definition](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap03.html#tag_03_146) and [pathname resolution](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap04.html#tag_04_16): byte names, slash/NUL boundaries and symlink-sensitive traversal.
- [Linux pathname(7)](https://man7.org/linux/man-pages/man7/pathname.7.html), [open(2)](https://man7.org/linux/man-pages/man2/open.2.html), [openat2(2)](https://man7.org/linux/man-pages/man2/openat2.2.html): pathname/access flags and the distinction between ordinary lookup and explicit resolution controls. Network/filesystem/provider behavior still needs validation.
- [Linux fsync(2)](https://man7.org/linux/man-pages/man2/fsync.2.html) and [Apple fsync(2)](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html): file/directory persistence and platform request differences; no universal hardware guarantee.
- [Apple APFS FAQ](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html), [filesystem details](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemDetails/FileSystemDetails.html), [QA1173](https://developer.apple.com/library/archive/qa/qa1173/_index.html): archived/version-qualified naming/normalization distinctions; not facts about every mounted macOS volume.
- [Pinned WASI preview1 definitions](https://github.com/WebAssembly/WASI/blob/fae981bae14809d91f9bc2d63852d461f331d161/preview1/docs.md): preopens, rights and path/directory operations. [Pinned Wasmtime preview1 source](https://github.com/bytecodealliance/wasmtime/blob/c52bc6d8fc7ebf2ba3f7e39ab3203a3c13c1d272/crates/wasi/src/p1.rs) and [Wiggle UTF-8 reader](https://github.com/bytecodealliance/wasmtime/blob/c52bc6d8fc7ebf2ba3f7e39ab3203a3c13c1d272/crates/wiggle/src/lib.rs): one concrete strict-UTF-8 host, not arbitrary native-byte portability or a tested Rill provider.

#28 owns formal path/component/family inference, error/progress and dependency checks; #30 owns native/Wasm mechanisms. #33 time/deadline coordination remains, with no invented timestamp type. #36 env/arguments does not silently choose temp parents; #38/#39/#40 must explicitly address certificate reads, static-file traversal/security and migration/driver durability. Unworkable mechanisms return to map #1 rather than silently changing this contract.
