# Actual execution results

```text
CONFIG OpenSSL=OpenSSL 3.5.9 29 Sep 2026 partial_write=on moving_buffer=off auto_retry=off max_fragment=16384 pipelines=1 outbound_padding=none app_attempt_max=16384 finite_pump=20000
LEDGER hypothesis_only plaintext_out=32+16+16KiB plaintext_in=16+16+16+16KiB ciphertext_out=16+16+16+16KiB ciphertext_in=16+16+16+16KiB synchronous_C_does_not_model_async_or_managed_native_copy_overlap=1
RESULT renegotiation_unsafe_continue PASS event=1 warning_alert_callback=1 wbio_after_read=BIO pair buffered_after_read=0 prior_cipher=256 later_cipher=30 prior_delivered=1 local_fresh_write=success peer_record_failure=1
RESULT renegotiation_explicit_stop PASS event=1 warning_alert_buffered=1 classified=WANT_READ stop=1 fresh_write=avoided occupied_before=256
RESULT basic_tls12_c2s PASS fragmented=17 staged=12000 native_success=12000 cipher_handoff=12275 delivery=12275
RESULT basic_tls12_s2c PASS fragmented=23 staged=12000 native_success=12000 cipher_handoff=12730 delivery=12730
RESULT basic_tls13_c2s PASS fragmented=19 staged=12000 native_success=12000 cipher_handoff=13547 delivery=13547
RESULT basic_tls13_s2c PASS fragmented=29 staged=12000 native_success=12000 cipher_handoff=13873 delivery=13873
RESULT same_args_retry PASS ring=16384 record_wire=16413 want_write=1 ptr_preserved=1 len_preserved=1 native_success=16384
RESULT partial_success_suffix PASS first_success=4096 fresh_suffix=4096 failure_prefix_inferred=0
RESULT bidirectional_stall_resume PASS cwrite=8000 swrite=7000 lower_stalled_then_resumed=1 separate_saved_wants=1
RESULT keyupdate_not_requested PASS pending_read=WANT_READ fresh_write=success blocked_control=0
RESULT keyupdate_requested PASS pending_read=WANT_READ fresh_write=success blocked_control=0
RESULT keyupdate_requested_blocked PASS pending_read=WANT_READ fresh_write=success blocked_control=0
RESULT close_eof PASS close_notify=ZERO_RETURN truncation=SSL_ERROR retained_obligations=none quiescent_release=after_join
SUMMARY pass=13 total=13 fail=0 scope=standalone_in_memory_reference_not_Rill_not_TCP
```

Exit code: `0`. See `README.md` for scope, findings, modeled partitions, and gaps.

## Renegotiation finding

The earlier source-derived prediction of immediate `BAD_WRITE_RETRY` was falsified: the fresh write succeeds locally. The strengthened fixture preserves the prior ciphertext and then transfers every ciphertext byte made available after that write. The peer consumes the prior application record but fails record decryption before receiving the fresh byte; the warning-alert callback had fired, yet no warning-alert ciphertext was handed off and the sender record sequence had advanced. The explicit-stop fixture avoids issuing that write. This finite in-memory result supports the selected stop policy for the detected TLS 1.2 renegotiation path; it is not a complete provider/runtime/security proof.
