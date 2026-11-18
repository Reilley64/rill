"""Independent finite shape enumeration for the approved M4.4 coverage criterion.

Probes, especially Component holes, are declared shapes, never Rill values.
The compiler specializes pattern matrices; this reference enumerates products.
"""
from dataclasses import dataclass
from enum import Enum
from itertools import product


@dataclass(frozen=True)
class Limits:
    work: int = 100_000
    entries: int = 100_000
    depth: int = 64


@dataclass(frozen=True)
class Shapes:
    root: str
    constructors: dict[str, tuple[tuple[str, tuple[str, ...]], ...]]


@dataclass(frozen=True)
class Pattern:
    tag: str | None = None
    fields: tuple["Pattern", ...] = ()


@dataclass(frozen=True)
class Row:
    pattern: Pattern
    guarded: bool = False


@dataclass(frozen=True)
class Probe:
    tag: str | None
    fields: tuple["Probe", ...] = ()
    hole: str | None = None


class Status(Enum):
    COVERED = "Covered"
    UNCOVERED = "UncoveredDeclaredShape"
    INCOMPLETE = "Incomplete"
    RESOURCE = "ResourceLimit"


@dataclass(frozen=True)
class Result:
    status: Status
    witness: Probe | None = None
    work: int = 0
    entries: int = 0
    depth: int = 0


class Exhausted(Exception):
    pass


class Unsupported(Exception):
    pass


class Budget:
    def __init__(self, limits):
        self.limits = limits
        self.work = self.entries = self.depth = 0

    def step(self, n=1):
        if self.work + n > self.limits.work:
            raise Exhausted
        self.work += n

    def reserve(self, n=1):
        self.step(n)
        if self.entries + n > self.limits.entries:
            raise Exhausted
        self.entries += n

    def visit(self, depth):
        self.step()
        if depth > min(self.limits.depth, 64):
            raise Exhausted
        self.depth = max(self.depth, depth)


def coverage(static_shapes, pattern_rows, limits=Limits()):
    budget = Budget(limits)
    definitions = static_shapes.constructors

    def validate(ty, pattern, depth):
        budget.visit(depth)
        if ty not in definitions:
            raise Unsupported
        if pattern.tag is None:
            if pattern.fields:
                raise Unsupported
            return
        for tag, fields in definitions[ty]:
            budget.step()
            if tag == pattern.tag:
                if len(fields) != len(pattern.fields):
                    raise Unsupported
                for field, child in zip(fields, pattern.fields):
                    validate(field, child, depth + 1)
                return
        raise Unsupported

    def enumerate_shapes(ty, component, depth):
        budget.visit(depth)
        if ty not in definitions:
            raise Unsupported
        if not definitions[ty]:
            if component:
                budget.reserve()
                return [Probe(None, hole=ty)]
            return []
        values = []
        for tag, fields in definitions[ty]:
            budget.step()
            budget.reserve(len(fields))
            choices = [enumerate_shapes(field, True, depth + 1) for field in fields]
            for parts in product(*choices):
                budget.reserve(1 + len(parts))
                values.append(Probe(tag, parts))
        return values

    def matches(pattern, probe, depth):
        budget.visit(depth)
        if pattern.tag is None:
            return True
        if probe.hole is not None or pattern.tag != probe.tag:
            return False
        return all(matches(p, v, depth + 1) for p, v in zip(pattern.fields, probe.fields))

    try:
        budget.step()
        if static_shapes.root not in definitions:
            raise Unsupported
        for row in pattern_rows:
            validate(static_shapes.root, row.pattern, 1)
        if any(not row.guarded and row.pattern.tag is None for row in pattern_rows):
            status, witness = Status.COVERED, None
        else:
            status, witness = Status.COVERED, None
            for probe in enumerate_shapes(static_shapes.root, False, 1):
                covered = False
                for row in pattern_rows:
                    budget.step()
                    if not row.guarded and matches(row.pattern, probe, 1):
                        covered = True
                        break
                if not covered:
                    status, witness = Status.UNCOVERED, probe
                    break
    except Exhausted:
        status, witness = Status.RESOURCE, None
    except Unsupported:
        status, witness = Status.INCOMPLETE, None
    return Result(status, witness, budget.work, budget.entries, budget.depth)
