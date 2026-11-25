from contextlib import redirect_stderr, redirect_stdout
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

import check_comments
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


class CommentCheckSourceTests(unittest.TestCase):
    def refused(self, source, **kwargs):
        return [text for _, text in check_comments.check_source(source, **kwargs)]

    def test_plain_comments_are_refused_in_every_position(self):
        source = "// a note\nfn f() {} // trailing\n/* block */\n/*\n multi\n*/\n//// banner\n// TODO: later\n// let x = 1;\n"
        self.assertEqual(len(self.refused(source)), 7)
        self.assertEqual(check_comments.check_source("fn f() {}\n// note\n"), [(2, "// note")])

    def test_each_keep_list_form_passes(self):
        forms = {
            "line license header": "// SPDX-License-Identifier: MIT\n// Copyright 2026 Rill\nfn f() {}\n",
            "block license header": "/* Licensed under the MIT license. */\nfn f() {}\n",
            "doc on a public item": "/// Adds one.\n#[inline]\npub fn f() {}\n",
            "block doc on a public item": "/** Adds one. */\npub struct S;\n",
            "doc on an enum variant": "pub enum E {\n    /// The first.\n    A,\n}\n",
            "doc on a trait member": "pub trait T {\n    /// Runs.\n    fn run(&self);\n}\n",
            "inner doc on a public module": "//! The crate.\npub fn f() {}\n",
            "SAFETY on an unsafe block": "fn f() {\n    // SAFETY: the pointer is live.\n    unsafe { g() };\n}\n",
            "SAFETY on a let": "fn f() {\n    // SAFETY: held.\n    let x = unsafe { g() };\n}\n",
            "multi-line SAFETY on an unsafe impl": "// SAFETY: the cell is\n// only read here.\nunsafe impl Send for S {}\n",
            "bead link": "fn f() {}\n// ri-qlh.18.6\n// ci-snb8\n// ri-abcd12\n",
            "RFC link": "// docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md\nfn f() {}\n",
            "RFC link with anchor": "// docs/rfcs/0007-ownership.md#cleanup\nfn f() {}\n",
            "ADR link": "fn f() {} // docs/adr/0001-record.md\n",
        }
        for name, source in forms.items():
            with self.subTest(form=name):
                self.assertEqual(self.refused(source, inner_public=True), [])

    def test_lookalikes_are_refused(self):
        cases = {
            "doc on a private item": "/// Adds one.\nfn f() {}\n",
            "doc on a crate-visible item": "/// Adds one.\npub(crate) fn f() {}\n",
            "doc on a private field": "pub struct S {\n    /// The count.\n    n: u32,\n}\n",
            "inner doc on a private module": "//! The module.\nfn f() {}\n",
            "SAFETY off an unsafe block": "// SAFETY: fine.\nfn f() {}\n",
            "SAFETY on an unsafe fn": "// SAFETY: fine.\nunsafe fn f() {}\n",
            "license comment after code": "fn f() {}\n// License: MIT\n",
            "bead link with prose": "// see ri-qlh.18.6 for why\nfn f() {}\n",
            "RFC link with prose": "// docs/rfcs/0007-ownership.md says so\nfn f() {}\n",
            "bead id followed by prose": "// ri-qlh is why\nfn f() {}\n",
        }
        for name, source in cases.items():
            with self.subTest(case=name):
                self.assertTrue(self.refused(source), source)

    def test_comment_markers_in_literals_are_not_comments(self):
        source = (
            'fn f() -> &\'static str {\n'
            '    let a = "// not a comment";\n'
            '    let b = r#"/* nor "this" */"#;\n'
            "    let c = '/';\n"
            "    let d = '\\'';\n"
            "    let e = b\"//\";\n"
            '    let g = "esc \\" // still inside";\n'
            '    "x"\n}\n'
        )
        self.assertEqual(self.refused(source), [])

    def test_block_comments_nest(self):
        self.assertEqual(len(self.refused("/* outer /* inner */ still outer */\nfn f() {}\n")), 1)
        self.assertEqual(self.refused("fn f() {}\n/* a /* b */ // c */ fn g() {}\n// ri-abc\n"), ["/* a /* b */ // c */"])

    def test_only_comments_touching_changed_lines_are_reported(self):
        source = "// old\nfn f() {}\n// new\n/*\nspan\n*/\n"
        self.assertEqual(check_comments.check_source(source, changed={3}), [(3, "// new")])
        self.assertEqual(check_comments.check_source(source, changed={5}), [(4, "/*")])
        self.assertEqual(check_comments.check_source(source, changed=set()), [])


class CommentCheckRunTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        git(self.root, "init", "-q", "-b", "main")
        git(self.root, "config", "user.email", "test@example.com")
        git(self.root, "config", "user.name", "Test")
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {}\n")
        git(self.root, "add", ".")
        git(self.root, "commit", "-qm", "base")
        git(self.root, "update-ref", "refs/remotes/origin/main", "HEAD")
        git(self.root, "checkout", "-qb", "work")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit(self):
        git(self.root, "add", ".")
        git(self.root, "commit", "-qm", "change")

    def run_check(self, *argv, env=None):
        out, err = io.StringIO(), io.StringIO()
        with patch.dict(os.environ, env or {}), redirect_stdout(out), redirect_stderr(err):
            code = check_comments.main(list(argv), root=self.root)
        return code, out.getvalue(), err.getvalue()

    def test_a_branch_with_no_new_comment_passes_despite_old_ones(self):
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {}\npub fn g() {}\n")
        self.commit()
        self.assertEqual(self.run_check()[0], 0)

    def test_an_added_plain_comment_fails_with_the_instruction(self):
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {}\n// workaround\nfn g() {}\n")
        self.commit()
        code, _, err = self.run_check()
        self.assertEqual(code, 1)
        self.assertIn("crates/a/src/lib.rs:3: // workaround", err)
        self.assertNotIn("old comment", err)
        self.assertIn("Remove the comment. If it explains a workaround, fix the root cause; "
                      "if it states a rule, write a test or a type; "
                      "if it records a decision, link the bead, RFC or ADR.", err)

    def test_added_keep_list_forms_pass(self):
        self.write("crates/a/src/lib.rs", "// old comment\n/// Adds.\npub fn f() {}\n// ri-abcd\nfn g() {}\n")
        self.commit()
        self.assertEqual(self.run_check()[0], 0)

    def test_an_uncommitted_and_an_untracked_file_are_read(self):
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {} // edit\n")
        self.write("runtime/new.rs", "// fresh\n")
        self.write("docs/note.rs", "// outside the source directories\n")
        code, _, err = self.run_check()
        self.assertEqual(code, 1)
        self.assertIn("crates/a/src/lib.rs:2: // edit", err)
        self.assertIn("runtime/new.rs:1: // fresh", err)
        self.assertNotIn("docs/note.rs", err)

    def test_a_dispatched_run_diffs_against_the_merge_base_with_origin_main(self):
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {}\n// first\n")
        self.commit()
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {}\n// first\n// second\n")
        self.commit()
        code, _, err = self.run_check(env={"GITHUB_BASE_REF": ""})
        self.assertEqual(code, 1)
        self.assertIn(":3: // first", err)
        self.assertIn(":4: // second", err)

    def test_a_pull_request_run_diffs_against_its_base_branch(self):
        git(self.root, "checkout", "-q", "main")
        git(self.root, "checkout", "-qb", "integration")
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {}\n// integrated\n")
        self.commit()
        git(self.root, "update-ref", "refs/remotes/origin/integration", "HEAD")
        git(self.root, "checkout", "-qb", "feature")
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {}\n// integrated\n// mine\n")
        self.commit()
        code, _, err = self.run_check(env={"GITHUB_BASE_REF": "integration"})
        self.assertEqual(code, 1)
        self.assertIn(":4: // mine", err)
        self.assertNotIn("integrated", err)

    def test_a_missing_base_fails_and_names_the_checkout_depth(self):
        git(self.root, "update-ref", "-d", "refs/remotes/origin/main")
        with self.assertRaises(SystemExit) as raised:
            self.run_check()
        self.assertIn("fetch-depth: 0", str(raised.exception))

    def test_an_explicit_base_overrides_origin_main(self):
        self.write("crates/a/src/lib.rs", "// old comment\npub fn f() {}\n// added\n")
        self.commit()
        self.assertEqual(self.run_check("--base", "HEAD")[0], 0)
        self.assertEqual(self.run_check("--base", "HEAD~1")[0], 1)

    def test_tree_mode_reads_every_comment(self):
        code, out, err = self.run_check("--tree")
        self.assertEqual(code, 1)
        self.assertIn("crates/a/src/lib.rs:1: // old comment", err)
        self.write("crates/a/src/lib.rs", "pub fn f() {}\n")
        code, out, _ = self.run_check("--tree")
        self.assertEqual(code, 0)
        self.assertIn("every comment", out)

    def test_inner_docs_are_a_contract_only_on_a_public_module(self):
        self.write("crates/a/src/lib.rs", "pub mod open;\nmod shut;\n")
        self.write("crates/a/src/open.rs", "//! The open module.\n")
        self.write("crates/a/src/shut.rs", "//! The shut module.\n")
        self.commit()
        code, _, err = self.run_check()
        self.assertEqual(code, 1)
        self.assertIn("crates/a/src/shut.rs:1", err)
        self.assertNotIn("open.rs", err)

    def test_the_repository_wires_the_check_into_check_and_ci(self):
        tasks = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())["tasks"]
        self.assertIn("check:comments", tasks["check"]["depends"])
        self.assertEqual(tasks["check:comments"]["run"], "python tools/check_comments.py")
        for name in ["check.yml", "native.yml"]:
            workflow = yaml.load((doctor.ROOT / ".github/workflows" / name).read_text(), Loader=yaml.BaseLoader)
            checkouts = [step for job in workflow["jobs"].values() for step in job["steps"]
                         if step.get("uses", "").startswith("actions/checkout@")]
            self.assertTrue(checkouts)
            for step in checkouts:
                with self.subTest(workflow=name):
                    self.assertEqual(step["with"]["fetch-depth"], "0")


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
                   "rill-runtime": {"rill-collection"},
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
                if name in {"compiler", "ide", "lsp", "rill-collection", "rill-error-storage", "rill-runtime"}:
                    self.assertFalse({"inkwell", "llvm-sys"} & {dep["name"] for dep in package["dependencies"]})


