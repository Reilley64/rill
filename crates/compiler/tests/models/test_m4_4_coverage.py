"""Finite coverage-reference assertions, not Rill execution."""
import unittest

from m4_4_coverage import Limits, Pattern, Row, Shapes, Status, coverage


class CoverageTests(unittest.TestCase):
    def test_nested_refutable_payload_does_not_cover_its_outer_variant(self):
        shapes = Shapes("E", {"E": (("E.V", ("bool",)),), "bool": (("false", ()), ("true", ()))})
        result = coverage(shapes, (Row(Pattern("E.V", (Pattern("true"),))),), Limits())
        self.assertEqual(result.status, Status.UNCOVERED)
        self.assertEqual(result.witness.tag, "E.V")
        self.assertEqual(result.witness.fields[0].tag, "false")

    def test_root_zero_domains_are_vacuous_but_components_are_holes(self):
        for zero in ("never", "Empty"):
            self.assertEqual(coverage(Shapes(zero, {zero: ()}), ()).status, Status.COVERED)
            for order in ((zero, "bool"), ("bool", zero)):
                shapes = Shapes("E", {"E": (("E.V", order),), zero: (), "bool": (("false", ()), ("true", ()))})
                at = order.index("bool")
                fields = [Pattern(), Pattern()]
                fields[at] = Pattern("true")
                row = Row(Pattern("E.V", tuple(fields)))
                result = coverage(shapes, (row,))
                self.assertEqual(result.status, Status.UNCOVERED)
                self.assertEqual(result.witness.fields[at].tag, "false")
                self.assertEqual(result.witness.fields[1 - at].hole, zero)
                fields[at] = Pattern("false")
                complement = Row(Pattern("E.V", tuple(fields)))
                self.assertEqual(coverage(shapes, (row, complement)).status, Status.COVERED)
                self.assertEqual(coverage(shapes, (Row(Pattern("E.V", (Pattern(), Pattern()))),)).status, Status.COVERED)

    def test_all_two_boolean_product_subsets_have_independent_expected_coverage(self):
        shapes = Shapes("Pair", {"Pair": (("Pair", ("bool", "bool")),), "bool": (("false", ()), ("true", ()))})
        patterns = [Pattern("Pair", (Pattern(a), Pattern(b))) for a, b in (("false", "false"), ("false", "true"), ("true", "false"), ("true", "true"))]
        for mask in range(16):
            rows = tuple(Row(p) for i, p in enumerate(patterns) if mask & (1 << i))
            expected = Status.COVERED if mask == 15 else Status.UNCOVERED
            self.assertEqual(coverage(shapes, rows).status, expected)
            self.assertEqual(coverage(shapes, rows[::-1]).status, expected)

    def test_nested_record_and_tuple_wildcards_cover_only_their_positions(self):
        shapes = Shapes("Record", {"Record": (("Record", ("Pair", "bool")),), "Pair": (("Pair", ("bool", "bool")),), "bool": (("false", ()), ("true", ()))})
        partial = Row(Pattern("Record", (Pattern("Pair", (Pattern(), Pattern("true"))), Pattern())))
        self.assertEqual(coverage(shapes, (partial,)).status, Status.UNCOVERED)
        other = Row(Pattern("Record", (Pattern("Pair", (Pattern(), Pattern("false"))), Pattern())))
        self.assertEqual(coverage(shapes, (partial, other)).status, Status.COVERED)

    def test_guarded_rows_never_prove_coverage(self):
        shapes = Shapes("bool", {"bool": (("false", ()), ("true", ()))})
        rows = (Row(Pattern("false")), Row(Pattern("true"), guarded=True))
        self.assertEqual(coverage(shapes, rows).status, Status.UNCOVERED)
        self.assertEqual(coverage(shapes, (Row(Pattern(), guarded=True),)).status, Status.UNCOVERED)
        self.assertEqual(coverage(shapes, rows + (Row(Pattern()),)).status, Status.COVERED)

    def test_irrefutability_is_the_same_single_row_coverage_question(self):
        shapes = Shapes("E", {"E": (("E.A", ()), ("E.B", ()))})
        self.assertEqual(coverage(shapes, (Row(Pattern("E.A")),)).status, Status.UNCOVERED)
        self.assertEqual(coverage(shapes, (Row(Pattern()),)).status, Status.COVERED)
        known = Shapes("Variant", {"Variant": (("E.A", ()),)})
        self.assertEqual(coverage(known, (Row(Pattern("E.A")),)).status, Status.COVERED)

    def test_limits_are_inclusive_and_exhaustion_is_not_noncoverage(self):
        shapes = Shapes("E", {"E": (("E.V", ("bool",)),), "bool": (("false", ()), ("true", ()))})
        rows = (Row(Pattern("E.V", (Pattern("false"),))), Row(Pattern("E.V", (Pattern("true"),))))
        result = coverage(shapes, rows)
        exact = Limits(result.work, result.entries, result.depth)
        self.assertEqual(coverage(shapes, rows, exact).status, Status.COVERED)
        for limits in (Limits(result.work - 1, result.entries, result.depth), Limits(result.work, result.entries - 1, result.depth), Limits(result.work, result.entries, result.depth - 1)):
            self.assertEqual(coverage(shapes, rows, limits).status, Status.RESOURCE)
        self.assertEqual(coverage(shapes, (Row(Pattern()),), Limits(0, 0, 0)).status, Status.RESOURCE)

    def test_recursive_expansion_is_bounded_and_wildcards_need_no_unfolding(self):
        shapes = Shapes("Node", {"Node": (("Next", ("Node",)),)})
        self.assertEqual(coverage(shapes, ()).status, Status.RESOURCE)
        self.assertEqual(coverage(shapes, (Row(Pattern()),)).status, Status.COVERED)

    def test_unknown_shapes_or_ill_typed_patterns_are_incomplete(self):
        self.assertEqual(coverage(Shapes("Missing", {}), ()).status, Status.INCOMPLETE)
        shapes = Shapes("E", {"E": (("A", ()),)})
        self.assertEqual(coverage(shapes, (Row(Pattern("Other")),)).status, Status.INCOMPLETE)
        self.assertEqual(coverage(shapes, (Row(Pattern("A", (Pattern(),))),)).status, Status.INCOMPLETE)


if __name__ == "__main__":
    unittest.main()
