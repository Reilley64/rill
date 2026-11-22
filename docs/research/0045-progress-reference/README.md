# ri-w0c Q33–Q35 standalone OpenSSL progress reference

This directory is a **throwaway native reference experiment**, not production Rill, a provider-delivery proof, an API freeze, an entitlement claim, or approval of the candidate v0 budget. It exercises the supplied OpenSSL 3.5.9 source artifact named for commit `45e844fa2a14ec92d146bd8f5778ac130b6625fb` entirely in memory. It does not use TCP, Rill jobs, cancellation, leases, managed/native wake machinery, or an external network target. ri-dej and ri-ei7 remain open.

Run the complete private copy/configure/build/certificate/compile/test sequence with one command:

```sh
python3 docs/research/0045-progress-reference/run_reference.py
```

The runner refuses an archive SHA-256 mismatch, checks `VERSION.dat`, deletes and recreates `/tmp/0045-progress-reference-private`, and copies the supplied extraction before configuring. It never configures or builds `/tmp/0045-source/openssl-45e844fa2a14ec92d146bd8f5778ac130b6625fb`. No install or system change is performed. Build output and the ephemeral P-256 key/certificate remain under `/tmp`.

## Selected settings and fixture trust

- OpenSSL: 3.5.9 supplied source archive, expected SHA-256 in `run_reference.py`; exact build/version output and commands are in `manifest.json`. The extraction has no `.git`, so the commit cannot be independently recomputed from Git metadata; the archive hash and commit-bearing extraction name are reported rather than overstated.
- Private minimal reference build: `no-shared no-tests no-asm no-dso no-engine no-async -O2`, `make -j4 build_sw`. This is not the ri-irw final build, namespace, patch set, or bridge.
- TLS: single serialized owner, one logical read and one logical write at most; TLS startup completes before application I/O. Initial and final handshake ciphertext is explicitly transferred before publication.
- Modes: partial write on from creation; moving write buffer off; AUTO_RETRY off. Max send fragment 16 KiB; native application attempt at most 16 KiB; one default pipeline; no configured outbound padding.
- The runner creates a one-day ECDSA P-256 self-signed certificate with `subjectAltName=DNS:localhost`. The client trusts only that fixture certificate, verifies the chain and hostname `localhost`; the server uses it. Client authentication is deliberately out of scope (`SSL_VERIFY_NONE` on the server). Keys are ephemeral `/tmp` fixture material.
- Every SSL result is classified immediately by `SSL_get_error` on the same thread after clearing the error queue and before BIO inspection. Each endpoint saves its own last result/WANT and continuation pointer/length. The message callback only observes post-handshake TLS 1.2 `HelloRequest`/renegotiating `ClientHello`; it calls no OpenSSL API and touches no internals.
- Pump loops are finite (`MAX_PUMP=20000`). A stall or unsupported state fails and prints endpoint state; the harness does not add unbounded queues, discard input to manufacture progress, relax caps, or silently relabel a failure.

## Executed fixtures

1. **TLS 1.2 rejected-renegotiation adversary, continuation attempted.** A completed 227-byte client application write under `ECDHE-ECDSA-AES128-GCM-SHA256` fills a 256-byte ring (227 + 29). There is no active application-write WANT. The server fixture alone temporarily permits local renegotiation to emit the test `HelloRequest`; this is an explicit peer-fixture exception, not candidate policy. The client remains `SSL_OP_NO_RENEGOTIATION`. After the read returns WANT_READ and the warning-alert callback fires, the fixture preserves and drains the prior ciphertext, issues a fresh one-byte write, then delivers all available ciphertext in original observed order. The local write succeeds, but only a 30-byte fresh application record follows: the peer fails record decryption because the missing warning-alert ciphertext advanced sender record state. A pass means this unsafe continuation consequence is reproduced—not that continuation is supported.
2. **Same adversary, explicit-stop policy.** After immediate read classification and before the first BIO query/other OpenSSL call, the callback flag selects terminal stop. The fixture verifies that the unsafe fresh write is not issued. It does **not** claim that the warning alert was retired or delivered. No `SSL_shutdown` is called on poison.
3. TLS 1.2 and TLS 1.3 in both application directions, with ciphertext transferred in small fragments.
4. A 16 KiB TLS 1.2 write whose 16,413-byte record exceeds a 16 KiB BIO: WANT_WRITE, external drain, and retry with exactly the same pointer and length. No other SSL I/O occurs during write-WANT_WRITE.
5. Positive partial success with a 4 KiB configured fragment inside a <=16 KiB attempt. A new native suffix begins only after positive success. No prefix is inferred from a zero/WANT, ciphertext length, or private `wnum`.
6. Bidirectional staged commands with a deliberate lower-receiver stall, then bounded fragmented resume and delivery.
7. TLS 1.3 requested and non-requested KeyUpdate with a pending application read; fresh write is attempted only when established (`SSL_in_init` false), no application write is pending, and the passive renegotiation-event invariant is clear. A tiny-ring requested-KU case tries to block output and reports whether it actually observed WANT_WRITE.
8. `close_notify` versus raw lower EOF/truncation, followed by quiescent disposal only after no retained harness continuation remains.

