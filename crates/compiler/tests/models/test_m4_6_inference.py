"""Adversarial examples for the M4.6 abstract procedure, not Rill programs."""
from itertools import permutations, product
import unittest

from m4_5_inputs import Term, Var
from m4_6_inference import (Alternative, Default, Equation, Goal, Rule, agree, prove,
                            solve, uniform_winner)


class DefaultReadiness(unittest.TestCase):
    def test_arrow_output_can_preempt_an_earlier_default(self):
        arrow = Equation("R", ("A",), lambda a: "bool" if a == "bool" else "i64")
        defaults = (Default("R", (), lambda: "i32"), Default("A", (), lambda: "bool"))
        self.assertEqual(solve({}, (arrow,), defaults), ("complete", {"A": "bool", "R": "bool"}, ("A",)))

    def test_independent_anchors_win_and_conflicts_are_not_repaired(self):
        anchors = (Equation("T", (), lambda: "bool"), Equation("T", (), lambda: "i32"))
        default = (Default("T", (), lambda: "i32"),)
        self.assertEqual(solve({}, anchors[:1], default), ("complete", {"T": "bool"}, ()))
        for ordering in permutations(anchors):
            status, _, used = solve({}, ordering, default)
            self.assertEqual((status, used), ("conflict", ()))

    def test_required_reads_include_indirect_dependencies(self):
        rules = (Equation("P", ("T",), lambda t: ("Box", t)),
                 Equation("T", ("P",), lambda p: p[1]),
                 Equation("R", ("P",), lambda p: p[1]))
        defaults = (Default("R", (), lambda: "i32"), Default("T", (), lambda: "bool"))
        for ordering in permutations(rules):
            self.assertEqual(solve({}, ordering, defaults),
                             ("complete", {"P": ("Box", "bool"), "R": "bool", "T": "bool"}, ("T",)))

    def test_unavailable_producer_and_blocked_default_are_not_ready(self):
        pending = Equation("T", None, lambda: "bool")
        self.assertEqual(solve({}, (pending,), (Default("T", (), lambda: "i32"),)), ("pending", {}, ()))
        arrow = Equation("R", ("A",), lambda a: "bool")
        defaults = (Default("R", (), lambda: "i32"), Default("A", ("R",), lambda r: r))
        self.assertEqual(solve({}, (arrow,), defaults), ("pending", {}, ()))


I, B, V, E = map(Term, ("i32", "bool", "E.V", "E"))
T, U, F, G = map(Var, ("T", "U", "F", "G"))


def alt(*terms):
    return Alternative(terms, terms)


class ContextAgreement(unittest.TestCase):
    def test_an_anchor_does_not_cancel_an_unconstrained_fallback(self):
        repeated, fallback = alt(T, T), alt(T, U)
        anchors = {0: I}
        self.assertEqual(agree((repeated,), anchors, (), (1,), "type"), I)
        self.assertIsNone(agree((repeated, fallback), anchors, (), (1,), "type"))
        self.assertIsNone(agree((fallback,), anchors, (), (1,), "type"))
        self.assertIsNone(agree((repeated, None), anchors, (), (1,), "type"))

    def test_ground_oracle_leaves_exactly_the_correlated_alternatives(self):
        plain = alt(T, T, T), alt(B, T, I), alt(T, I, I)
        pool = (I, B, V)
        for first, second in product(pool, repeat=2):
            # Enumerate known actual vectors independently of model matching.
            candidates = ([(x, x, x) for x in pool], [(B, x, I) for x in pool],
                          [(x, I, I) for x in pool])
            possible = {v[2] for vs in candidates for v in vs if v[:2] == (first, second)}
            expected = next(iter(possible)) if len(possible) == 1 else None
            self.assertEqual(agree(plain, {0: first, 1: second}, (), (2,), "type"), expected)

    def test_project_receiving_types_not_the_actual_admissions(self):
        parent, variant = Alternative((V, E), (E, E)), Alternative((E, E), (E, E))
        self.assertEqual(agree((parent, variant), {}, (), (0,), "type"), E)
        self.assertEqual(agree((parent, variant), {0: V}, (), (1,), "type"), E)

    def test_tuple_category_precedes_arity_and_path_specific_callback_context(self):
        args = Term("tuple", (F, G))
        pair = Alternative((args,), (args,), (((0, 0), (), T), ((0, 1), (I,), U)))
        triple = alt(Term("tuple", (F, G, T)))
        self.assertEqual(agree((pair, triple), {}, (), (0, 0), "params", bracket_arity=2), ())
        self.assertEqual(agree((pair, triple), {}, (), (0, 1), "params", bracket_arity=2), (I,))
        self.assertIsNone(agree((pair, triple), {}, (), (0, 0), "result", bracket_arity=2))
        for other in (alt(F), alt(Term("List", (F,)))):
            self.assertIsNone(agree((pair, other), {}, (), (0, 0), "params", bracket_arity=2))

    def test_borrowed_callback_can_get_inputs_from_a_written_clause_only(self):
        lst = Term("List", (T,))
        borrow = Term("&", (lst,))
        owned = Alternative((lst, F), (lst, F), (((1,), (T,), U),))
        shared = Alternative((borrow, F), (borrow, F), (((1,), (Term("&", (T,)),), U),))
        actual = Term("&", (Term("List", (I,)),))
        self.assertEqual(agree((owned, shared), {0: actual}, (), (1,), "params"), (Term("&", (I,)),))
        self.assertIsNone(agree((owned, shared), {}, (), (1,), "params"))

    def test_filters_can_change_the_winner_without_changing_priority(self):
        # Each added column represents the same family parameter under its header substitution.
        original = (((T,),), ((E,), (V,)))
        filtered = (((T, T),), ((E, E), (V, E)))
        for types, winner in (((V, V), 0), ((V, E), 1), ((E, E), 1), ((I, V), None)):
            result = uniform_winner((types,), original, filtered)
            self.assertEqual(result, winner)

    def test_uniform_winner_proof_agrees_with_ground_winners_on_bounded_domain(self):
        original = (((T, U),), ((I, T),), ((I, I),))
        finite_domain = (I, B, V)
        for symbolic in ((F, G), (F, F), (B, F), (I, F), (I, I), (I, B)):
            actual = {tuple(dict(zip((F, G), assignment)).get(t, t) for t in symbolic)
                      for assignment in product(finite_domain, repeat=2)}
            # The concrete priority in this fixture is known by construction.
            winners = {2 if b == I else 1 if a == I else 0 for a, b in actual if a == I}
            if any(a != I for a, _ in actual):
                winners.add(0)
            expected = next(iter(winners)) if len(winners) == 1 else None
            self.assertEqual(uniform_winner((symbolic,), original, original), expected)


