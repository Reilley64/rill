"""Finite declaration model for approved M4.2; not compiler or Rill execution."""
from dataclasses import dataclass, field


@dataclass
class Result:
    summaries: dict = field(default_factory=dict)
    edges: set = field(default_factory=set)
    incomplete: set = field(default_factory=set)
    outcome: str = "complete"


class ResourceLimit(Exception):
    pass


def analyze(declarations, max_work=100_000, max_entries=100_000, max_depth=64):
    result = Result()
    work = entries = 0

    def charge(count=1, storage=False):
        nonlocal work, entries
        if work + count > max_work or storage and entries + count > max_entries:
            raise ResourceLimit
        work += count
        if storage:
            entries += count

    def add(items, value):
        if value not in items:
            charge(storage=True)
            items.add(value)

    def walk(term, origin, collect_edges, depth=1):
        charge()
        if depth > min(max_depth, 64):
            raise ResourceLimit
        if isinstance(term, str):
            return set()
        tag, *parts = term
        if tag == "parameter":
            return {parts[0]}
        if tag == "reference":
            return set()
        if tag == "callable":
            add(result.incomplete, origin)
            return set()
        if tag == "variant":
            return walk(parts[0], origin, collect_edges, depth + 1)
        if tag == "tuple":
            indices = set()
            for item in parts:
                indices |= walk(item, origin, collect_edges, depth + 1)
            return indices
        if tag == "nominal":
            name, arguments = parts
            if collect_edges:
                add(result.edges, (origin, name))
            if name in result.incomplete:
                add(result.incomplete, origin)
            indices = set()
            for index in sorted(result.summaries[name]):
                indices |= walk(arguments[index], origin, collect_edges, depth + 1)
            return indices
        raise ValueError(f"unsupported model term: {tag}")

    try:
        for name in sorted(declarations):
            charge(storage=True)
            result.summaries[name] = set()
        changed = True
        while changed:
            changed = False
            for name, (_, fields) in sorted(declarations.items()):
                charge()
                old = (len(result.summaries[name]), len(result.incomplete))
                for term in fields:
                    for index in walk(term, name, False):
                        add(result.summaries[name], index)
                changed |= old != (len(result.summaries[name]), len(result.incomplete))
        for name, (_, fields) in sorted(declarations.items()):
            for term in fields:
                walk(term, name, True)
        # Separate cycle oracle: transitive closure, not the compiler's SCC algorithm.
        charge(len(result.edges), storage=True)
        reachable = set(result.edges)
        for through in sorted(declarations):
            for left in sorted(declarations):
                for right in sorted(declarations):
                    charge()
                    if (left, through) in reachable and (through, right) in reachable:
                        add(reachable, (left, right))
        if any((name, name) in reachable for name in declarations):
            result.outcome = "rejected"
        if result.incomplete:
            result.outcome = "incomplete"
    except ResourceLimit:
        result.outcome = "resource"
    return result
