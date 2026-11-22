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

import yaml

import doctor
from check_docs import anchors, check


def git(root, *args):
    subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True)


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

    def test_checkout_scans_the_commit_not_ignored_directories(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            root.mkdir()
            git(root, "init", "-q")
            git(root, "config", "user.email", "test@example.com")
            git(root, "config", "user.name", "Test")
            (root / ".gitignore").write_text("/worktrees/\n")
            (root / "README.md").write_text("[guide](docs/guide.md#guide)\n[old](old.md)\n")
            (root / "old.md").write_text("# Old\n")
            (root / "docs").mkdir()
            (root / "docs/guide.md").write_text("# Guide\n[home](../README.md)\n")
            (root / "worktrees/other").mkdir(parents=True)
            (root / "worktrees/other/README.md").write_text("[bad](absent.md)\n")
            git(root, "add", ".gitignore", "README.md", "old.md", "docs")
            git(root, "commit", "-qm", "docs")
            export = Path(directory) / "export"
            export.mkdir()
            archive = subprocess.run(["git", "-C", str(root), "archive", "HEAD"],
                                     check=True, capture_output=True).stdout
            subprocess.run(["tar", "-x", "-C", str(export)], input=archive, check=True)
            self.assertEqual(check(export), (3, []))
            self.assertEqual(check(root), (3, []))

            (root / "draft.md").write_text("[bad](absent.md)\n")
            count, errors = check(root)
            self.assertEqual(count, 4)
            self.assertEqual(len(errors), 1)
            self.assertIn("draft.md:1: missing path", errors[0])

            (root / "old.md").unlink()
            count, errors = check(root)
            self.assertEqual(count, 4)
            self.assertEqual(len(errors), 2)
            self.assertIn("README.md:2: missing path: old.md", errors[0])

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
            **{("mise", "which", tool): str(path) for tool, path in paths.items()},
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
        allowed = {"rill-collection": set(), "rill-error-storage": set(),
                   "compiler": {"rill-collection", "rill-error-storage"},
                   "ide": {"compiler"}, "lsp": {"ide"},
                   "codegen-llvm": {"compiler"}, "cli": {"compiler", "lsp", "codegen-llvm"}}
        packages = {package["name"]: package for package in metadata["packages"]}
        self.assertEqual(set(packages), set(allowed))
        for name, package in packages.items():
            with self.subTest(package=name):
                self.assertEqual(package["publish"], [])
                internal = {dep["name"] for dep in package["dependencies"] if dep["name"] in packages}
                self.assertLessEqual(internal, allowed[name])
                if name in {"compiler", "ide", "lsp", "rill-collection", "rill-error-storage"}:
                    self.assertFalse({"inkwell", "llvm-sys"} & {dep["name"] for dep in package["dependencies"]})


class OwnershipArchiveTests(unittest.TestCase):
    """build.rs sets RILL_OWNERSHIP_ARCHIVE only for aarch64-apple-darwin."""

    @staticmethod
    def gates_to_aarch64_macos(line):
        return ("cfg(" in line and "not(" not in line
                and 'target_arch = "aarch64"' in line and 'target_os = "macos"' in line)

    def gated(self, path, lines):
        text = "\n".join(lines)
        if any(line.startswith("#![") and self.gates_to_aarch64_macos(line) for line in lines):
            return True
        for parent in (path.parent.with_suffix(".rs"), path.parent / "mod.rs"):
            if path.name != "mod.rs" and parent.is_file() and parent != path:
                declaration = parent.read_text().splitlines()
                for number, line in enumerate(declaration):
                    if line.rstrip(";").split()[-2:] == ["mod", path.stem] and number \
                            and self.gates_to_aarch64_macos(declaration[number - 1]):
                        return True
        uses = [number for number, line in enumerate(lines) if 'env!("RILL_OWNERSHIP_ARCHIVE")' in line]
        return all(any(self.gates_to_aarch64_macos(line) for line in lines[max(0, number - 3):number])
                   for number in uses)

    def test_archive_users_compile_only_for_aarch64_macos(self):
        crate = doctor.ROOT / "crates" / "compiler"
        users = [path for path in sorted(crate.rglob("*.rs"))
                 if "target" not in path.relative_to(crate).parts and path.name != "build.rs"
                 and 'env!("RILL_OWNERSHIP_ARCHIVE")' in path.read_text()]
        self.assertTrue(users)
        ungated = [str(path.relative_to(doctor.ROOT)) for path in users
                   if not self.gated(path, path.read_text().splitlines())]
        self.assertEqual(ungated, [])


class WorkflowTests(unittest.TestCase):
    def workflows(self):
        paths = sorted((doctor.ROOT / ".github/workflows").glob("*.yml"))
        self.assertTrue(paths, "No GitHub workflows found")
        for path in paths:
            yield path.name, yaml.load(path.read_text(), Loader=yaml.BaseLoader)

    def test_workflow_job_and_step_names_have_type_prefixes(self):
        stage = r"^(Build|Test|Release|Deploy|Operations)(?: and (build|test|release|deploy|operations))?: .+"
        for name, workflow in self.workflows():
            with self.subTest(workflow=name):
                self.assertRegex(workflow["name"], stage)
                for job in workflow["jobs"].values():
                    self.assertRegex(job.get("name", ""), stage)
                    for step in job["steps"]:
                        self.assertRegex(step.get("name", ""), r"^[A-Z][A-Za-z ,]+: .+")

    def test_rust_workflows_use_explicit_ci_homes(self):
        for name, workflow in self.workflows():
            if not any("jdx/mise-action@" in step.get("uses", "")
                       for job in workflow["jobs"].values() for step in job["steps"]):
                continue
            with self.subTest(workflow=name):
                self.assertEqual(workflow.get("env", {}).get("CARGO_HOME"),
                                 "${{ github.workspace }}/target/ci/cargo")
                self.assertEqual(workflow.get("env", {}).get("RUSTUP_HOME"),
                                 "${{ github.workspace }}/target/ci/rustup")


class NativeAcceptanceTests(unittest.TestCase):
    STEP = "Test: Verify native runtime acceptance is unavailable"

    def run_step(self, mise_script):
        workflow = yaml.load((doctor.ROOT / ".github/workflows/native.yml").read_text(), Loader=yaml.BaseLoader)
        steps = workflow["jobs"]["native"]["steps"]
        self.assertEqual([step["name"] for step in steps].count(self.STEP), 1)
        script = next(step["run"] for step in steps if step["name"] == self.STEP)
        self.assertEqual([step["name"] for step in steps if "mise run test:runtime" in step.get("run", "")],
                         [self.STEP])
        with tempfile.TemporaryDirectory() as directory:
            mise = Path(directory) / "mise"
            mise.write_text("#!/bin/sh\n" + mise_script)
            mise.chmod(0o755)
            env = {**os.environ, "PATH": f"{directory}{os.pathsep}{os.environ['PATH']}"}
            return subprocess.run(["bash", "-e", "-c", script], cwd=doctor.ROOT,
                                  capture_output=True, text=True, env=env)

    def test_the_stub_is_asserted_so_the_job_is_not_red_by_design(self):
        result = self.run_step(f'[ "$1 $2" = "run test:runtime" ] && exec "{sys.executable}" tools/unavailable.py test:runtime\nexit 64\n')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_real_runtime_result_or_other_failure_still_fails_the_job(self):
        for script in ["exit 0\n", "echo 'compile error' >&2\nexit 1\n",
                       "echo 'unrelated is not implemented in this scaffold' >&2\nexit 2\n"]:
            with self.subTest(script=script):
                self.assertNotEqual(self.run_step(script).returncode, 0)


class TestTaskTests(unittest.TestCase):
    def test_rust_test_tasks_are_locked_and_llvm_free(self):
        tasks = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())["tasks"]
        self.assertEqual(tasks["test"]["depends"], ["doctor"])
        self.assertEqual(tasks["test"]["run"],
                         "cargo test --locked --no-default-features -p compiler -p ide -p lsp -p cli -p rill-collection -p rill-error-storage")
        self.assertEqual(tasks["test:compiler"]["depends"], ["doctor"])
        self.assertEqual(tasks["test:compiler"]["run"],
                         "cargo test --locked -p compiler --no-default-features")

    def test_check_reuses_tests_and_keeps_formatting_and_clippy(self):
        tasks = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())["tasks"]
        self.assertEqual(tasks["check"]["depends"].count("test"), 1)
        self.assertIn("check:rust", tasks["check"]["depends"])
        self.assertEqual(tasks["check:rust"]["run"], [
            "cargo fmt --all -- --check",
            "cargo clippy --locked --no-default-features -p compiler -p ide -p lsp -p cli -p rill-collection -p rill-error-storage --all-targets -- -D warnings",
        ])