class Evidence(unittest.TestCase):
    def test_expected_result_does_not_determine_output_or_change_winner(self):
        request = Goal("prove", "read", (I,), (), E)
        model = {request.selection_key(): Rule(V)}
        self.assertEqual(prove(request, model.get).state, "disproved")
        model[request.selection_key()] = Rule(None)
        self.assertEqual(prove(request, model.get).state, "unresolved")
        determined = Goal("determine", "read", (I,))
        model[determined.selection_key()] = Rule(V)
        self.assertEqual(prove(determined, model.get).completed[determined], V)

    def test_selected_obligation_does_not_seed_its_own_proof(self):
        root = Goal("prove", "g", (I,), (), B)
        rules = {root.selection_key(): Rule(B, (root,))}
        attempt = prove(root, rules.get)
        self.assertEqual(attempt.state, "unresolved")
        self.assertEqual(attempt.cycles, ((root, root),))
        self.assertEqual(attempt.completed, {})
        self.assertEqual(prove(root, rules.get, {root: B}).state, "proved")

    def test_filter_and_origin_keys_do_not_share_assumptions(self):
        plain = Goal("prove", "g", (V,), (V,), B, ("empty-environment",))
        borrowed = Goal("prove", "g", (V,), (V,), B, ("borrow-field",))
        filtered = Goal("prove", "g", (V,), (E,), B, plain.origins)
        for different in (borrowed, filtered):
            self.assertEqual(prove(different, {}.get, {plain: B}).state, "unresolved")

    def test_growing_keys_exhaust_budget_and_preserve_independent_facts(self):
        root = Goal("prove", "root", (), (), B)
        seed = Goal("prove", "seed", (), (), B)
        growing = Goal("prove", "grow", (I,), (), B)

        def choose(key):
            if key == root.selection_key():
                return Rule(B, (seed, growing))
            if key == seed.selection_key():
                return Rule(B)
            return Rule(B, (Goal("prove", "grow", (Term("Box", key[1]),), (), B),))

        result = prove(root, choose, limit=7)
        self.assertEqual(result.state, "resource-limit")
        self.assertEqual(result.cycles, ())
        self.assertEqual(result.completed, {seed: B})
        finite = {root.selection_key(): Rule(B, (seed,)), seed.selection_key(): Rule(B)}
        self.assertEqual(prove(root, finite.get, limit=2).state, "proved")
        self.assertEqual(prove(root, finite.get, limit=1).state, "resource-limit")

    def test_successful_graph_proofs_match_least_ground_closure(self):
        nodes = [Goal("prove", n, (), (), B) for n in ("a", "b", "c")]
        edges = [tuple(n for i, n in enumerate(nodes) if mask & (1 << i)) for mask in range(8)]
        for deps in product(edges, repeat=3):
            choose = {n.selection_key(): Rule(B, d) for n, d in zip(nodes, deps)}.get
            derived = set()
            for _ in nodes:
                derived.update(n for n, requirement in zip(nodes, deps) if set(requirement) <= derived)
            for node in nodes:
                expected = "proved" if node in derived else "unresolved"
                self.assertEqual(prove(node, choose).state, expected)


if __name__ == "__main__":
    unittest.main()