The **tiny-ring KeyUpdate test is stress outside the candidate v0 16 KiB ring partition**. It is kept separate in output and does not amend the ledger.

## Accounting modeled by this synchronous C harness

The candidate numeric ledger remains **unapproved**. The harness names and tracks command staging acceptance, positive native write success, ciphertext handoff, and peer-side ciphertext delivery separately. It never converts ciphertext length or failed native output into a plaintext prefix.

What is concretely exercised:

- fixed BIO capacity (16 KiB in candidate fixtures, 256 bytes for the source counterexample, and 64 bytes only for separate KU stress);
- stable native retry pointer/length for one <=16 KiB native write;
- bounded copied transfer spans on the C stack;
- synchronous ring occupancy, drain, feed, and lower-receiver stall/resume;
- private read destination lifetime for each synchronous retry loop;
- no more than one active logical read/write continuation per endpoint.

What is **not** physically modeled:

- the proposed 32 KiB admitted plaintext-out allocation as a real managed queue;
- four simultaneously allocated 64 KiB ledgers or strict credit enforcement among every named partition;
- retained asynchronous lower-job payloads, drain/tail ownership after return, managed-to-native copies, movable managed memory, worker migration, copy overlap, leases, wake generations, cancellation, or joined release;
- OpenSSL's opaque provider allocations as part of the four staging budgets.

Thus this run tests selected state transitions and candidate fixed-buffer behavior; it does not prove the ledger equations sufficient in Rill.

## Known objections, gaps, and deliberately untested cases

- Received TLS 1.3 ticket discard is not implemented or proven. Ticket issuance is disabled in the fixture, but stock received-ticket storage/profile enforcement is still a ri-ei7/provider gap.
- Mandatory key-use limits and profile hooks are not implemented. KeyUpdate fixtures exercise stock requested/non-requested transitions only.
- The warning-alert stop policy avoids the executed record-desynchronization path; it does not prove alert retirement/delivery, cover every blocked control state, or by itself establish the production callback/private-shim mechanism. The earlier source-derived prediction of immediate `BAD_WRITE_RETRY` was falsified: the local fresh write succeeds before the peer later rejects the record sequence.
- The callback observes only fixed TLS 1.2 renegotiation message types. It is a candidate observation mechanism, not a frozen public/private API or complete future-provider guarantee.
- No malformed-message matrix, fatal-alert-output bound, post-handshake authentication, resumption, early data, compression, engines, async OpenSSL mode, padding callback, multi-pipeline record method, TLS 1.2 peer-initiated server-side ClientHello variant, cancellation race, or lower-job completion race is exercised.
- Clean TLS shutdown here is generic reference behavior, not proof of Rill's version-specific shutdown admission/flush contract. Raw EOF is only distinguished from authenticated close.
- A synchronous `SSL_*` call still has no hard CPU/time bound. Finite pump iterations bound harness retries, not provider call latency.
- In-memory BIO transfer is not TCP and proves no kernel/socket backpressure behavior.
- Public APIs and candidate numeric partitions remain unfrozen/unapproved. Failures in `results.md` are findings, not permission to weaken policy or caps.
