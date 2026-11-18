"""Finite path oracle for U2. Tokens are abstract occurrences, not admitted core types."""
from itertools import product
import unittest

from m4_8_result_use import solve


def line(*events):
    return {i: ((event, i + 1),) for i, event in enumerate(events)}


def source(site, *place):
    return ("source", site, place)


def consume(kind, *place):
    return (kind, place)


PASS = ("pass",)
EXIT = ("exit", ("local",))
DIVERGE = ("diverge",)


class ResultUse(unittest.TestCase):
    def test_bounded_path_oracle_covers_replacement_join_and_explicit_use(self):
        # Derive loss for each installed value by scanning its own suffix up to
        # the next replacement. This oracle never carries a worklist state.
        actions = ("observe", "read", "ack", "forward", "overwrite")
        paths = list(product(actions, repeat=3))
        graph = {}
        arms = []
        expected = set()
        for branch, actions in enumerate(paths):
            start = 1 + 5 * branch
            arms.append((PASS, start))
            events = [source(start, "local", "field")]
            events.extend(consume(a, "local", "field") for a in actions)
            events.append(EXIT)
            for i, event in enumerate(events, start):
                graph[i] = ((event, i + 1),) if event != EXIT else ((event, None),)
            for creation in (0, *(i + 1 for i, a in enumerate(actions) if a == "overwrite")):
                first = next(((i + 1, a) for i, a in enumerate(actions)
                              if i + 1 > creation and a != "read"), (4, "exit"))
                if first[1] in ("overwrite", "exit"):
                    expected.add((start + creation, start + first[0], ("local", "field")))
        graph[0] = tuple(arms)
        self.assertEqual(solve(graph), (expected, set()))

    def test_guard_false_keeps_nested_obligation_and_observe_is_shallow(self):
        graph = line(source(10, "local"), source(11, "local", "payload"), consume("observe", "local"))
        graph.update({3: ((consume("ack", "local", "payload"), 4), (PASS, 4)), 4: ((EXIT, None),)})
        self.assertEqual(solve(graph), ({(11, 4, ("local", "payload"))}, set()))

    def test_same_site_in_multiple_iterations_and_unyielded_owned_storage(self):
        graph = {
            0: ((source(10, "local", "element"), 1), (EXIT, None)),
            1: ((("move", ("local", "element"), ("local", "iterator")), 0),
                (consume("ack", "local", "element"), 2)),
            2: ((EXIT, None),),
        }
        # Before the last yield is acknowledged, another occurrence from the
        # same site remains in the iterator. Replacing it also loses a value.
        self.assertEqual(solve(graph), ({(10, exit, ("local", "iterator")) for exit in (0, 1, 2)}, set()))

    def test_skipped_region_still_checks_its_own_discard_without_merging_paths(self):
        graph = line(source(10, "local"), DIVERGE, source(20, "local"), EXIT)
        self.assertEqual(solve(graph, entries=(0, 2)), ({(20, 3, ("local",))}, set()))

    def test_same_origin_siblings_have_distinct_discard_witnesses(self):
        graph = line(source(10, "local", "first"), source(10, "local", "second"), EXIT)
        expected = {(10, 2, ("local", field)) for field in ("first", "second")}
        self.assertEqual(solve(graph), (expected, set()))

    def test_missing_summary_blocks_only_dependent_components(self):
        graph = line(source(10, "local", "dynamic"), source(11, "local", "known"),
                     consume("unknown", "local", "dynamic"), EXIT)
        self.assertEqual(solve(graph), ({(11, 3, ("local", "known"))}, {2}))
