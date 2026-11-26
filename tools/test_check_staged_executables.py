from contextlib import redirect_stderr, redirect_stdout
import io
from pathlib import Path
import subprocess
import tempfile
import unittest

import check_staged_executables as check


class CheckSourceTests(unittest.TestCase):
    def test_a_copy_of_current_exe_is_refused(self):
        self.assertEqual(check.check_source("fn f() {\n    fs::copy(std::env::current_exe().unwrap(), &probe).unwrap();\n}\n"), [2])

    def test_a_copy_of_a_cargo_binary_is_refused(self):
        self.assertEqual(check.check_source('fs::copy(Path::new(env!("CARGO_BIN_EXE_rill")), &to);'), [1])

    def test_a_copy_across_lines_is_refused(self):
        self.assertEqual(check.check_source("std::fs::copy(\n    std::env::current_exe()?,\n    to,\n)?;"), [1])

    def test_a_copy_of_a_binding_from_current_exe_is_refused(self):
        source = "let executable = std::env::current_exe().unwrap();\nfs::copy(&executable, &to).unwrap();\n"
        self.assertEqual(check.check_source(source), [2])

    def test_copies_of_other_files_and_other_uses_of_current_exe_pass(self):
        source = (
            "let executable = std::env::current_exe().unwrap();\n"
            "let directory = executable.parent();\n"
            'fs::copy(repo.join("stdlib").join(name), std.join(name)).unwrap();\n'
            'fs::copy(env!("RILL_OWNERSHIP_ARCHIVE"), to).unwrap();\n'
            "let archive = CELL.get_or_init(|| {\n    let executable = std::env::current_exe().unwrap();\n    executable\n});\n"
            "fs::copy(archive, to).unwrap();\n"
        )
        self.assertEqual(check.check_source(source), [])


class MainTests(unittest.TestCase):
    def run_in(self, files):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        for name, text in files.items():
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_text(text)
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = check.main([], root=root)
        return code, err.getvalue()

    def test_a_raw_copy_outside_the_helper_fails_with_its_location(self):
        code, err = self.run_in({"crates/x/tests/a.rs": "fs::copy(std::env::current_exe().unwrap(), &p);\n"})
        self.assertEqual(code, 1)
        self.assertIn("crates/x/tests/a.rs:1", err)

    def test_the_helper_crate_may_copy(self):
        code, _ = self.run_in({"crates/test-support/src/lib.rs": "fs::copy(from, to);\nfs::copy(std::env::current_exe()?, to);\n"})
        self.assertEqual(code, 0)

    def test_a_clean_tree_passes(self):
        code, _ = self.run_in({"crates/x/tests/a.rs": "stage_current_exe(&p).unwrap();\n"})
        self.assertEqual(code, 0)


if __name__ == "__main__":
    unittest.main()
