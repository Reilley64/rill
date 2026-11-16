#!/usr/bin/env python3
"""Review-only author data/models; NOT a Rill parser/runtime or TLS/security test."""

from collections import Counter
from dataclasses import dataclass
from pathlib import Path
import re


CLAIMS = Counter()


def claim(group, condition):
    """Count only evaluated reference comparisons, not executed Rill expects."""
    if not condition:
        raise AssertionError(f"reference comparison failed: {group}")
    CLAIMS[group] += 1


def dns_reference(text):
    """ASCII author model: no resolver, IP parser, IDNA or SAN matcher."""
    if not text.isascii():
        return None
    name = text[:-1] if text.endswith(".") else text
    if not 1 <= len(name) <= 253:
        return None
    labels = name.split(".")
    if any(not 1 <= len(label) <= 63
           or label.startswith("-") or label.endswith("-")
           or re.fullmatch(r"[A-Za-z0-9-]+", label) is None
           for label in labels):
        return None
    numeric = r"(?:[0-9]+|0[xX][0-9A-Fa-f]*)"
    if all(re.fullmatch(numeric, label) is not None for label in labels):
        return None
    return name.lower()


def alpn_reference(data):
    """A length-only opaque-byte admission model, not an ALPN negotiator."""
    return data if 1 <= len(data) <= 255 else None


def certificate_window(now, not_before, not_after):
    return not_before <= now < not_after


def max_age_seconds(nanoseconds):
    if nanoseconds <= 0 or nanoseconds % 1_000_000_000 != 0:
        return None
    return nanoseconds // 1_000_000_000


def crl_window(now, this_update, next_update, max_age):
    return (max_age > 0 and this_update < next_update
            and this_update <= now < min(next_update, this_update + max_age))


@dataclass
class TailTransfer:
    """Toy bookkeeping only: integer tail and one Python identity token.

    No bytes are authenticated here, no real error is constructed, no native
    completion/release or complete TLS/control/closed-state machine is modeled.
    """

    tail: int
    pending_original: object

    def surface_original_elsewhere(self):
        original = self.pending_original
        self.pending_original = None
        return original

    def read(self, maximum, control_rejected=False):
        if control_rejected:
            return "control", 0, None
        if maximum == 0:
            return "zero", 0, None
        count = min(maximum, self.tail)
        self.tail -= count
        if self.tail:
            return "prefix", count, None
        if self.pending_original is not None:
            original = self.pending_original
            self.pending_original = None
            return "original", count, original
        return "already-failed", count, None


def check_dns():
    group = "DNS ASCII author cases"
    accepted = [
        ("ExAmPlE.TeSt", "example.test"),
        ("ExAmPlE.TeSt.", "example.test"),
        ("LOCALHOST", "localhost"),
        ("123.Example", "123.example"),
        ("XN--A.Example.", "xn--a.example"),
        ("0xg", "0xg"),  # Not all numeric-looking; no IDNA assertion.
        ("a-b.test", "a-b.test"),
        ("a" * 63, "a" * 63),
    ]
    boundary = ".".join(("a" * 63, "b" * 63, "c" * 63, "d" * 61))
    accepted.extend(((boundary, boundary), (boundary + ".", boundary)))
    for text, expected in accepted:
        claim(group, dns_reference(text) == expected)
    rejected = [
        "", ".", "example\x00.test", "_service.example", "*.example.test",
        "caf\u00e9.example", "\uff11.example", "example.test..", ".example.test",
        "example..test", "-example.test", "example-.test", "example.-test",
        " example.test", "example.test\t", "example.test:443", "[::1]",
        "123", "127.1", "999.999.999.999", "1.2.3.4.5", "0x", "0Xf.0x.010.",
        "2130706433", "0177.0.0.1", "0x7f.0.0.1", "a" * 64,
        boundary + "d", boundary + "d.",
    ]
    for text in rejected:
        claim(group, dns_reference(text) is None)
    claim(group, len(boundary) == 253)
    claim(group, len(boundary + ".") == 254)


def check_alpn():
    group = "ALPN byte/size author cases"
    for data in (b"\x00", b"\xff\x00\x80", bytes([255]) * 255, b"http/1.1"):
        claim(group, alpn_reference(data) == data)
    for data in (b"", bytes(256)):
        claim(group, alpn_reference(data) is None)
    claim(group, tuple(b"http/1.1") == (104, 116, 116, 112, 47, 49, 46, 49))
    claim(group, b"H2" != b"h2")
    claim(group, (bytes([255]) * 255)[254] == 255)
    # Pure wire-budget arithmetic; no list uniqueness or negotiation is proved.
    claim(group, 255 * (255 + 1) + (254 + 1) == 65_535)
    claim(group, 256 * (255 + 1) == 65_536)


