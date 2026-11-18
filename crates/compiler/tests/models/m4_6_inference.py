"""Finite reference procedure for abstract M4.6 inputs, independent of Rust."""
from dataclasses import dataclass
from typing import Callable

from m4_5_inputs import Term, Var, includes, intersects


@dataclass(frozen=True)
class Equation:
    target: str
    reads: tuple[str, ...] | None
    evaluate: Callable


@dataclass(frozen=True)
class Default:
    target: str
    reads: tuple[str, ...]
    evaluate: Callable


def solve(anchors, equations, defaults):
    """Compare exact ground solutions; evaluate at most one ready default at a time."""
    values, waiting, used = dict(anchors), list(equations), []
    while True:
        ready = [e for e in waiting if e.reads is not None and values.keys() >= set(e.reads)]
        if ready:
            # Evaluate a ready batch against the same facts. No worklist order
            # may pick between incompatible anchors.
            proposals = {}
            for e in ready:
                proposals.setdefault(e.target, set()).add(e.evaluate(*(values[r] for r in e.reads)))
            if any(len(p) != 1 or (t in values and values[t] not in p) for t, p in proposals.items()):
                return "conflict", values, tuple(used)
            values.update((t, next(iter(p))) for t, p in proposals.items())
            waiting = [e for e in waiting if e not in ready]
            continue

        missing = {e.target for e in waiting} | {d.target for d in defaults}
        missing |= {r for e in waiting if e.reads for r in e.reads}
        necessary = {t: {t} for t in missing - values.keys()}
        has_default = {d.target for d in defaults}

        def reads_needed(e):
            return set().union(*(necessary.get(r, ()) for r in (e.reads or ())))

        # A mandatory read must occur for every way of supplying a non-defaulted
        # intermediate. A written default is another way to fill that slot, not
        # evidence that the source expression itself must read its dependencies.
        while True:
            before = sum(map(len, necessary.values()))
            for t, reads in necessary.items():
                producers = [e for e in waiting if e.target == t]
                if producers and t not in has_default:
                    reads.update(set.intersection(*(reads_needed(e) for e in producers)))
            if before == sum(map(len, necessary.values())):
                break

        chosen = next((d for d in defaults
                       if d.target not in values and values.keys() >= set(d.reads)
                       and all(d.target in reads_needed(e) for e in waiting if e.target == d.target)), None)
        if chosen is None:
            pending = waiting or any(d.target not in values for d in defaults)
            return ("pending" if pending else "complete"), values, tuple(used)
        values[chosen.target] = chosen.evaluate(*(values[r] for r in chosen.reads))
        used.append(chosen.target)


@dataclass(frozen=True)
class Alternative:
    actual: tuple
    receiving: tuple
    clauses: tuple = ()


def _bindings(equations):
    bindings = {}
    waiting = list(equations)
    while waiting:
        p, a = waiting.pop()
        if isinstance(p, Var):
            if bindings.setdefault(p, a) != a:
                return None
        elif p.head != a.head or len(p.args) != len(a.args):
            return None
        else:
            waiting.extend(zip(p.args, a.args))
    return bindings


def _apply(t, bindings):
    if isinstance(t, Var):
        return bindings.get(t, t)
    return Term(t.head, tuple(_apply(a, bindings) for a in t.args))


def _known(t):
    return not isinstance(t, Var) and all(_known(a) for a in t.args)


def agree(alternatives, known_inputs, filters, path, projection, bracket_arity=None):
    """Ground partial matching with written receiving terms, never ranked here."""
    viable = []
    for a in alternatives:
        if a is None:  # A missing alternative is not a disproved alternative.
            return None
        bindings = _bindings((*((a.actual[i], t) for i, t in known_inputs.items()), *filters))
        if bindings is not None:
            viable.append((a, bindings, _apply(a.receiving[path[0]], bindings)))
    if bracket_arity is not None:
        if any(isinstance(t, Var) or t.head != "tuple" for _, _, t in viable):
            return None
        viable = [(a, b, t) for a, b, t in viable if len(t.args) == bracket_arity]
    proposals = set()
    for a, bindings, t in viable:
        for i in path[1:]:
            if isinstance(t, Var) or i >= len(t.args):
                return None
            t = t.args[i]
        if projection == "type":
            if not _known(t):
                return None
            proposals.add(t)
            continue
        choices = set()
        for clause_path, args, result in a.clauses:
            if clause_path == path:
                choices.add(tuple(_apply(p, bindings) for p in args) if projection == "params"
                            else _apply(result, bindings))
        if isinstance(t, Term) and t.head == "callable":
            choices.add(t.args[:-1] if projection == "params" else t.args[-1])
        if projection == "params":
            choices = {Term("params", c) for c in choices}
        known_choices = {c for c in choices if _known(c)}
        if len(known_choices) != 1:
            return None
        fixed, = known_choices
        if any(_bindings(((c, fixed),)) is None for c in choices):
            return None
        proposals.add(fixed.args if projection == "params" else fixed)
    return next(iter(proposals)) if len(proposals) == 1 else None


def uniform_winner(domain, original, filtered):
    """Coverage and dominance are independent; filtered columns affect only coverage and overlap."""
    for i, allowed in enumerate(filtered):
        if includes(domain, allowed) and all(
            i == j or not intersects(domain, other) or
            (includes(original[i], original[j]) and not includes(original[j], original[i]))
            for j, other in enumerate(filtered)
        ):
            return i
    return None


@dataclass(frozen=True)
class Goal:
    judgment: str
    family: str
    inputs: tuple
    filters: tuple = ()
    expected: object = None
    origins: tuple = ()

    def selection_key(self):
        return self.family, self.inputs, self.filters, self.origins


@dataclass(frozen=True)
class Rule:
    result: object
    requires: tuple[Goal, ...] = ()


@dataclass(frozen=True)
class Proof:
    state: str
    completed: dict
    cycles: tuple


def prove(root, select, assumptions=None, limit=100):
    """Depth-first finite dependencies with frozen caller assumptions, no retry."""
    hypotheses = dict(assumptions or {})
    completed, blocked, cycles = {}, {}, []
    active = []
    todo = [(root, None)]
    steps = 0
    while todo:
        goal, rule = todo.pop()
        if rule is not None:
            active.pop()
            if any(child not in completed and blocked.get(child) != "disproved" for child in rule.requires):
                blocked[goal] = "unresolved"
            elif any(child not in completed for child in rule.requires) or (
                    goal.judgment == "prove" and goal.expected != rule.result):
                blocked[goal] = "disproved"
            else:
                completed[goal] = rule.result
            continue
        if goal in completed or goal in blocked:
            continue
        if goal in hypotheses:
            completed[goal] = hypotheses[goal]
            continue
        if goal in active:
            cycles.append(tuple(active[active.index(goal):]) + (goal,))
            continue
        if steps == limit:
            return Proof("resource-limit", completed, tuple(cycles))
        steps += 1
        chosen = select(goal.selection_key())
        if chosen is None or chosen.result is None:
            blocked[goal] = "unresolved"
        else:
            active.append(goal)
            todo.append((goal, chosen))
            todo.extend((child, None) for child in reversed(chosen.requires))
    return Proof("proved" if root in completed else blocked[root], completed, tuple(cycles))
