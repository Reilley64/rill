from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import doctor
from check_docs import anchors, check


class DocumentationTests(unittest.TestCase):
    def test_real_destinations_and_code_examples(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "guide.md").write_text('# A title\n\n<a id="old-title"></a>\n## Repeat\n## Repeat\n')
            (root / "README.md").write_text(
                '[`guide`](guide.md#a-title)\n[old](guide.md#old-title)\n'
                '[repeat](guide.md#repeat-1)\n[ref]: guide.md#a-title\n'
                '`[example](not-a-file.md)`\n```md\n[fenced](also-absent.md)\n```\n'
            )
            count, errors = check(root)
            self.assertEqual(count, 4)
            self.assertEqual(errors, [])

    def test_broken_destinations_are_reported(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "README.md").write_text('# Title\n[missing](absent.md)\n[anchor](#absent)\n')
            _, errors = check(root)
            self.assertEqual(len(errors), 2)
            self.assertIn("missing path", errors[0])
            self.assertIn("missing anchor", errors[1])

    def test_headings_in_nested_fences_are_not_anchors(self):
        self.assertEqual(anchors('````md\n```\n# Not an anchor\n```\n````\n# Actual\n'), {"actual"})


class ToolSelectionTests(unittest.TestCase):
    def check(self, *, uv_version=None, override=None, wrong_path=False):
        config = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())
        tools = config["tools"]
        rust = tools["core:rust"]["version"]
        uv = tools["aqua:astral-sh/uv"]
        root = Path("/fixture/mise")
        paths = {tool: root / "bin" / tool for tool in ["rustc", "cargo", "uv"]}
        answers = {
            ("mise", "--version"): config["min_version"],
            ("mise", "where", "core:python@" + tools["core:python"]): str(root),
            ("rustc", "--version"): f"rustc {rust} (fixture)",
            ("rustup", "which", "--toolchain", rust, "rustc"): str(paths["rustc"]),
            ("rustc", "--print", "sysroot"): str(root),
            ("cargo", "--version"): f"cargo {rust} (fixture)",
            ("uv", "--version"): f"uv {uv_version or uv} (fixture)",
            ("mise", "which", "uv", "--tool", "aqua:astral-sh/uv@" + uv): str(paths["uv"]),
        }
        with patch.object(doctor, "capture", side_effect=lambda *args: answers[args]), \
                patch.object(doctor.platform, "system", return_value="Linux"), \
                patch.object(doctor.platform, "machine", return_value="x86_64"), \
                patch.object(doctor.platform, "python_version", return_value=tools["core:python"]), \
                patch.object(doctor.sys, "executable", str(root / "bin/python")), \
                patch.object(doctor.shutil, "which", side_effect=lambda tool: str(Path("/ambient") / tool if wrong_path else paths[tool])), \
                patch.dict(os.environ, {"RUSTC_WRAPPER": override or "", "RUSTC": "", "RUSTC_WORKSPACE_WRAPPER": ""}), \
                redirect_stdout(io.StringIO()):
            doctor.check()

    def test_selected_tools_pass(self):
        self.check()

    def test_shadowed_uv_version_fails(self):
        with self.assertRaisesRegex(ValueError, "uv .* is required"):
            self.check(uv_version="0.0.0")

    def test_matching_version_outside_selected_path_fails(self):
        with self.assertRaisesRegex(ValueError, "outside the selected mise"):
            self.check(wrong_path=True)

    def test_cargo_compiler_wrapper_fails(self):
        with self.assertRaisesRegex(ValueError, "compiler overrides"):
            self.check(override="/ambient/wrapper")


class WorkspaceTests(unittest.TestCase):
    def test_internal_dependency_direction_and_private_packages(self):
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=doctor.ROOT, text=True))
        allowed = {"compiler": set(), "ide": {"compiler"}, "lsp": {"ide"},
                   "codegen-llvm": {"compiler"}, "cli": {"compiler", "lsp", "codegen-llvm"},
                   "runtime": set()}
        packages = {package["name"]: package for package in metadata["packages"]}
        self.assertEqual(set(packages), set(allowed))
        for name, package in packages.items():
            with self.subTest(package=name):
                self.assertEqual(package["publish"], [])
                internal = {dep["name"] for dep in package["dependencies"] if dep["name"] in packages}
                self.assertLessEqual(internal, allowed[name])
                if name in {"compiler", "ide", "lsp"}:
                    self.assertFalse({"inkwell", "llvm-sys"} & {dep["name"] for dep in package["dependencies"]})


class ScaffoldStatusTests(unittest.TestCase):
    def test_product_suites_cannot_pass_as_empty_checks(self):
        script = Path(__file__).with_name("unavailable.py")
        for name in ["build", "test:language", "test:stdlib", "test:runtime", "test:lsp-client"]:
            with self.subTest(task=name):
                result = subprocess.run([sys.executable, script, name], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(result.stdout, "")
                self.assertIn("not implemented", result.stderr)
                self.assertIn(name, result.stderr)


if __name__ == "__main__":
    unittest.main()
