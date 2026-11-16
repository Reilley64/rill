"""#43 executable REFERENCE MODEL, not a C shim/Rill runtime or full codec.

Only primitive envelopes/tokens/values, the schema's used subset, and selected
sequential state transitions are modeled. No concurrency/ABI/ownership proof.
"""
import json
import math
import re
import struct
from dataclasses import dataclass

HEADER = struct.Struct("<4sHHIIQ")
TOKEN = struct.Struct("<QQQII")
MAX_FRAME = 64 * 1024 * 1024
U64_MAX = (1 << 64) - 1


def unsigned(value, maximum=U64_MAX):
    if type(value) is not int or not 0 <= value <= maximum:
        raise ValueError("unsigned bound")
    return value


def frame(kind, body, maximum=MAX_FRAME):
    if type(kind) is not int or kind not in (1, 2, 3, 4) or type(body) is not bytes:
        raise ValueError("frame kind/body")
    if len(body) > unsigned(maximum) - HEADER.size:
        raise ValueError("frame limit")
    return HEADER.pack(b"RSQL", 1, kind, 0, 0, len(body)) + body


def unframe(data, maximum=MAX_FRAME):
    if len(data) < HEADER.size or len(data) > unsigned(maximum):
        raise ValueError("frame bounds")
    magic, version, kind, flags, reserved, size = HEADER.unpack_from(data)
    if magic != b"RSQL" or version != 1 or kind not in (1, 2, 3, 4) or flags or reserved:
        raise ValueError("frame header")
    if size != len(data) - HEADER.size:
        raise ValueError("frame length")
    return kind, data[HEADER.size:]


@dataclass(frozen=True)
class Token:
    service: int
    slot: int
    generation: int
    kind: int

    def encode(self):
        for value in (self.service, self.slot, self.generation):
            if unsigned(value) == 0:
                raise ValueError("absent live token")
        if type(self.kind) is not int or self.kind not in (1, 2, 3, 4):
            raise ValueError("token kind")
        return TOKEN.pack(self.service, self.slot, self.generation, self.kind, 0)

    @classmethod
    def decode(cls, data):
        if len(data) != TOKEN.size:
            raise ValueError("token length")
        service, slot, generation, kind, reserved = TOKEN.unpack(data)
        if reserved:
            raise ValueError("token reserved")
        token = cls(service, slot, generation, kind)
        token.encode()
        return token


def encode_value(tag, value=None, maximum=16 * 1024 * 1024):
    if type(tag) is not int:
        raise ValueError("value tag")
    if tag == 0 and value is None:
        return b"\x00"
    if tag == 1 and type(value) is int and -(1 << 63) <= value < (1 << 63):
        return b"\x01" + struct.pack("<q", value)
    if tag == 2 and type(value) is float and not math.isnan(value):
        return b"\x02" + struct.pack("<d", value)
    if tag in (3, 4) and type(value) is bytes and len(value) <= unsigned(maximum):
        if tag == 3:
            value.decode("utf-8", errors="strict")  # Input text, not replacement.
        return bytes([tag]) + struct.pack("<Q", len(value)) + value
    raise ValueError("unsupported input value")


def decode_value(data, maximum=16 * 1024 * 1024, input_text=False):
    """Decode ONE native cell; output TEXT remains raw for later checked decode."""
    if not data:
        raise ValueError("missing value")
    tag = data[0]
    if tag == 0 and len(data) == 1:
        return tag, None
    if tag in (1, 2) and len(data) == 9:
        return tag, struct.unpack("<q" if tag == 1 else "<d", data[1:])[0]
    if tag in (3, 4) and len(data) >= 9:
        size = struct.unpack_from("<Q", data, 1)[0]
        if size > unsigned(maximum) or size != len(data) - 9:
            raise ValueError("value length")
        raw = data[9:]
        if tag == 3 and input_text:
            raw.decode("utf-8", errors="strict")
        return tag, raw
    raise ValueError("value encoding")