class OwnershipArchiveTests(unittest.TestCase):
    """build.rs builds the first-party archive for every host, and m5 compiles on every host."""

    def test_archive_is_not_gated_to_one_host(self):
        crate = doctor.ROOT / "crates" / "compiler"
        self.assertNotIn("return;", (crate / "build.rs").read_text())
        self.assertNotIn("#![cfg(", (crate / "tests" / "m5.rs").read_text())
        self.assertNotIn("cfg(", (crate / "src" / "loader" / "installed.rs").read_text())

    def test_check_runs_the_zero_test_guard_for_m5(self):
        tasks = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())["tasks"]
        self.assertIn("check:m5", tasks["check"]["depends"])
        self.assertEqual(tasks["check:m5"]["run"],
                         "cargo test --locked --no-default-features -p compiler --test m5 -- --list | grep -c ': test$'")


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
    BUILD_STEP = "Build: Stage the prototypes installation"
    TEST_STEP = "Test: Run the prototype acceptance through the staged rill"
    RELEASE_STEP = "Build and test: Verify release acceptance stops at the unavailable build"
    BUILD_STUB = f'exec "{sys.executable}" tools/unavailable.py build\n'

    def run_step(self, mise_script, step_name, command):
        workflow = yaml.load((doctor.ROOT / ".github/workflows/native.yml").read_text(), Loader=yaml.BaseLoader)
        steps = workflow["jobs"]["native"]["steps"]
        self.assertEqual([step["name"] for step in steps].count(step_name), 1)
        script = next(step["run"] for step in steps if step["name"] == step_name)
        self.assertEqual([step["name"] for step in steps if command in step.get("run", "")], [step_name])
        with tempfile.TemporaryDirectory() as directory:
            mise = Path(directory) / "mise"
            mise.write_text("#!/bin/sh\n" + mise_script)
            mise.chmod(0o755)
            env = {**os.environ, "PATH": f"{directory}{os.pathsep}{os.environ['PATH']}"}
            return subprocess.run(["bash", "-e", "-c", script], cwd=doctor.ROOT,
                                  capture_output=True, text=True, env=env)

    def test_the_prototype_steps_run_the_real_tasks_against_the_llvm_prefix(self):
        workflow = yaml.load((doctor.ROOT / ".github/workflows/native.yml").read_text(), Loader=yaml.BaseLoader)
        steps = workflow["jobs"]["native"]["steps"]
        names = [step["name"] for step in steps]
        for name, command in [(self.BUILD_STEP, "mise run build:prototypes"),
                              (self.TEST_STEP, "mise run test:prototypes")]:
            with self.subTest(step=name):
                [step] = [step for step in steps if step["name"] == name]
                self.assertEqual(step["run"], command)
                self.assertEqual(step["env"]["LLVM_SYS_231_PREFIX"],
                                 "${{ github.workspace }}/target/toolchains/llvm-23.1.5/${{ matrix.llvm }}")
                self.assertEqual(step["env"]["LLVM_SYS_231_STRICT_VERSIONING"], "1")
        self.assertLess(names.index(self.BUILD_STEP), names.index(self.TEST_STEP))
        self.assertFalse([step for step in steps if "test:runtime" in step.get("run", "")])

    def run_release(self, mise_script):
        guard = '[ "$1 $2" != "run verify:release" ] && exit 64\n'
        return self.run_step(guard + mise_script, self.RELEASE_STEP, "mise run verify:release")

    def test_the_release_step_runs_only_on_dispatch(self):
        workflow = yaml.load((doctor.ROOT / ".github/workflows/native.yml").read_text(), Loader=yaml.BaseLoader)
        step = next(step for step in workflow["jobs"]["native"]["steps"] if step["name"] == self.RELEASE_STEP)
        self.assertEqual(step["if"], "github.event_name == 'workflow_dispatch'")

    def test_the_build_stub_after_passing_checks_is_asserted(self):
        result = self.run_release("echo 'checks passed'\n" + self.BUILD_STUB)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_failing_check_still_fails_the_release_step(self):
        for script in ["echo 'check failed' >&2\nexit 1\n", "echo 'check failed' >&2\nexit 2\n",
                       "echo 'build is not implemented in this scaffold' >&2\nexit 1\n"]:
            with self.subTest(script=script):
                self.assertNotEqual(self.run_release(script).returncode, 0)

    def test_a_changed_stub_or_a_successful_release_fails_the_release_step(self):
        for script in ["exit 0\n", "echo 'build exists' >&2\nexit 2\n",
                       "echo 'unrelated is not implemented in this scaffold' >&2\nexit 2\n"]:
            with self.subTest(script=script):
                result = self.run_release(script)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("::error::verify:release no longer stops at the build stub", result.stdout)


class TestTaskTests(unittest.TestCase):
    def test_rust_test_tasks_are_locked_and_llvm_free(self):
        tasks = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())["tasks"]
        self.assertEqual(tasks["test"]["depends"], ["doctor"])
        self.assertEqual(tasks["test"]["run"],
                         "cargo test --locked --no-default-features -p compiler -p ide -p lsp -p cli -p rill-collection -p rill-error-storage -p rill-runtime")
        self.assertEqual(tasks["test:compiler"]["depends"], ["doctor"])
        self.assertEqual(tasks["test:compiler"]["run"],
                         "cargo test --locked -p compiler --no-default-features")

    def test_check_reuses_tests_and_keeps_formatting_and_clippy(self):
        tasks = doctor.tomllib.loads((doctor.ROOT / "mise.toml").read_text())["tasks"]
        self.assertEqual(tasks["check"]["depends"].count("test"), 1)
        self.assertIn("check:rust", tasks["check"]["depends"])
        self.assertEqual(tasks["check:rust"]["run"], [
            "cargo fmt --all -- --check",
            "cargo clippy --locked --no-default-features -p compiler -p ide -p lsp -p cli -p rill-collection -p rill-error-storage -p rill-runtime --all-targets -- -D warnings",
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