class PrePushHookTests(unittest.TestCase):
    def test_setup_enables_the_tracked_hook(self):
        tasks = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())["tasks"]
        self.assertIn("git config core.hooksPath .githooks", tasks["setup"]["run"])

    def test_hook_runs_the_documentation_check_and_is_executable(self):
        hook = doctor.ROOT / ".githooks/pre-push"
        self.assertTrue(os.access(hook, os.X_OK))
        self.assertIn("mise run check:docs", hook.read_text())

    def test_hook_refuses_a_push_with_a_broken_fragment_and_accepts_a_clean_one(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            (base / "remote").mkdir()
            git(base / "remote", "init", "-q", "--bare")
            work = base / "work"
            work.mkdir()
            git(work, "init", "-q", "-b", "main")
            git(work, "config", "user.email", "test@example.com")
            git(work, "config", "user.name", "Test")
            git(work, "remote", "add", "origin", str(base / "remote"))
            (work / ".githooks").mkdir()
            (work / "tools").mkdir()
            hook = work / ".githooks/pre-push"
            hook.write_text((doctor.ROOT / ".githooks/pre-push").read_text())
            hook.chmod(0o755)
            (work / "tools/check_docs.py").write_text((doctor.ROOT / "tools/check_docs.py").read_text())
            # A stand-in for mise: `mise run check:docs` runs the checker as the real task does.
            bin_dir = base / "bin"
            bin_dir.mkdir()
            mise = bin_dir / "mise"
            mise.write_text(f'#!/bin/sh\n[ "$1 $2" = "run check:docs" ] && exec "{sys.executable}" tools/check_docs.py\nexit 64\n')
            mise.chmod(0o755)
            git(work, "config", "core.hooksPath", ".githooks")
            env = {**os.environ, "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}"}

            def push():
                return subprocess.run(["git", "-C", str(work), "push", "origin", "HEAD:main"],
                                      capture_output=True, text=True, env=env)

            (work / "README.md").write_text("# Title\n[ok](#title)\n")
            git(work, "add", ".")
            git(work, "commit", "-qm", "clean")
            self.assertEqual(push().returncode, 0)

            (work / "README.md").write_text("# Title\n[bad](#absent)\n")
            git(work, "commit", "-qam", "broken")
            refused = push()
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("missing anchor", refused.stderr)


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