def check_schema(value, schema, root=None):
    """Only the keywords in this repository schema, not a general JSON Schema engine.

    Model integers are deliberately Python ints (never bool/float). No remote refs.
    Passing this check is NOT compiler/build admission or proof of native identity.
    """
    root = schema if root is None else root
    if "$ref" in schema:
        prefix = "#/$defs/"
        if not schema["$ref"].startswith(prefix):
            raise ValueError("nonlocal schema ref")
        check_schema(value, root["$defs"][schema["$ref"][len(prefix):]], root)
    if "const" in schema and json.dumps(value, sort_keys=True) != json.dumps(schema["const"], sort_keys=True):
        raise ValueError("constant")
    if "enum" in schema and not any(type(value) is type(v) and value == v for v in schema["enum"]):
        raise ValueError("enum")
    types = {"object": dict, "string": str, "integer": int, "array": list}
    if "type" in schema and type(value) is not types[schema["type"]]:
        raise ValueError("type")
    if isinstance(value, dict):
        properties = schema.get("properties", {})
        if any(key not in value for key in schema.get("required", [])):
            raise ValueError("required")
        if schema.get("additionalProperties") is False and value.keys() - properties.keys():
            raise ValueError("unknown property")
        for key in value.keys() & properties.keys():
            check_schema(value[key], properties[key], root)
    if isinstance(value, list):
        if len(value) < schema.get("minItems", 0):
            raise ValueError("array length")
        if "items" in schema:
            for item in value:
                check_schema(item, schema["items"], root)
    if isinstance(value, str):
        if len(value) < schema.get("minLength", 0):
            raise ValueError("string length")
        if "pattern" in schema and re.search(schema["pattern"], value) is None:
            raise ValueError("pattern")
    if type(value) is int:
        if value < schema.get("minimum", value) or value > schema.get("maximum", value):
            raise ValueError("number bounds")
    for item in schema.get("allOf", []):
        check_schema(value, item, root)
    if "if" in schema:
        try:
            check_schema(value, schema["if"], root)
        except ValueError:
            branch = schema.get("else", {})
        else:
            branch = schema.get("then", {})
        check_schema(value, branch, root)


class Job:
    """Sequential state sketch. It implements no threads, queues, C or leases."""
    def __init__(self, generation, offer=False):
        if unsigned(generation) == 0:
            raise ValueError("generation")
        self.generation = generation
        self.phase = "queued"
        self.offer = offer
        self.registered = False
        self.handled = not offer
        self.interrupts = 0
        self.revoked = False

    def claim(self):
        if self.phase != "queued":
            raise ValueError("claim state")
        self.phase = "running"

    def remove_queued(self):
        if self.phase != "queued":
            return False
        self.phase = "quiescent"
        self.revoked = True
        self.offer = False  # No resource-producing command dispatched.
        self.handled = True
        return True

    def interrupt(self, generation):
        if generation != self.generation or self.revoked or self.phase != "running":
            return False
        self.interrupts += 1
        return True

    def complete(self):
        if self.phase != "running":
            raise ValueError("completion state")
        self.revoked = True
        self.phase = "completing" if self.interrupts else "quiescent"

    def join_interrupt(self):
        if self.interrupts == 0:
            raise ValueError("missing interrupt")
        self.interrupts -= 1
        if self.phase == "completing" and not self.interrupts:
            self.phase = "quiescent"

    def register_owner(self):
        if self.phase != "quiescent" or not self.offer or self.handled or self.registered:
            raise ValueError("registration state")
        self.registered = True

    def adopt(self, delivery_deferred):
        if self.phase != "quiescent" or not self.registered or self.handled or not delivery_deferred:
            raise ValueError("adoption guard")
        self.handled = True

    def dispose_offer(self):
        if self.phase != "quiescent" or self.handled:
            raise ValueError("offer state")
        self.registered = False
        self.handled = True  # Abstract assertion, NOT actual disposal/join evidence.

    def release(self):
        if self.phase != "quiescent" or self.interrupts or not self.handled:
            raise ValueError("release ownership")
        self.phase = "released"


class Wake:
    def __init__(self, generation):
        self.generation = unsigned(generation)
        self.registered = True
        self.ready = False

    def notify(self, generation):
        if self.registered and generation == self.generation:
            self.ready = True

    def revoke(self):
        self.registered = False


class QueueBudget:
    """Ordinary admission accounting only, not a real executor/cleanup queue."""
    def __init__(self, entries, payload_bytes):
        self.entries = unsigned(entries)
        self.limit = unsigned(payload_bytes)
        self.payloads = []
        self.used = 0

    def admit(self, payload_bytes):
        size = unsigned(payload_bytes)
        if len(self.payloads) >= self.entries or size > self.limit - self.used:
            return False
        self.payloads.append(size)
        self.used += size
        return True

    def take(self):
        size = self.payloads.pop(0)
        self.used -= size
        return size


def observation(earlier_native_work, current_claimed):
    return "MayHaveExecuted" if earlier_native_work or current_claimed else "NotSubmitted"
