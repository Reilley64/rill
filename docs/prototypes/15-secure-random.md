# Secure random byte case inventory

Read the source examples in [`prototypes/15-secure-random.rill`](../../prototypes/15-secure-random.rill).

Design evidence for [#37](https://github.com/Reilley64/rill/issues/37), [ADR 0020](../adr/0020-secure-random-byte-generation.md) and the [public contract](../standard-library-contracts.md#13-secure-random-bytes). The companion has eight intended-valid expectations. Reviewers checked them by hand, without compiling or executing them. They use empty standard calls and explicitly chosen application fakes. They do not replace std::random with a deterministic provider. No native syscall/framework/WASI provider, scheduler integration, cryptographic generator, private production-provider override or compiler is implemented here.

## Source examples and non-statistical claims

- Cases call randomBytes(0), fill an empty Bytes owner and reuse that owner after completion. They also pass the standard generation operation as an ordinary function value and forward an empty fill through an exact-error helper.
- Application consumers take explicit dependencies with callableOnce evidence and an application-chosen noBorrow(E) restriction. They forward Result errors. That fixture restriction is not a new generic standard entropy API. Fake bytes and error types belong to the application. #41 implement syntax extends the application's canonical display implementation, not std::random's provider.
- A fake call intentionally returns known bytes; another explicitly selected fake modifies one byte then fails. Those cases check the application's wiring and permitted failure handling, not a real provider, native retention or security guarantee.
- liveRequest and liveFillLength show nonempty production call shapes. The expectations do not automatically run them with a nonempty request. Error observation uses kind/nativeCode, never display parsing or private RandomError field construction.

The empty examples alone cannot prove the provider was not called. That needs a private call-count harness. Disposal/reuse examples are not a borrow-checker/native lifetime proof. No assertion expects real bytes to equal a constant, differ across calls, or be nonzero. Statistically plausible output cannot prove a host honest or a generator cryptographically secure.

## Private adapter/reference traces

These are required future harness cases, not a public Rill mock-provider API. Isolate controlled synthetic output and status in test-only linkage/runtime fixtures. Keep those fixtures structurally separate from supported production provider configuration. Do not infer production randomness from the trace bytes.

| Controlled case | Required outcome |
| --- | --- |
| Owned length zero; provider configured unavailable | Empty success; provider-call count zero. |
| Existing destination length zero | Success; no provider initialization, call, resize or append. |
| Linux requested 5, count 2, EINTR, count 3 | Advance only by confirmed counts and retry the remaining extent. Report success after all 5. Check normal cancellation and fairness between attempts. |
| Nonempty Linux call returns zero | ProviderFailed, no successful short fill or retry spin. |
| Linux missing syscall or securely classified access denial | Unavailable or PermissionDenied as appropriate, optional actual code, no device/PRNG/test fallback. |
| Other Linux reported error | ProviderFailed unless a reliable portable category applies. |
| macOS success statuses over several bounded chunks | Complete the total request with checked size_t lengths. |
| macOS failed status after earlier chunks | Error, no partial owned output. In-place contents may be modified; no rollback/prefix guarantee. |
| WASI successful statuses | Complete each requested chunk under a documented secure host configuration, with checked u32 lengths/offsets. |
| WASI error status or host-specific call limit | Map reported errors honestly; no fake short success, silent truncation or universal host-limit assumption. |
| Missing Wasm import/instantiation failure or fatal host trap | Outside the recoverable RandomError promise. A zero fast path cannot instantiate a missing-import module. |
| Error status after the provider changed part/all of its extent | Retain valid initialized Bytes/unchanged logical length; do not claim a usable generated prefix. |
| Successful owned generation | Publish exactly the requested initialized extent only after full success. |
| Failed/cancelled owned generation | No partial owner escapes; clean up once after actual provider access ends. |
| Cancellation arrives before/between calls | Follow existing delivery rules; no new Result cancellation variant. |
| Cancellation/timeout arrives inside a blocked native call | Keep owner and exclusive access, wait for completion/confirmed abort; no forced release or hard return bound. |
| Unexpected out-of-bounds count or invalid retained pointer | This is a backend/provider invariant or safety failure, not a promised recoverable memory-corruption error. Validate before arithmetic or use wherever possible. |
| Native status fits signed i64 | Preserve diagnostically without reinterpretation/wrapping; not a portable branch protocol. |

## Independent source rejection cases

1. Pass a read-only Bytes view to fillRandom, obtain a mutable text/subrange view, or assume the operation appends/resizes/fills spare capacity. Its input is exclusive access to a whole existing owner.
2. Mutate/move/dispose/alias the destination while an active operation or required native cleanup still retains exclusive access. A cancellation request is not confirmation the native writer stopped.
3. Treat Result as Bytes/count, omit required Result handling, implicitly convert RandomError to another error type, or interpret failure as permission to use a certified random prefix.
4. Construct/rebuild RandomError private fields, parse human display as the machine protocol or assume native error codes have platform-independent values.
5. Refer to an invented seed/setProvider/global test override, GRND_INSECURE option, device-file fallback, ordinary PRNG or UUID/range/shuffle helper as though this ticket had supplied it.
6. Assume rill test replaces secure generation the way std::env intentionally supplies empty inputs. The companion's known outputs require an explicitly passed application fake; canonical aliases/local shadowing do not register a new secure provider.
7. Use a conflicting family body, trait/public dyn/erased interface or imported alias to bypass existing identity/orphan/duplicate/callable rules. No new user-implementable entropy family is declared.
8. Promise lifetime/property evidence for a borrowed destination merely because RandomError is noBorrow/sendable/shareable. Error payload evidence does not release the caller's active exclusive operation.

## Provider, scheduling and host acceptance inventory

- Linux uses getrandom flags 0 and no compatibility fallback. Validate availability/error translation, initialization waits, EINTR/short counts, zero progress, checked chunk offsets and no insecure flag. Do not promote the manual's historical 32MiB-era cap into a current universal limit.
- macOS uses SecRandomCopyBytes with kSecRandomDefault, explicit system-framework linkage and checked status. Establish actual target availability/linking; do not substitute getentropy's 256-byte cap or arc4random_buf's void return contract for this provider's API.
- Validate concurrent calls and any provider-internal synchronization. There is no public mutable generator object, hidden application capture, independent-stream promise or uniqueness promise. Potentially blocking native work must not silently run on an ordinary M:N worker. Bound and fairly schedule work using the existing blocking facility. Usually-fast calls do not prove that work is nonblocking.
- WASI preview1 says high-quality data and permits blocking. Record supported host versions/configuration and cryptographic provider provenance; deterministic/weak host services are not production secure configurations. A dishonest host cannot be detected by guest sampling. Do not claim every WASI engine, or every configurable Wasmtime provider, meets the security contract.
- Validate host-compatible chunk sizes against actual ABI and call caps. No fixed portable total request cap is added. Public u64 counts do not permit wrapping or narrowing to size_t/u32 or exceeding target addressability. Existing Bytes capacity or allocation failure is fatal. Application-facing examples must bound hostile size inputs themselves.
- Synchronous Wasm imports may stall the single-thread scheduler. Reentrancy, suspension, offload and memory.grow can invalidate retained host pointers despite owner retention. Demonstrate safe staging, copy-back or pointer coordination, or leave the provider/backend mechanism unvalidated. #30's gates remain. Calling a mechanism async or a blocking pool does not prove it safe.
- Cancellation/stop/race must preserve every backing owner, initialized byte extent, native registration and exclusive restriction through actual completion/confirmed abort. Exactly-once partial allocation/cleanup must not expose partially completed owned output. A request may block indefinitely; no force kill/rollback/fatal-panic cleanup guarantee is inferred.
- Native/host errors and standard error/cause display must not retain/dump random bytes, original destination contents, hidden staging buffers or keys. Optional numeric codes are safe diagnostic metadata, not arbitrary provider text containing input. No secure erase/locked memory is promised for ordinary Bytes.
- Use test-only faults for call counts, partial progress/mutation, interrupted retries, zero progress, unavailable/denied/error statuses, boundary sizes and cancellation races. Production rill test calls remain real. Separate source/static/reference checks, unavailable execution, actual host runs and security review in the resulting evidence; no chi-square or duplicate test certifies security.
- TLS #38 must explicitly integrate its provider's entropy requirements and host trust, not assume replacing a library entropy hook is approved or that std::random alone proves TLS safe. #36's empty test environment does not seed or configure random generation. #33 retains any deadline API; hard timeout completion is not promised here.

#28 specifies checking, source-origin/error mapping, progress, checked arithmetic, secret-safe diagnostics and cleanup. #30 retains existing native/Wasm backend validation. Supported deployment still requires evidence about the entropy provider, linking and trust. If a necessary provider mechanism or checking rule is inconsistent, return it to the map. This case inventory cannot establish that it is secure or implemented.
