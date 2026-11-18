"""Abstract conversion pair and evidence relations, not a Rill checker."""
from m4_5_inputs import alternatives, includes, intersects
from m4_6_inference import _apply, _bindings


def _pair_alternatives(pattern, enums):
    to, source = pattern
    return tuple((to, actual) for (actual,) in alternatives((source,), enums))


def pair_relation(left, right, enums):
    a, b = (_pair_alternatives(p, enums) for p in (left, right))
    return includes(a, b), includes(b, a), intersects(a, b)


def pair_match(pattern, pair, enums, source_filter=None):
    """Return the actual and receiving source, with exact destination matching."""
    for accepted in _pair_alternatives(pattern, enums):
        bindings = _bindings(zip(accepted, pair))
        if bindings is None:
            continue
        receiving = _apply(pattern[1], bindings)
        if source_filter is None or receiving == source_filter:
            return pair[1], receiving
    return None


def identity_gate(family, pair, source_filter, admitted):
    if family != "into" or pair[0] != pair[1]:
        return "not-identity"
    if not admitted:
        return "provider-pending"
    return "identity" if source_filter in (None, pair[1]) else "filter-conflict"


def applicability(goals, error_known):
    if "limit" in goals:
        return "limit"
    if "disproved" in goals:
        return "disproved"
    return "proved" if error_known and all(g == "proved" for g in goals) else "unresolved"


def select(patterns, states, enums):
    """Choose from matching pairs. Evidence does not change the pair order."""
    if "limit" in states:
        return "limit", ()
    alternatives = [_pair_alternatives(p, enums) for p in patterns]
    proved = [i for i, state in enumerate(states) if state == "proved"]

    def dominated(i):
        return any(i != j and includes(alternatives[j], alternatives[i])
                   and not includes(alternatives[i], alternatives[j]) for j in proved)

    if any(s == "unresolved" and not dominated(i) for i, s in enumerate(states)):
        return "unresolved", ()
    maxima = tuple(i for i in proved if not dominated(i))
    return ("absent", "winner", "ambiguity")[min(2, len(maxima))], maxima


def output_routes(seed, clauses):
    """Find structural routes without assuming that any called relation succeeds."""
    known, used = set(seed), []
    waiting = list(enumerate(clauses))
    while True:
        ready = False
        for entry in waiting[:]:
            index, (reads, writes) = entry
            if reads is None or not reads <= known:
                continue
            waiting.remove(entry)
            if not known.issuperset(writes):
                known.update(writes)
                used.append(index)
                ready = True
        if not ready:
            return known, tuple(used)
