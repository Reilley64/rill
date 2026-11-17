"""M4.2 finite reference cases. These do not execute the compiler or Rill."""
import unittest

from m4_2_layout import analyze


def nominal(name, *args):
    return ("nominal", name, args)


class LayoutModelTests(unittest.TestCase):
    def test_t15_forwarding_nesting_and_phantom_arguments(self):
        result = analyze({
            "Box": (1, (("parameter", 0),)),
            "Phantom": (1, ("i32",)),
            "Nested": (0, (nominal("Box", nominal("Box", "i32")),)),
            "Node": (0, (nominal("Phantom", nominal("Node")),)),
        })
        self.assertEqual(result.outcome, "complete")
        self.assertEqual(result.summaries, {
            "Box": {0}, "Phantom": set(), "Nested": set(), "Node": set(),
        })
        self.assertEqual(result.edges, {("Nested", "Box"), ("Node", "Phantom")})
        self.assertEqual(result.incomplete, set())

    def test_t13_t16_direct_mutual_and_growing_cycles(self):
        for declarations, expected in [
            ({"Chain": (0, (nominal("Chain"),))}, {("Chain", "Chain")}),
            ({"A": (0, (nominal("B"),)), "B": (0, (nominal("A"),))}, {("A", "B"), ("B", "A")}),
            ({"Box": (1, (("parameter", 0),)), "Growing": (1, (nominal("Growing", nominal("Box", ("parameter", 0))),))}, {("Growing", "Growing")}),
        ]:
            with self.subTest(declarations=declarations):
                result = analyze(declarations)
                self.assertEqual(result.outcome, "rejected")
                self.assertEqual(result.edges, expected)

    def test_d2_parent_variant_representation(self):
        result = analyze({
            "Choice": (1, (("parameter", 0),)),
            "Node": (0, (("variant", nominal("Choice", nominal("Node")), "Empty"),)),
        })
        self.assertEqual(result.outcome, "rejected")
        self.assertEqual(result.summaries, {"Choice": {0}, "Node": set()})
        self.assertEqual(result.edges, {("Node", "Choice"), ("Node", "Node")})

    def test_d3_callable_unknowns_references_tuples_and_unused_arguments(self):
        result = analyze({
            "Box": (1, (("parameter", 0),)),
            "Phantom": (1, ("i32",)),
            "Unknown": (0, (("callable", "i32", "bool"),)),
            "Forward": (0, (nominal("Unknown"),)),
            "Stopped": (0, (("reference", nominal("Stopped")),)),
            "Unused": (0, (nominal("Phantom", ("callable", "i32")),)),
            "Tuple": (2, (("tuple", ("parameter", 1), ("reference", ("parameter", 0))),)),
        })
        self.assertEqual(result.outcome, "incomplete")
        self.assertEqual(result.incomplete, {"Unknown", "Forward"})
        self.assertEqual(result.summaries["Tuple"], {1})
        self.assertEqual(result.edges, {("Forward", "Unknown"), ("Unused", "Phantom")})

    def test_all_two_vertex_graphs_match_an_independent_cycle_oracle(self):
        pairs = [("A", "A"), ("A", "B"), ("B", "A"), ("B", "B")]
        for mask in range(16):
            edges = {pair for bit, pair in enumerate(pairs) if mask & (1 << bit)}
            declarations = {name: (0, tuple(nominal(right) for left, right in sorted(edges) if left == name)) for name in ("A", "B")}
            expected_cycle = bool(mask & 1 or mask & 8 or (mask & 2 and mask & 4))
            result = analyze(declarations)
            reversed_result = analyze(dict(reversed(list(declarations.items()))))
            self.assertEqual(result.outcome, "rejected" if expected_cycle else "complete")
            self.assertEqual(result.edges, edges)
            self.assertEqual(result, reversed_result)

    def test_controlled_limits_never_prove_finite_layout(self):
        declarations = {"Loop": (0, (nominal("Loop"),))}
        for limit in ("max_work", "max_entries", "max_depth"):
            self.assertEqual(analyze(declarations, **{limit: 0}).outcome, "resource")

    def test_ordered_forwarding_reaches_a_least_fixed_point(self):
        result = analyze({
            "A": (2, (nominal("B", ("parameter", 0), ("parameter", 1)),)),
            "B": (2, (nominal("Z", ("parameter", 1)),)),
            "Z": (1, (("parameter", 0),)),
        })
        self.assertEqual(result.summaries, {"A": {1}, "B": {1}, "Z": {0}})
        self.assertEqual(result.edges, {("A", "B"), ("B", "Z")})
        self.assertEqual(result.outcome, "complete")

    def test_empty_declarations_and_no_name_or_private_field_exemption(self):
        result = analyze({"Empty": (0, ()), "Unit": (0, ("void",)), "Bottom": (0, ("never",))})
        self.assertEqual(result.outcome, "complete")
        self.assertEqual(result.edges, set())
        # Model fields are storage types; source visibility does not alter them.
        for name in ("PrivateSelf", "List", "Shared"):
            self.assertEqual(analyze({name: (0, (nominal(name),))}).outcome, "rejected")
        self.assertEqual(analyze({"List": (0, (("reference", nominal("List")),))}).outcome, "complete")


if __name__ == "__main__":
    unittest.main()
