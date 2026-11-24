from contextlib import redirect_stderr, redirect_stdout
import io
from pathlib import Path
import tempfile
import unittest

import doctor
import row_tests

MATRIX = """| Row | Test | Outcome |
| --- | --- | --- |
| O1 | `o01_first` | Checked |
| K3 | none | Checked |
| `C12` | none | Checked |
| Other | none | not a row id |
"""


def run(*argv):
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        code = row_tests.main(list(argv))
    return code, out.getvalue(), err.getvalue()


class RowTestsTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        (self.root / "matrix.md").write_text(MATRIX)
        self.tests = self.root / "tests"
        self.tests.mkdir()

    def write(self, name, text):
        path = self.tests / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def mapping(self):
        rows = row_tests.matrix_rows((self.root / "matrix.md").read_text())
        return row_tests.row_tests(rows, row_tests.source_tests(self.tests))

    def test_rows_are_the_table_rows_whose_first_cell_is_an_id(self):
        rows = row_tests.matrix_rows(MATRIX)
        self.assertEqual(rows, {"O1": "o01", "K3": "k03", "C12": "c12"})

    def test_a_test_belongs_to_a_row_by_fn_name_or_by_file_and_directory_name(self):
        self.write("m5.rs", "#[test]\nfn o01_first() {}\n#[test]\nfn unrelated() {}\n")
        self.write("m5/k03_forms.rs", "#[test]\nfn anything() {}\n")
        self.write("m5/c12/mod.rs", "#[test]\npub fn anything_else() {}\n")
        self.assertEqual(self.mapping(), {
            "O1": ["m5.rs::o01_first"],
            "K3": ["m5/k03_forms.rs::anything"],
            "C12": ["m5/c12/mod.rs::anything_else"],
        })

    def test_a_prefix_matches_a_whole_segment_only(self):
        self.write("m5.rs", "#[test]\nfn o010_other() {}\n#[test]\nfn o1_unpadded() {}\n#[test]\nfn o01x() {}\n")
        self.assertEqual(self.mapping()["O1"], [])

    def test_attributes_around_test_count_and_ignore_does_not(self):
        self.write("m5.rs", (
            '#[cfg(all(target_arch = "aarch64"))]\n#[test]\n#[should_panic]\nfn o01_gated() {}\n'
            "#[test]\n#[ignore]\nfn o01_ignored() {}\n"
            "fn o01_helper() {}\n"
        ))
        self.assertEqual(self.mapping()["O1"], ["m5.rs::o01_gated"])

    def test_check_fails_naming_each_row_without_a_test(self):
        self.write("m5.rs", "#[test]\nfn o01_first() {}\n")
        code, out, err = run("check", str(self.root / "matrix.md"), str(self.tests))
        self.assertEqual(code, 1)
        self.assertIn("row K3 has no test: name one `k03_...`", err)
        self.assertIn("row C12 has no test", err)
        self.assertNotIn("row O1", err)

    def test_check_passes_when_every_row_has_a_test(self):
        self.write("m5.rs", "#[test]\nfn o01_first() {}\n#[test]\nfn k03_x() {}\n#[test]\nfn c12_x() {}\n")
        self.assertEqual(run("check", str(self.root / "matrix.md"), str(self.tests)), (0, "", ""))

    def test_a_matrix_with_no_row_is_an_error_not_a_pass(self):
        (self.root / "matrix.md").write_text("| Row |\n| --- |\n")
        with self.assertRaises(SystemExit):
            run("check", str(self.root / "matrix.md"), str(self.tests))

    def test_map_prints_one_line_per_row(self):
        self.write("m5.rs", "#[test]\nfn o01_first() {}\n")
        code, out, _ = run("map", str(self.root / "matrix.md"), str(self.tests))
        self.assertEqual(code, 0)
        self.assertIn("| O1 | `m5.rs::o01_first` |", out)
        self.assertIn("| K3 | none |", out)


    def test_the_documentation_task_runs_the_check(self):
        tasks = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())["tasks"]
        self.assertIn("python tools/row_tests.py check", tasks["check:docs"]["run"])
        self.assertIn("check:docs", tasks["check"]["depends"])


if __name__ == "__main__":
    unittest.main()
