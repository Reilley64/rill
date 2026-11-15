"""Executable reference-model checks; NONE is Rill/native ABI validation."""
import copy
import hashlib
import json
import re
from pathlib import Path
import shutil
import struct
import subprocess
import unittest

from protocol_model import (HEADER, TOKEN, Job, QueueBudget, Token, Wake, check_schema,
                            decode_value, encode_value, frame, observation, unframe, unsigned)

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SCHEMA = json.loads((ROOT / "docs/sqlite-provider-binding-v1.schema.json").read_text())


def metadata_fixture():
    """Synthetic strings are intentionally NOT a usable build artifact."""
    p = SCHEMA["properties"]
    return {
        "artifactVersion": 1, "contract": "rill.sqlite.native/1",
        "module": {"canonicalIdentity": "MODEL-ONLY/module", "sourceSha3_256": "0" * 64,
                   "ownershipPolicy": "rill.sqlite.ownership/1"},
        "bindings": {k: "MODEL-ONLY/module#" + k for k in p["bindings"]["required"]},
        "resourceTypes": {k: "MODEL-ONLY/module#" + k for k in p["resourceTypes"]["required"]},
        "source": {k: v["const"] for k, v in p["source"]["properties"].items()},
        "target": {"os": "macos", "arch": "aarch64", "libc": "libSystem",
                   "deployment": "MODEL-ONLY", "compiler": "MODEL-ONLY", "linker": "MODEL-ONLY",
                   "compilerArguments": ["MODEL-ONLY"], "linkerArguments": ["MODEL-ONLY"]},
        "abi": {"id": "rill.sqlite.c/1", "wireVersion": 1, "pointerBits": 64,
                "callingConvention": "C", "headerSha3_256": hashlib.sha3_256((HERE / "contract.h").read_bytes()).hexdigest()},
        "profile": {k: copy.deepcopy(v["const"]) for k, v in p["profile"]["properties"].items()},
        "symbols": {"privatePrefix": "rill_sqlite_private_model_",
                    "exportedFunctions": p["symbols"]["properties"]["exportedFunctions"]["const"][:]},
        "hooks": copy.deepcopy(p["hooks"]["const"]),
        "budgets": dict(zip(p["budgets"]["required"], [4, 1, 64, 67108864, 64, 67108864,
                          1048576, 64, 4194304, 128, 16777216, 16777216, 67108864, 1048576, 4096, 32767])),
    }


