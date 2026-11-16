# Native paths, owned files and focused filesystem operations

[Issue #35](https://github.com/Reilley64/rill/issues/35) extends #23/#32 with paths, file handles and focused directory/temporary operations. The user approved Q1-Q44 and explicitly confirmed the consolidated contract. This records API/behavior design, not a filesystem implementation, security/soundness proof, backend validation or acceptance of #28's future RFCs. Use ADRs 0017/0018 for `implement` syntax and streaming/evidence semantics.

## Native path values and lexical operations

`std::path::Path` is an opaque owned byte-preserving value, not implicitly copied. Current targets use `/` separators and leading `/` for absolute paths; no Windows drives/UNC grammar is added. A Path is a spelling, not an opened object, filesystem identity or authority.

```rill
// One ordinary value-namespace factory family; the Path type is separate.
function Path<S>(source: S): Result<Path, PathError>;
// Implementations: (&string) and (&Bytes), each copying into owned storage.
enum PathError { EmbeddedNul { offset: u64 } }

pathBytes(&Path): &Bytes
pathText(&Path): Result<&string, Utf8Error>
isAbsolute(&Path): bool
join(&Path, &Path): Path
parent(&Path): Option<Path>
fileName(&Path): Option<&Bytes>
components(&Path): Components

enum PathComponent { Root(u64), Current, Parent, Name(&Bytes) }
```

Factories reject embedded NUL with a byte offset; valid UTF-8 strings are encoded without normalization and byte inputs are copied exactly. Construction does not check existence or backend acceptance. Empty is a valid lexical value, but every actual filesystem-location input rejects it with IoErrorKind.InvalidInput; it never means current directory. PathError has standard display and borrow-free/transfer/share evidence. UTF-8 conversion uses the existing Utf8Error, never replacement decoding. Byte/text views retain their source dependencies; no fabricated owner header or new borrowed path-subrange type.

Owned Path storage supplies standard noBorrow/sendable/shareable metadata; borrowing a Path view still retains dependencies. Canonical explicit copy returns independent owned Path storage; canonical eq/hash compare bytes exactly. `a/b`, `a//b` and `a/./b` are distinct values. Canonical show escapes invalid bytes/control characters for diagnostics; its output is not a Path factory decoding format or filesystem identity test. No case/Unicode normalization, tilde/environment expansion or lexical normalizer is added.

Join replaces the base with an absolute child; an empty child preserves the base and an empty base preserves the child. Insert one separator only when needed; preserve existing spelling, including repeated separators. Join is not confinement. Components distinguish a leading Root carrying the exact slash count, literal `.`/`..`, and borrowed ordinary names. Interior repeated/trailing separators delimit components rather than yielding empty names; source spelling remains unchanged. Components uses canonical iterate/next and ordinary source/dependency rules, not a new variadic or lifetime facility. POSIX may treat exactly two leading slashes specially; do not reduce them to one or claim all filesystems interpret them identically.

fileName returns the final ordinary-name bytes and ignores trailing separators. A final `.`/`..` or root has no fileName. parent ignores trailing separators and returns an owned prefix before the last component and its separator run, preserving a leading root run. It does not resolve `..` or symlinks.

| Path | Lexical result |
| --- | --- |
| `a/b/` | parent is `a`; fileName is `b`. |
| `a` | parent is empty. |
| `/a` | parent is `/`. |
| `//a` | parent is `//`. |
| `a/.` or `a/..` | parent is `a`; there is no fileName. |
| `/` or empty | There is no parent. |

Retained interior spelling is not normalized. For example, parent of `a//b/c` is `a//b`.

## File opens and capabilities

New effectful APIs live in `std::fs` and accept explicit &Path. Existing readFile(&string)/writeFile(&string,&Bytes) remain and gain &Path implementations in the same canonical families, not implicit conversions or unrelated implementation sets. Their old unbounded-read/partial-effect/non-atomic/non-durable guarantees are unchanged; use bounded streaming for untrusted input.

```rill
enum CreateMode { ExistingOnly, IfMissing, NewOnly }
enum WriteBehavior { Preserve, Truncate, Append }
type WriteOptions = { creation: CreateMode; behavior: WriteBehavior };

openRead(&Path): Result<ReadFile, IoError>
openWrite(&Path, WriteOptions): Result<WriteFile, IoError>
openReadWrite(&Path, WriteOptions): Result<ReadWriteFile, IoError>

enum SeekFrom { Start(u64), Current(i64), End(i64) }
function seek<F>(file: &mut F, from: SeekFrom): Result<u64, IoError>;
function sync<F>(file: &mut F): Result<void, IoError>;
```

Both WriteOptions fields are required; there is no generic open selected by expected return or hidden truncation. Read-only open never creates. ExistingOnly requires an existing entry. IfMissing creates only when absent and preserves existing contents unless Truncate was chosen. NewOnly atomically fails AlreadyExists on an existing final entry, including a dangling symlink: never check then create or follow a link to bypass exclusivity. All creation/behavior combinations are meaningful; Preserve starts at offset zero without shortening existing contents, Truncate explicitly shortens, Append remains authoritative over later seeks.

ReadFile/WriteFile/ReadWriteFile are non-implicitly-copying resource owners with one logical cursor; no public duplication/shared-cursor wrapper is added. Distinct types encode access, not regular-file/seekability guarantees. Reads/writes/seek exclusively borrow their handle. Separate opens have separate handles but no isolation from external contents changes. Filesystem-opened pipes/devices can have platform-specific behavior; directory byte I/O is rejected and directories use their own API. Unsupported object/backend operations fail explicitly.

| Handle | Canonical stream capabilities | File operations |
| --- | --- | --- |
| ReadFile | read; consuming close | seek; handle metadata |
| WriteFile | write; flush; consuming close | seek; sync; handle metadata |
| ReadWriteFile | read; write; flush; consuming close | seek; sync; handle metadata |
| TempFile | read; write; flush; consuming close | seek; sync; handle metadata; tempPath; keep |

File streaming implementations use IoError, hence read/write failures are IoFailure<IoError>. Ordinary file close returns Result<void,IoError>; temporary close differs below. Distinct ordinary input patterns, canonical identity/orphan/duplicate rules and pre-monomorphization evidence remain. Generic file helpers use seek(F)/sync(F), with the standard fixed IoError result; no general return-only dispatch or trait is introduced.

Seek returns the resulting absolute u64 position. Check negative/out-of-backend-range offsets, never wrap public values to native sizes. Beyond-EOF seeking does not itself extend/write. Append writes use the then-current end under faithful native append behavior; a seek may position reads but cannot turn append writes into positioned writes. Each underlying write is distinct; writeAll is not promised one atomic append or application-record transaction. Unseekable objects report Unsupported, not an invented position.

Unbuffered file flush has no Rill-owned pending output and does not call durable sync. Explicit sync on write-capable files requests platform file-data/required-metadata synchronization (e.g. native fsync semantics); it is not a universal hardware/crash guarantee. syncDirectory(&Path):Result<void,IoError> separately requests containing-directory metadata synchronization. Neither operation silently succeeds when unavailable. No transparent seek/sync for BufferedReader/Writer: explicitly flush/decompose, retain unread/pending bytes and operate on the returned handle. Reader decomposition exposes the actual advanced underlying cursor; Current does not invent a delivered-byte cursor.

Creation/truncation is not transactional: errors/cancellation can leave a created name or shortened contents after native success/later setup failure. Finish safe handle cleanup, but do not promise restoration/deletion for ordinary opens. Temporary construction additionally attempts cleanup of resources it created but could not return. No rollback, safe whole-buffer retry or atomic publication is inferred.

## Metadata, symlinks and directory operations

```rill
enum FileKind { Regular, Directory, Symlink, Other }
type Metadata = { kind: FileKind; size: Option<u64> };
function metadata<S>(source: S): Result<Metadata, IoError>;
// &Path follows the final link; &ReadFile/&WriteFile/&ReadWriteFile/&TempFile
// inspect the opened object without another path lookup.
symlinkMetadata(&Path): Result<Metadata, IoError>
readLink(&Path): Result<Path, IoError>
createSymlink(target: &Path, link: &Path): Result<void, IoError>
currentDir(): Result<Path, IoError>

readDir(&Path): Result<ReadDir, IoError>
type DirEntry = { name: Path };
createDir(&Path): Result<void, IoError>
createDirs(&Path): Result<void, IoError>
removeFile(&Path): Result<void, IoError>
removeDir(&Path): Result<void, IoError>
rename(source: &Path, destination: &Path): Result<void, IoError>
syncDirectory(&Path): Result<void, IoError>
```

Metadata size is Some(byte length) for regular files only, with no portable size for other kinds. This adds no timestamp type competing with #33, ownership-ID/ACL/permission-management API, identity token or exists helper. Metadata/entries are observations, not reservations/snapshots or locks. Native currentDir observes the process directory and can fail; no setCurrentDir is added. External embedding/native changes can still affect relative paths. Retaining an absolute spelling is not retaining authority or immunity to renames/symlinks.

Ordinary opens and directory lookup follow native symlink traversal. symlinkMetadata/readLink inspect the final link; intermediate components may still traverse links. A relative link target resolves against the link's parent when later used. removeFile/rename operate on a final link, not its target; removeDir does not recursively follow a link to delete a target tree. No all-components no-symlink open or hard-link API is added. Inspection-then-open, normalization and prefix checks are not sandbox/security boundaries.

ReadDir owns native enumeration state. Canonical iterate consumes it; next(&mut ReadDir) yields Option<Result<DirEntry,IoError>>. Entry names are owned one-component relative Paths, excluding `.`/`..`, with no full-path/cached-kind promise. Order is unspecified, enumeration is not a snapshot, and later joining the old parent spelling need not address the enumerated object after external renames. Iterate incrementally under caller budgets; no recursive walk, sorting or whole-directory accumulation is implicit. EOF or one enumeration error makes the iterator terminal; later next returns None. Any remaining native resource stays owned until consuming close(ReadDir):Result<void,IoError> or disposal, with exactly-once release and no recoverable close error through disposal.

createDir creates exactly one entry and errors AlreadyExists if it exists. createDirs iteratively creates missing ancestors, accepts existing directories and can leave created prefixes on failure/cancellation; symlink traversal remains native, not confinement. removeFile unlinks a nondirectory final entry (including a symlink); removeDir removes only an empty directory. No recursive removal helper. rename requests native compatible-destination replacement, not portable no-replace; cross-filesystem/unsupported operations fail without copy-and-delete fallback. Atomicity is only the backend/filesystem's actual guarantee, never crash durability. Do not emulate exclusivity/no-replace with existence checks.

Ordinary file/directory creation requests conventional native modes (POSIX 0666/0777), filtered by host umask/ACL/capability/filesystem policy. Temporaries request owner-only modes (0600/0700). Rill does not modify process umask or claim to override host policy; no general chmod/chown/ACL surface is added.

## Temporary ownership and cleanup

```rill
tempFile(parent: &Path): Result<TempFile, IoError>
tempDir(parent: &Path): Result<TempDir, IoError>
function tempPath<T>(temporary: &T): &Path;
function keep<T,R>(temporary: T): R;
// keep(TempFile) -> [ReadWriteFile,Path]; keep(TempDir) -> Path

enum TempFileCloseError {
  Close(Err<IoError>),
  Remove(Err<IoError>),
  Both { closing: Err<IoError>, removing: Err<IoError> },
}
// close(TempFile):Result<void,TempFileCloseError>
// close(TempDir):Result<void,IoError>
```

Constructors require an explicit parent; no automatic environment/global temporary-directory policy. Reserve fresh names exclusively with a finite provider collision-retry bound, failing rather than looping forever. Use faithful native temporary-creation facilities/private creation requests; names are not secrecy tokens or a new cryptographic API. Do not add a hidden insecure fallback when host support is insufficient. This is not permission to trust an attacker-controlled parent directory or claim complete path-race protection.

TempFile owns its ReadWriteFile and cleanup path. It delegates I/O/seek/sync without implicit copying. TempDir owns its cleanup path, not a promised directory capability. Borrowed tempPath depends on the guard; copy it explicitly before close/keep if needed afterward. keep consumes/disarms deletion and returns ordinary ownership; it does not rename/sync/close or promise persistence. Distinct input patterns determine the output R, not caller return dispatch.

Explicit TempFile close closes its handle, then attempts unlink even on close failure. It preserves the original envelopes for close-only, remove-only or combined failures. TempDir close attempts empty-directory removal. Both consume the guard even on recoverable failure. Disposal attempts best-effort deletion and required native cleanup without recoverable errors; it never recursively deletes children or guarantees the name disappeared. External unlink/rename/replacement can affect path-based cleanup; no unforgeable name authority is claimed. Move/clear initialized handle fields when delegated close consumes them so remaining-field cleanup cannot close them twice. Fatal panic/program exit retain existing lack of universal cleanup guarantees.

## Errors, WASI namespace and native retention

Extend IoErrorKind with Unsupported, NotDirectory, IsDirectory, DirectoryNotEmpty alongside the existing kinds. Invalid path encoding/input is InvalidInput; insufficient authority is PermissionDenied. Preserve native codes as optional/nonportable, owned diagnostic context and standard display/noBorrow/sendable/shareable for IoError. New PathError/TempFileCloseError have appropriate standard display/property evidence; storing their typed envelopes does not imply implicit error adaptation. Standard IoError already satisfies ADR 0018 Q36's scoped buffer/probe requirements; do not broaden those into restrictions on all primitive errors.

Native paths preserve bytes as accepted by the actual filesystem; some Linux names are not UTF-8 and mounted macOS filesystems need not share naming/normalization behavior. Reject unsupported encodings explicitly rather than replacing bytes. Ordinary lexical equality never becomes filesystem identity. POSIX path resolution, not lexical `..` cancellation, governs native symlinks.

WASI uses host preopen guest names as its filesystem namespace. Absolute paths select the longest matching absolute preopen on component boundaries; relative paths require an explicit `.` preopen. Do not guess the first descriptor or grant ambient host access. Ambiguous/unusable mappings fail explicitly, not by enumeration order. Once selected, preserve the remaining path's resolution semantics and pass it to that capability without lexical `..` cancellation; do not fall back to a broader capability after denial. The host still enforces traversal/rights. Missing namespace/operation support is Unsupported, denied authority is PermissionDenied, unacceptable encoding is InvalidInput. currentDir is Unsupported on WASI v0, not an invented absolute name for `.`. #28 must formalize mount-prefix/root spelling and invalid mapping diagnostics; this namespace intent is not a host implementation proof.

If a backend cannot faithfully perform requested exclusive creation, append, symlink creation, sync or usable namespace/private temporary creation, return Unsupported rather than weaken it. In particular, a WASI provider lacking a private-creation request/policy must not pretend ordinary path creation meets the temporary contract. Permission requests remain filtered by host policy, not universal ACL guarantees. No process spawning/watching, package manifest, migration/static-file framework or OS-wide authority API is added.

Cancellation remains outside Result. Paths, buffers, handles, callback/native state stay valid through completion or confirmed abortion; abort request, close request or moving work to a blocking pool is not release confirmation. Required cleanup can suspend indefinitely with cancellation deferred. Observe #33's deadline semantics when settled; racing a timer is not a hard return deadline. #30 gates actual scheduling, native close/retry behavior, Wasm pointer stability and exactly-once reclamation. Never blindly retry native close where descriptor reuse/platform semantics could close another resource.

## Evidence and follow-through

[Prototype 13](../../prototypes/13-filesystem.rill) includes pure lexical expectations and application workflows; its [case inventory](../prototypes/13-filesystem.md) supplies bounded copy/config/certificate/directory/temporary examples and independent rejection/partial/symlink/cancellation/WASI traces. All are hand-checked, not Rill or OS execution. Primary platform sources and their limits are linked in the inventory. #28 must specify ordinary family/result inference, dependency transport, path/component edge behavior, native namespace/error fidelity and exactly-once state transitions. #33/#36/#38/#39/#40 receive integration constraints without reopening their choices. Unworkable mechanisms return to map #1; closure is human design approval only.
