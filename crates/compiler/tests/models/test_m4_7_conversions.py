"""Bounded oracles for the approved M4.7 procedure; no core admission or execution."""
from itertools import combinations, permutations, product
import unittest

from m4_5_inputs import Term, Var
from m4_7_conversions import (applicability, identity_gate, pair_match, pair_relation,
                              output_routes, select)


B, I, P, V, W = map(Term, ("bool", "i32", "E", "E.Tag", "E.Other"))
T, U = Var("T"), Var("U")
ENUMS = {P.head: (V.head, W.head)}


def wrap(value):
    return Term("Wrap", (value,))


def ref(value, mutable=False):
    return Term("&mut" if mutable else "&", (value,))


class PairMatching(unittest.TestCase):
    def test_only_the_written_outer_source_can_admit_a_variant(self):
        self.assertIsNone(pair_match((wrap(T), T), (wrap(P), V), ENUMS))
        self.assertEqual(pair_match((wrap(T), P), (wrap(P), V), ENUMS), (V, P))
        self.assertEqual(pair_match((wrap(T), T), (wrap(P), P), ENUMS), (P, P))
        self.assertIsNone(pair_match((P, I), (V, I), ENUMS))
        self.assertIsNone(pair_match((I, wrap(P)), (I, wrap(V)), ENUMS))
        self.assertEqual(pair_match((I, ref(P)), (I, ref(V)), ENUMS), (ref(V), ref(P)))
        self.assertIsNone(pair_match((I, ref(P, True)), (I, ref(V, True)), ENUMS))

    def test_exact_filter_compares_the_written_receiver_not_actual(self):
        self.assertIsNone(pair_match((I, V), (I, V), ENUMS, P))
        self.assertEqual(pair_match((I, P), (I, V), ENUMS, P), (V, P))
        self.assertIsNone(pair_match((I, T), (I, V), ENUMS, P))

    def test_finite_pair_relations_agree_with_independent_ground_sets(self):
        # The ground oracle describes the sets directly, not via model matching.
        leaves = (B, I, P, V, W)
        # Include the nested witness for Wrap<T> <- T intersecting Wrap<A> <- Wrap<B>.
        values = leaves + tuple(map(wrap, leaves))
        domains = (
            ((wrap(T), wrap(T)), {(wrap(x), wrap(x)) for x in values}),
            ((wrap(T), wrap(U)), set(product(map(wrap, values), repeat=2))),
            ((wrap(B), wrap(T)), {(wrap(B), wrap(x)) for x in values}),
            ((wrap(T), wrap(I)), {(wrap(x), wrap(I)) for x in values}),
            ((wrap(T), P), set(product(map(wrap, values), (P, V, W)))),
            ((wrap(T), V), {(wrap(x), V) for x in values}),
            ((wrap(T), T), {(wrap(x), x) for x in values}),
            ((wrap(P), P), {(wrap(P), x) for x in (P, V, W)}),
        )
        universe = set.union(*(d for _, d in domains))
        for (a, ad), (b, bd) in product(domains, repeat=2):
            self.assertEqual(pair_relation(a, b, ENUMS), (ad <= bd, bd <= ad, bool(ad & bd)), (a, b))
        for pattern, declared in domains:
            self.assertEqual({pair for pair in universe if pair_match(pattern, pair, ENUMS) is not None}, declared)

    def test_identity_gate_neither_admits_a_variant_nor_looks_for_fallback(self):
        self.assertEqual(identity_gate("into", (P, V), P, True), "not-identity")
        self.assertEqual(identity_gate("tryInto", (P, P), None, True), "not-identity")
        self.assertEqual(identity_gate("into", (P, P), V, True), "filter-conflict")
        self.assertEqual(identity_gate("into", (P, P), V, False), "provider-pending")
        self.assertEqual(identity_gate("into", (P, P), None, True), "identity")


class Applicability(unittest.TestCase):
    def test_failed_conjunct_can_settle_an_unrelated_pending_one(self):
        for goals in permutations(("unresolved", "disproved", "proved")):
            self.assertEqual(applicability(goals, False), "disproved")
        self.assertEqual(applicability(("proved",), False), "unresolved")
        self.assertEqual(applicability(("proved",), True), "proved")
        self.assertEqual(applicability(("disproved", "limit"), False), "limit")

    def test_compare_possible_maxima_to_all_requirement_completions(self):
        patterns = ((T, U), (I, U), (T, P), (I, P))
        # Independent test oracle: all four match (I,P). Only 1 and 2 cross.
        strict = {(1, 0), (2, 0), (3, 0), (3, 1), (3, 2)}
        for states in product(("proved", "disproved", "unresolved"), repeat=4):
            proved = {i for i, s in enumerate(states) if s == "proved"}
            unresolved = {i for i, s in enumerate(states) if s == "unresolved"}
            possible = set()
            for size in range(len(unresolved) + 1):
                for ready in combinations(unresolved, size):
                    active = proved | set(ready)
                    possible.update(i for i in active if all((j, i) not in strict for j in active))
            if possible & unresolved:
                expected = ("unresolved", ())
            elif len(possible) == 1:
                expected = ("winner", tuple(sorted(possible)))
            else:
                expected = ("ambiguity" if possible else "absent", tuple(sorted(possible)))
            self.assertEqual(select(patterns, states, ENUMS), expected, states)
        self.assertEqual(select(patterns, ("proved", "disproved", "disproved", "limit"), ENUMS),
                         ("limit", ()))


class OutputRoutes(unittest.TestCase):
    def test_reachability_agrees_with_every_closed_subset(self):
        names = ("T", "E", "F")
        subsets = [set(s) for k in range(4) for s in combinations(names, k)]
        # One clause per output, with all combinations of read sets, including cycles.
        for reads in product(subsets, repeat=3):
            clauses = tuple((frozenset(r), (w,)) for w, r in zip(names, reads))
            for seed in subsets:
                closed = [s for s in subsets if seed <= s
                          and all(not (r <= s) or set(w) <= s for r, w in clauses)]
                expected = set.intersection(*closed)
                for ordering in (clauses, tuple(reversed(clauses))):
                    known, used = output_routes(seed, ordering)
                    self.assertEqual(known, expected)
                    seen = set(seed)
                    for index in used:
                        r, w = ordering[index]
                        self.assertTrue(r <= seen)
                        seen.update(w)
                    self.assertEqual(seen, expected)

    def test_unclassified_clause_cannot_supply_a_route(self):
        clauses = ((None, ("Unknown",)), (frozenset({"T"}), ("E", "Nested")),
                   (frozenset({"Nested"}), ("F",)))
        self.assertEqual(output_routes({"T"}, clauses), ({"T", "E", "Nested", "F"}, (1, 2)))
        self.assertEqual(output_routes(set(), clauses), (set(), ()))