class ProtocolModelTests(unittest.TestCase):
    def test_header_width_and_roundtrip(self):
        self.assertEqual(HEADER.size, 24)
        for kind in (1, 2, 3, 4):
            self.assertEqual(unframe(frame(kind, b"data")), (kind, b"data"))

    def test_bad_magic_version_kind_flags_reserved(self):
        good = frame(2, b"")
        for offset, byte in ((0, 0), (4, 2), (6, 5), (8, 1), (12, 1)):
            with self.subTest(offset=offset):
                bad = bytearray(good); bad[offset] = byte
                with self.assertRaises(ValueError): unframe(bytes(bad))

    def test_truncation_trailing_and_huge_length(self):
        for bad in (b"", frame(2, b"a")[:-1], frame(2, b"") + b"x",
                    HEADER.pack(b"RSQL", 1, 2, 0, 0, (1 << 64) - 1)):
            with self.assertRaises(ValueError): unframe(bad)

    def test_frame_budget_checked(self):
        self.assertEqual(len(frame(2, b"x", 25)), 25)
        with self.assertRaises(ValueError): frame(2, b"xx", 25)
        with self.assertRaises(ValueError): unframe(frame(2, b"x"), 24)

    def test_unsigned_no_bool_negative_overflow(self):
        for value in (True, -1, 1 << 64, 1.0):
            with self.assertRaises(ValueError): unsigned(value)

    def test_token_roundtrip_width(self):
        token = Token(1, 2, 3, 4)
        self.assertEqual(TOKEN.size, 32)
        self.assertEqual(Token.decode(token.encode()), token)

    def test_bad_token(self):
        for data in (bytes(32), bytes(31), TOKEN.pack(1, 2, 3, 99, 0), TOKEN.pack(1, 2, 3, 1, 1)):
            with self.assertRaises(ValueError): Token.decode(data)
        with self.assertRaises(ValueError): Token(1, 2, 1 << 64, 1).encode()

    def test_integer_extrema(self):
        for value in (-(1 << 63), (1 << 63) - 1):
            self.assertEqual(decode_value(encode_value(1, value)), (1, value))
        with self.assertRaises(ValueError): encode_value(1, 1 << 63)
        with self.assertRaises(ValueError): encode_value(1, True)

    def test_float_bits_and_nan_input(self):
        for value in (-0.0, 1.5, float("inf"), -float("inf")):
            decoded = decode_value(encode_value(2, value))[1]
            self.assertEqual(struct.pack("<d", decoded), struct.pack("<d", value))
        with self.assertRaises(ValueError): encode_value(2, float("nan"))

    def test_null_empty_blob_text_and_embedded_nul(self):
        for tag, value in ((0, None), (3, b""), (4, b""), (3, b"a\0b"), (4, b"\0")):
            self.assertEqual(decode_value(encode_value(tag, value)), (tag, value))

    def test_text_input_strict_output_raw(self):
        with self.assertRaises(UnicodeDecodeError): encode_value(3, b"\xff")
        raw = b"\x03" + struct.pack("<Q", 1) + b"\xff"
        self.assertEqual(decode_value(raw), (3, b"\xff"))
        with self.assertRaises(UnicodeDecodeError): decode_value(raw, input_text=True)

    def test_value_lengths_tags_and_limits(self):
        for bad in (b"", b"\x09", b"\x00x", b"\x01", b"\x04" + struct.pack("<Q", 100)):
            with self.assertRaises(ValueError): decode_value(bad)
        with self.assertRaises(ValueError): encode_value(4, b"ab", 1)
        with self.assertRaises(ValueError): decode_value(encode_value(4, b"ab"), 1)

    def test_schema_synthetic_fixture_only(self):
        check_schema(metadata_fixture(), SCHEMA)

    def test_schema_missing_unknown_and_version(self):
        for mutation in (lambda x: x.pop("module"), lambda x: x.update(shell="run"),
                         lambda x: x.update(artifactVersion=2), lambda x: x.update(artifactVersion=True)):
            value = metadata_fixture(); mutation(value)
            with self.assertRaises(ValueError): check_schema(value, SCHEMA)

    def test_schema_pin_mismatch(self):
        value = metadata_fixture(); value["source"]["version"] = "3.50.4"
        with self.assertRaises(ValueError): check_schema(value, SCHEMA)

    def test_schema_target_pair(self):
        value = metadata_fixture(); value["target"]["libc"] = "glibc"
        with self.assertRaises(ValueError): check_schema(value, SCHEMA)
        value["target"]["os"] = "linux"
        check_schema(value, SCHEMA)

    def test_schema_rejects_extra_native_authority(self):
        value = metadata_fixture(); value["symbols"]["exportedFunctions"].append("system")
        with self.assertRaises(ValueError): check_schema(value, SCHEMA)
        value = metadata_fixture(); value["module"]["shareableAnyType"] = True
        with self.assertRaises(ValueError): check_schema(value, SCHEMA)

    def test_schema_batch_defined_zero_not_absent(self):
        value = metadata_fixture(); value["profile"]["defines"]["SQLITE_ENABLE_BATCH_ATOMIC_WRITE"] = 0
        with self.assertRaises(ValueError): check_schema(value, SCHEMA)

    def test_schema_budget_bound_and_cache_zero(self):
        for invalid in (-1, 1 << 64, True):
            value = metadata_fixture(); value["budgets"]["ordinaryWorkers"] = invalid
            with self.assertRaises(ValueError): check_schema(value, SCHEMA)
        value = metadata_fixture(); value["budgets"]["cacheEntries"] = 0
        check_schema(value, SCHEMA)

    def test_model_queue_capacity_no_hidden_growth(self):
        queue = QueueBudget(2, 5)
        self.assertTrue(queue.admit(3)); self.assertFalse(queue.admit(3))
        self.assertTrue(queue.admit(2)); self.assertFalse(queue.admit(0))
        self.assertEqual(queue.used, 5)
        self.assertEqual(queue.take(), 3); self.assertTrue(queue.admit(1))

    def test_queued_cancel_prevents_claim(self):
        job = Job(1, offer=True)
        self.assertTrue(job.remove_queued())
        with self.assertRaises(ValueError): job.claim()
        job.release()  # No native creation offer exists if never dispatched.

    def test_claim_wins_remove_not_enough(self):
        job = Job(1); job.claim()
        self.assertFalse(job.remove_queued())
        with self.assertRaises(ValueError): job.release()
        job.complete(); job.release()

    def test_stale_interrupt_and_join(self):
        job = Job(2); job.claim()
        self.assertFalse(job.interrupt(1)); self.assertTrue(job.interrupt(2))
        job.complete()
        self.assertFalse(job.interrupt(2))
        with self.assertRaises(ValueError): job.release()
        job.join_interrupt(); job.release()

    def test_adoption_needs_registered_guard(self):
        job = Job(1, offer=True); job.claim(); job.complete()
        with self.assertRaises(ValueError): job.adopt(True)
        job.register_owner()
        with self.assertRaises(ValueError): job.adopt(False)
        job.adopt(True); job.release()

    def test_no_release_unadopted_offer(self):
        job = Job(1, offer=True); job.claim(); job.complete()
        with self.assertRaises(ValueError): job.release()
        job.dispose_offer(); job.release()

    def test_no_duplicate_registration_adoption_or_release(self):
        job = Job(1, offer=True); job.claim(); job.complete(); job.register_owner()
        with self.assertRaises(ValueError): job.register_owner()
        job.adopt(True)
        with self.assertRaises(ValueError): job.adopt(True)
        job.release()
        with self.assertRaises(ValueError): job.release()

    def test_wake_ready_survives_early_and_duplicate_notification(self):
        wake = Wake(1); wake.notify(1); wake.notify(1)
        self.assertTrue(wake.ready)

    def test_wake_revoked_or_recycled(self):
        wake = Wake(1); wake.revoke(); wake.notify(1)
        self.assertFalse(wake.ready)
        replacement = Wake(2); replacement.notify(1)
        self.assertFalse(replacement.ready)

    def test_whole_query_not_latest_job_observation(self):
        self.assertEqual(observation(True, False), "MayHaveExecuted")
        self.assertEqual(observation(False, True), "MayHaveExecuted")
        self.assertEqual(observation(False, False), "NotSubmitted")

    def test_schema_export_set_matches_declarations(self):
        declared = re.findall(r"uint32_t (rill_sqlite_v1_\w+)\(", (HERE / "contract.h").read_text())
        self.assertEqual(declared, metadata_fixture()["symbols"]["exportedFunctions"])

    def test_schema_requires_binding_roles_and_argument_vectors(self):
        value = metadata_fixture(); value["bindings"].pop("adopt")
        with self.assertRaises(ValueError): check_schema(value, SCHEMA)
        value = metadata_fixture(); value["target"]["compilerArguments"] = "shell command"
        with self.assertRaises(ValueError): check_schema(value, SCHEMA)

    def test_c_header_declarations_only(self):
        cc = shutil.which("cc")
        if not cc:
            self.skipTest("no C compiler; declaration syntax untested")
        subprocess.run([cc, "-std=c11", "-Werror", "-fsyntax-only", "-x", "c", "-include",
                        str(HERE / "contract.h"), "-"], input=b"int main(void) { return 0; }\n",
                       check=True, timeout=30, capture_output=True)
        # No linking, ABI execution, definitions or product implementation tested.


if __name__ == "__main__":
    unittest.main()
