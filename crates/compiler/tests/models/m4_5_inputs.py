"""Abstract finite ordinary-input comparisons; no Rill execution or core admission."""
from dataclasses import dataclass
from itertools import product


@dataclass(frozen=True)
class Var:
    name: object


@dataclass(frozen=True)
class Term:
    head: object
    args: tuple = ()


def _rewrite(term, variable):
    if isinstance(term, Var):
        return variable(term)
    return Term(term.head, tuple(_rewrite(child, variable) for child in term.args))


def normalize(vector):
    indices = {}
    return tuple(_rewrite(term, lambda v: indices.setdefault(v, Var(len(indices))))
                 for term in vector)


def alternatives(vector, enums):
    def outer(term):
        yield term
        if isinstance(term, Var):
            return
        if term.head in enums:
            for variant in enums[term.head]:
                yield Term(variant, term.args)
        elif term.head == "&" and isinstance(term.args[0], Term):
            parent = term.args[0]
            for variant in enums.get(parent.head, ()):
                yield Term("&", (Term(variant, parent.args),))
        elif isinstance(term.head, tuple) and term.head[0] == "call":
            modes = ("read", "mut", "once")
            for mode in modes[:modes.index(term.head[1])]:
                yield Term(("call", mode), term.args)

    return tuple(dict.fromkeys(product(*(outer(term) for term in vector))))


def _matches(pattern, actual, env=None):
    if len(pattern) != len(actual):
        return False
    env = dict(env or {})
    pairs = list(zip(pattern, actual))
    while pairs:
        p, a = pairs.pop()
        if isinstance(p, Var):
            if env.setdefault(p, a) != a:
                return False
        elif isinstance(a, Var) or p.head != a.head or len(p.args) != len(a.args):
            return False
        else:
            pairs.extend(zip(p.args, a.args))
    return True


def includes(left, right):
    """Is every vector admitted by left also admitted by right?"""
    return all(any(_matches(b, a) for b in right) for a in left)


def _intersect(left, right):
    if len(left) != len(right):
        return False
    env = {}
    pairs = list(zip(
        (_rewrite(t, lambda v: Var((0, v.name))) for t in left),
        (_rewrite(t, lambda v: Var((1, v.name))) for t in right),
    ))

    def resolve(term):
        while isinstance(term, Var) and term in env:
            term = env[term]
        return term

    def occurs(var, term):
        term = resolve(term)
        if isinstance(term, Var):
            return term == var
        return any(occurs(var, t) for t in term.args)

    while pairs:
        a, b = map(resolve, pairs.pop())
        if a == b:
            continue
        if isinstance(b, Var) and not isinstance(a, Var):
            a, b = b, a
        if isinstance(a, Var):
            if occurs(a, b):
                return False
            env[a] = b
        elif a.head != b.head or len(a.args) != len(b.args):
            return False
        else:
            pairs.extend(zip(a.args, b.args))
    return True


def intersects(left, right):
    return any(_intersect(a, b) for a in left for b in right)


def protection_conflict(body, domain, *, required_winner):
    """Compare a non-admitted body with one abstract protection domain."""
    if domain is None:
        return None
    if not intersects(body, domain):
        return False
    return not (required_winner and includes(domain, body) and not includes(body, domain))


def select(candidates, actual, constraints=None):
    constraints = constraints or [{}] * len(candidates)
    matches = tuple(i for i, (candidate, env) in enumerate(zip(candidates, constraints))
                    if any(_matches(p, actual, env) for p in candidate))
    for i in matches:
        if all(i == j or (includes(candidates[i], candidates[j])
                          and not includes(candidates[j], candidates[i])) for j in matches):
            return (i,)
    return matches
