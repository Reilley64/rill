"""Bounded ground enumeration challenges the symbolic ordinary-input procedure."""
from itertools import combinations, product
import unittest

from m4_5_inputs import Term, Var, alternatives, includes, intersects, normalize, protection_conflict, select


def c(head, *args):
    return Term(head, args)


B, I = c("bool"), c("i32")
T, U = Var("T"), Var("U")
ENUMS = {"E": ("E.Left", "E.Right")}


def ground_instances(vector, values):
    """Enumerate substitutions directly; do not use matching or unification."""
    def variables(term):
        if isinstance(term, Var):
            return {term}
        return set().union(*(variables(child) for child in term.args))

    names = sorted(set().union(*(variables(term) for term in vector)), key=lambda v: v.name)

    def ground(term, env):
        if isinstance(term, Var):
            return env[term]
        return c(term.head, *(ground(child, env) for child in term.args))

    return {tuple(ground(term, dict(zip(names, args))) for term in vector)
            for args in product(values, repeat=len(names))}


class OrdinaryInputs(unittest.TestCase):
    def test_normalization_uses_first_occurrence_across_positions(self):
        self.assertEqual(normalize((c("Wrap", U), T, U)), (c("Wrap", Var(0)), Var(1), Var(0)))
        self.assertEqual(normalize((c("Wrap", U), T, U)), normalize((c("Wrap", T), U, T)))
        self.assertNotEqual(normalize((T, T)), normalize((T, U)))

    def test_fixed_parent_and_read_only_admit_variants_without_nested_covariance(self):
        for wrapper in (lambda x: x, lambda x: c("&", x)):
            self.assertEqual(set(alternatives((wrapper(c("E", T)),), ENUMS)),
                             {(wrapper(c(name, T)),) for name in ("E", *ENUMS["E"])})
        for head in ("&mut", "Wrap", "tuple"):
            vector = (c(head, c("E", T)),)
            self.assertEqual(alternatives(vector, ENUMS), (vector,))

    def test_callable_mode_admission_is_outer_and_directional(self):
        modes = ("read", "mut", "once")
        for i, mode in enumerate(modes):
            signature = c(("call", mode), c("E", B), T)
            self.assertEqual(set(alternatives((signature,), ENUMS)),
                             {(c(("call", actual), c("E", B), T),) for actual in modes[:i + 1]})
            reference = (c("&", signature),)
            self.assertEqual(alternatives(reference, ENUMS), (reference,))

    def test_finite_union_relations_against_ground_substitution(self):
        # Uniform substitution pool for each pair. Two unknown constructor arities
        # separate distinct variables even from restrictions nested in other patterns.
        values = (B, I, c("E", B), c("E.Left", B), c("E.Right", B),
                  c("Wrap", B), c("tuple", B, B, B), c("tuple", I, I, I, I))
        atoms = ((T, T), (T, U), (B, T), (T, I), (B, B), (I, I),
                 (c("Wrap", T), T), (T, c("Wrap", T)), (c("Wrap", B), B),
                 (c("tuple", T, U), T), (c("tuple", T, T), U),
                 (c("E", T), c("E", T)), (c("E.Left", T), c("E.Right", T)),
                 (c("E.Left", B), c("E.Right", B)))
        exact = tuple(dict.fromkeys(v for p in atoms for v in alternatives(p, ENUMS)))
        unions = [(v,) for v in exact] + list(combinations(exact, 2))
        unions.append(alternatives((c("E", T), c("E", T)), ENUMS))
        # Cross-pool substitutions can be needed to witness intersection, e.g.
        # T = Wrap<U>. Enumerate a larger pool without making it the language domain.
        overlap_values = (*values, *(c("Wrap", v) for v in values),
                          *(c("tuple", a, b) for a, b in product(values, repeat=2)),
                          *(c(name, v) for name in ("E", *ENUMS["E"]) for v in values),
                          c("Wrap", c("tuple", B, B)))
        finite = {v: ground_instances(v, overlap_values) for v in exact}
        overlaps = {(a, b): not finite[a].isdisjoint(finite[b]) for a, b in product(exact, repeat=2)}
        domains = [set().union(*(finite[v] for v in union)) for union in unions]
        # Use a smaller left probe set for inclusion. Every subterm in its
        # generated vectors is in overlap_values, including constructed terms.
        # Equal pools on both sides would truncate valid containing domains.
        probes = {v: ground_instances(v, values) for v in exact}
        for i, left in enumerate(unions):
            probe_domain = set().union(*(probes[v] for v in left))
            for j, right in enumerate(unions):
                if includes(left, right) != (probe_domain <= domains[j]):
                    self.fail(f"inclusion differs from ground oracle: {left!r}, {right!r}")
                # Two different generating substitutions can have the same ground
                # vector. The oracle checks actual set intersection, not equations.
                if i <= j and intersects(left, right) != any(overlaps[a, b] for a in left for b in right):
                    self.fail(f"intersection differs from ground oracle: {left!r}, {right!r}")

    def test_individually_satisfiable_columns_can_form_a_cycle(self):
        left = ((T, c("Wrap", T)),)
        right = ((c("Wrap", U), U),)
        self.assertFalse(intersects(left, right))
        self.assertTrue(intersects(left, ((U, c("Wrap", U)),)))

    def test_selection_uses_whole_vector_dominance(self):
        a, b = c("E.Left", B), c("E.Right", B)
        fixed = alternatives((c("E", B), c("E", B)), ENUMS)
        repeated = ((T, T),)
        candidates = (fixed, repeated)
        self.assertEqual(select(candidates, (a, b)), (0,))
        self.assertEqual(select(candidates, (I, I)), (1,))
        self.assertEqual(select(candidates, (a, a)), (0, 1))
        self.assertEqual(select((*candidates, ((a, a),)), (a, a)), (2,))
        self.assertEqual(select(candidates, (a, I)), ())
        self.assertEqual(select((((B, T),), ((U, B),)), (B, B)), (0, 1))

    def test_prohibited_domain_has_no_more_specific_provider_escape(self):
        required = ((B,),)
        adapter = ((T,),)
        self.assertFalse(protection_conflict(adapter, required, required_winner=True))
        self.assertTrue(protection_conflict(adapter, required, required_winner=False))
        self.assertTrue(protection_conflict(required, required, required_winner=True))
        self.assertFalse(protection_conflict(((I,),), required, required_winner=False))
        self.assertIsNone(protection_conflict(adapter, None, required_winner=False))
        enum_domain = alternatives((c("E", B), c("E", B)), ENUMS)
        self.assertTrue(protection_conflict(((T, T),), enum_domain, required_winner=True))
        self.assertTrue(protection_conflict(((c("E.Left", B), c("E.Left", B)),), enum_domain,
                                            required_winner=True))

    def test_constraints_filter_without_changing_priority(self):
        fixed = alternatives((c("E", B),), ENUMS)
        candidates = (((T,),), fixed)
        self.assertEqual(select(candidates, (c("E", B),), ({T: c("E", B)}, {})), (1,))
        self.assertEqual(select(candidates, (c("E.Left", B),), ({T: c("E", B)}, {})), (1,))
        self.assertEqual(select((((T, T),),), (I, I), ({T: B},)), ())


if __name__ == "__main__":
    unittest.main()
