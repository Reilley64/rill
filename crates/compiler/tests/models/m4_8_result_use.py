"""Finite storage paths and exact transitions for the M4.8 Result-use procedure."""
from collections import deque


def below(path, part):
    return path[:len(part)] == part


def solve(graph, entries=(0,)):
    """Close the finite set of (point, unhandled occurrence partition) states.

    Values at dynamic or otherwise unknown paths leave this exact partition;
    retain a blocker instead of deriving either success or an exact discard.
    An edge starts or commits an operation, never a trial pattern binding.
    """
    queue = deque((point, frozenset()) for point in entries)
    seen = set()
    discards, blockers = set(), set()
    while queue:
        point, incoming = queue.popleft()
        if (point, incoming) in seen:
            continue
        seen.add((point, incoming))
        for (kind, *args), target in graph.get(point, ()):
            if kind == "diverge":
                continue
            state = set(incoming)
            if kind in ("source", "overwrite", "move", "exit"):
                place = args[-1]
                displaced = {(site, path) for site, path in state if below(path, place)}
                discards.update((site, point, path) for site, path in displaced)
                state.difference_update(displaced)
            if kind == "source":
                state.add(tuple(args))
            elif kind == "overwrite":
                state.add((point, args[0]))
            elif kind == "move":
                src, dst = args
                moved = {(site, path) for site, path in state if below(path, src)}
                state.difference_update(moved)
                state.update((site, dst + path[len(src):]) for site, path in moved)
            elif kind == "observe":
                state = {(site, path) for site, path in state if path != args[0]}
            elif kind in ("ack", "forward", "unknown"):
                remaining = {(site, path) for site, path in state if not below(path, args[0])}
                if kind == "unknown" and remaining != state:
                    blockers.add(point)
                state = remaining
            if target is not None:
                queue.append((target, frozenset(state)))
    return discards, blockers