def check_time():
    group = "Time floor/window author cases"
    for nanos, seconds in (
        (-1_000_000_001, -2), (-1_000_000_000, -1), (-1, -1), (0, 0),
        (999_999_999, 0), (1_000_000_000, 1),
    ):
        claim(group, nanos // 1_000_000_000 == seconds)
    for now, expected in ((9, False), (10, True), (19, True), (20, False)):
        claim(group, certificate_window(now, 10, 20) == expected)
    for nanos, expected in (
        (-1_000_000_000, None), (0, None), (1, None), (1_500_000_000, None),
        (1_000_000_000, 1), (60_000_000_000, 60),
    ):
        claim(group, max_age_seconds(nanos) == expected)
    for now, expected in ((9, False), (10, True), (14, True), (15, False), (20, False)):
        claim(group, crl_window(now, 10, 20, 5) == expected)
    claim(group, crl_window(19, 10, 20, 100))
    claim(group, not crl_window(20, 10, 20, 100))
    claim(group, not crl_window(10, 10, 10, 5))
    claim(group, not crl_window(10, 10, 9, 5))
    claim(group, not crl_window(10, 10, 20, 0))
    claim(group, crl_window(-3, -5, 0, 3))
    claim(group, not crl_window(-2, -5, 0, 3))
    # Python arbitrary precision is not native/time_t representability evidence.


def check_budgets():
    group = "Numerical budget author cases"
    kib = 1024
    mib = kib * kib
    claim(group, 2 * (16 * kib) + 4096 == 36_864)
    claim(group, 2 * (64 * kib) + 4096 == 135_168)
    claim(group, 2 * (128 * kib) + 4096 == 266_240)
    claim(group, 2 * (16 * mib) + 4096 == 33_558_528)
    claim(group, 2 * (64 * mib) + 4096 == 134_221_824)
    claim(group, 4 * (64 * kib) == 256 * kib)  # Logical queues only.
    claim(group, 64 * kib <= 64 * kib)  # Inclusive prefix ceiling.
    claim(group, 64 * kib + 1 > 64 * kib)
    claim(group, 16 * mib * 1024 > 64 * mib)  # Aggregate CRL cap binds separately.
    claim(group, 100_000 * 1024 > 1_000_000)
    claim(group, (2 ** 20 - 7) + 7 == 2 ** 20)
    claim(group, (2 ** 20 - 7) + 8 > 2 ** 20)
    claim(group, 1023 + 1 == 1024)
    claim(group, 1024 + 1 > 1024)
    # No epoch reset, transport-chunk equivalence or native allocation bound inferred.


def check_error_transfer():
    group = "Toy tail/unique-error transfer cases"
    original = object()  # Python identity marker, NOT a fabricated Rill TlsError.
    pending = TailTransfer(7, original)
    claim(group, pending.read(3) == ("prefix", 3, None))
    claim(group, pending.read(4, control_rejected=True) == ("control", 0, None))
    claim(group, pending.tail == 4 and pending.pending_original is original)
    claim(group, pending.read(0) == ("zero", 0, None))
    claim(group, pending.pending_original is original)
    result = pending.read(4)
    claim(group, result[0:2] == ("original", 4) and result[2] is original)
    claim(group, pending.pending_original is None and pending.tail == 0)
    claim(group, pending.read(1) == ("already-failed", 0, None))
    surfaced = TailTransfer(5, original)
    claim(group, surfaced.surface_original_elsewhere() is original)
    claim(group, surfaced.pending_original is None)
    claim(group, surfaced.read(2) == ("prefix", 2, None))
    claim(group, surfaced.read(3, control_rejected=True) == ("control", 0, None))
    claim(group, surfaced.tail == 3)
    claim(group, surfaced.read(3) == ("already-failed", 3, None))
    claim(group, surfaced.read(1) == ("already-failed", 0, None))


def main():
    source = Path(__file__).with_name("values.rill").read_text(encoding="utf-8")
    # Deliberately textual, not a lexer/parser or reachability/purity analysis.
    declared = len(re.findall(r"^expect\s+[^\n]+;", source, re.MULTILINE))
    claim("Structural author check", declared == 32)
    check_dns()
    check_alpn()
    check_time()
    check_budgets()
    check_error_transfer()
    print(f"Declared intended-valid Rill expects (text count): {declared}; executed: 0")
    for group, count in CLAIMS.items():
        print(f"{group}: {count} reference comparisons passed")
    print(f"Total Python review comparisons: {sum(CLAIMS.values())}")
    print("Author data/models only; NOT a Rill parser/runtime, TLS/provider, crypto, IDNA or DER test.")
    print("Rill execution, native integration, TLS handshakes and security validation: ALL NOT RUN.")


if __name__ == "__main__":
    main()
