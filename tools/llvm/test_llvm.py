"""Offline fixtures only: never invokes a local LLVM, package manager or build tool."""
import argparse
from contextlib import redirect_stdout
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import tarfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import check
import common
import release
import setup

DIGEST = "0" * 64


class LLVMTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def fixture(self):
        p = self.root / "prefix"
        for directory in ["bin", "lib", "include/llvm-c", "include/llvm/Config"]:
            (p / directory).mkdir(parents=True, exist_ok=True)
        (p / "bin/llvm-config").write_text("fixture, not executable code")
        (p / "bin/llvm-config").chmod(0o755)
        for header in ["llvm-c/Core.h", "llvm-c/Target.h", "llvm-c/TargetMachine.h", "llvm/Pass.h"]:
            (p / "include" / header).write_text("fixture")
        (p / "include/llvm/Config/llvm-config.h").write_text("\n".join(
            f"#define LLVM_VERSION_{key} {value}" for key, value in zip(["MAJOR", "MINOR", "PATCH"], [23, 1, 5])))
        (p / "lib/libLLVMCore.a").write_bytes(b"!<arch>\n")
        (p / common.RECEIPT).write_text(json.dumps(common.pin("linux-x64")))
        self.answers = {
            "--version": "23.1.5", "--prefix": str(p), "--includedir": str(p / "include"),
            "--libdir": str(p / "lib"), "--targets-built": "X86",
            "--host-target": "x86_64-unknown-linux-gnu", "--link-static --shared-mode all": "static",
            "--link-static --libnames all": "libLLVMCore.a", "--link-static --system-libs all": "-lm -ldl",
            "--cflags": "-I" + str(p / "include"), "--ldflags": "-L" + str(p / "lib"),
        }
        return p

    def fake_run(self, args, **kwargs):
        if Path(args[0]).name == "llvm-config":
            return self.answers[" ".join(args[1:])]
        return "LLVM version 23.1.5"

    def pinned(self):
        return patch.dict(setup.ARTIFACTS, {name: (asset, size, DIGEST) for name, (asset, size, _) in setup.ARTIFACTS.items()})

    def inspect(self, prefix):
        with patch.object(check, "run", side_effect=self.fake_run), patch.dict(os.environ, {}, clear=True):
            return check.inspect(prefix, "linux-x64")

    def test_host_matrix_no_fallback(self):
        for system, arch, expected in [("Linux", "x86_64", "linux-x64"), ("Linux", "aarch64", "linux-arm64"),
                                       ("Darwin", "arm64", "macos-arm64"), ("Darwin", "x86_64", "macos-x64")]:
            self.assertEqual(common.host(system, arch), expected)
        for system, arch in [("Windows", "AMD64"), ("Linux", "riscv64"), ("Darwin", "i386")]:
            with self.assertRaises(ValueError):
                common.host(system, arch)
        self.assertEqual(common.pin("macos-x64")["route"], "release-archive")
        self.assertEqual({common.backend_for(host) for host in ["linux-x64", "macos-x64"]}, {"X86"})
        self.assertEqual({common.backend_for(host) for host in ["linux-arm64", "macos-arm64"]}, {"AArch64"})

    def test_musl_rejected(self):
        with patch.object(common, "host", return_value="linux-x64"), patch.object(common.platform, "libc_ver", return_value=("musl", "1.2")):
            with self.assertRaisesRegex(ValueError, "glibc"):
                common.native_host()

    def test_rosetta_rejected(self):
        with patch.object(common, "host", return_value="macos-x64"), patch.object(common.subprocess, "run") as run:
            run.return_value.stdout = "1\n"
            with self.assertRaisesRegex(ValueError, "Rosetta"):
                common.native_host()

    def test_download_hash_and_size(self):
        data = b"fixture bytes"
        for digest, size, succeeds in [(hashlib.sha256(data).hexdigest(), len(data), True),
                                         ("0" * 64, len(data), False), ("0" * 64, len(data) - 1, False)]:
            destination = self.root / "download"
            destination.unlink(missing_ok=True)
            with patch.object(setup.urllib.request, "urlopen", return_value=io.BytesIO(data)) as fetch:
                if succeeds:
                    setup.download(destination, "fixture.xz", size, digest)
                    self.assertEqual(destination.read_bytes(), data)
                    self.assertTrue(fetch.call_args.args[0].startswith(common.BASE))
                else:
                    with self.assertRaises(ValueError):
                        setup.download(destination, "fixture.xz", size, digest)

    def archive(self, entries):
        path = self.root / "test.tar.xz"
        with tarfile.open(path, "w:xz") as archive:
            for name, kind, content in entries:
                info = tarfile.TarInfo(name)
                info.type = kind
                if kind == tarfile.REGTYPE:
                    info.size = len(content)
                    archive.addfile(info, io.BytesIO(content))
                else:
                    info.linkname = content
                    archive.addfile(info)
        return path

    def test_safe_archive_with_internal_links(self):
        archive = self.archive([("prefix/bin/clang", tarfile.REGTYPE, b"fixture"),
                                ("prefix/bin/cc", tarfile.SYMTYPE, "clang"),
                                ("prefix/bin/cxx", tarfile.LNKTYPE, "prefix/bin/clang")])
        tree = setup.extract(archive, self.root / "extract")
        self.assertEqual((tree / "bin/cc").read_bytes(), b"fixture")
        self.assertEqual((tree / "bin/cxx").read_bytes(), b"fixture")

    def test_unsafe_archives(self):
        cases = [[("../outside", tarfile.REGTYPE, b"bad")], [("/absolute", tarfile.REGTYPE, b"bad")],
                 [("prefix/link", tarfile.SYMTYPE, "../../escape")],
                 [("prefix/link", tarfile.LNKTYPE, "/tmp/escape")],
                 [("prefix/fifo", tarfile.FIFOTYPE, "")],
                 [("prefix/link", tarfile.SYMTYPE, "dir"), ("prefix/link/file", tarfile.REGTYPE, b"bad")],
                 [("prefix/a", tarfile.SYMTYPE, "b"), ("prefix/b", tarfile.SYMTYPE, "../../escape")]]
        for i, entries in enumerate(cases):
            with self.subTest(entries=entries), self.assertRaises((ValueError, OSError)):
                setup.extract(self.archive(entries), self.root / f"extract{i}")
        self.assertFalse((self.root / "escape").exists())

    def test_partial_prefix_rejected_before_execution(self):
        p = self.root / "prefix"
        p.mkdir()
        with patch.object(check, "run") as run, self.assertRaisesRegex(ValueError, "receipt"):
            check.inspect(p, "linux-x64")
        run.assert_not_called()

    def test_inspection_evidence_not_acceptance(self):
        facts = self.inspect(self.fixture())
        self.assertEqual(facts["status"], "prefix-inspected")
        self.assertEqual(facts["c_api_smoke"], "not-run")
        for key in ["inkwell", "binding_libffi", "rill_codegen", "provenance_signatures"]:
            self.assertEqual(facts[key], "unverified")

    def test_query_rejections(self):
        p = self.fixture()
        cases = {"--version": "23.1.4", "--libdir": "/opt/homebrew/lib", "--prefix": "/tmp/other",
                 "--targets-built": "X86 AArch64", "--host-target": "aarch64-unknown-linux-gnu",
                 "--link-static --shared-mode all": "shared", "--link-static --libnames all": "libLLVM.so",
                 "--link-static --system-libs all": "-L/opt/homebrew/lib -lz",
                 "--cflags": "-I/usr/local/include", "--ldflags": "-L/tmp/other"}
        for key, invalid in cases.items():
            with self.subTest(key=key), patch.dict(self.answers, {key: invalid}), self.assertRaises(ValueError):
                self.inspect(p)

    def test_header_and_archive_rejections(self):
        p = self.fixture()
        (p / "lib/libLLVMCore.a").write_bytes(b"!<thin>\n")
        with self.assertRaisesRegex(ValueError, "archive"):
            self.inspect(p)
        (p / "lib/libLLVMCore.a").write_bytes(b"!<arch>\n")
        header = p / "include/llvm/Config/llvm-config.h"
        header.write_text(header.read_text().replace("PATCH 5", "PATCH 4"))
        with self.assertRaisesRegex(ValueError, "header version"):
            self.inspect(p)

    def test_escaping_prefix_file(self):
        p = self.fixture()
        external = self.root / "external"
        external.write_bytes(b"!<arch>\n")
        (p / "lib/libLLVMCore.a").unlink()
        (p / "lib/libLLVMCore.a").symlink_to(external)
        with self.assertRaises(ValueError):
            self.inspect(p)

    def test_receipt_and_override_mismatch(self):
        p = self.fixture()
        with patch.dict(os.environ, {"LLVM_SYS_231_PREFIX": "/opt/homebrew/opt/llvm"}), self.assertRaises(ValueError):
            check.inspect(p, "linux-x64")
        (p / common.RECEIPT).write_text(json.dumps(common.pin("linux-arm64")))
        with self.assertRaises(ValueError):
            self.inspect(p)

    def test_release_flags_build_one_static_native_backend(self):
        flags = release.flags("/prefix", "AArch64")
        for expected in ["LLVM_TARGETS_TO_BUILD=AArch64", "LLVM_ENABLE_PROJECTS=", "LLVM_ENABLE_RUNTIMES=",
                         "CMAKE_INSTALL_PREFIX=/prefix", "LLVM_BUILD_LLVM_DYLIB=OFF", "LLVM_LINK_LLVM_DYLIB=OFF",
                         "LLVM_BUILD_TOOLS=ON", "LLVM_INCLUDE_TESTS=OFF"]:
            self.assertIn("-D" + expected, flags)
        for lib in ["ZLIB", "ZSTD", "LIBXML2", "LIBEDIT", "FFI"]:
            self.assertIn(f"-DLLVM_ENABLE_{lib}=OFF", flags)
        self.assertEqual(sum(flag.startswith("-DLLVM_TARGETS_TO_BUILD=") for flag in flags), 1)
        self.assertEqual(release.INSTALL_TARGETS, ("install-llvm-headers", "install-llvm-libraries", "install-llvm-config"))

    def test_release_source_extraction_keeps_only_build_directories(self):
        root = f"llvm-project-{common.VERSION}.src"
        names = [f"{root}/{directory}/file" for directory in release.SOURCE_DIRS] + [f"{root}/clang/file", f"{root}/llvmish/file"]
        archive = self.archive([(name, tarfile.REGTYPE, b"x") for name in names])
        release.extract_source(archive, self.root / "src")
        for directory in release.SOURCE_DIRS:
            self.assertTrue((self.root / "src" / directory / "file").is_file())
        self.assertEqual(sorted(path.name for path in (self.root / "src").iterdir()), sorted(release.SOURCE_DIRS))

    def test_release_archive_round_trips_through_setup_extract(self):
        tree = self.fixture()
        (tree / common.RECEIPT).unlink()
        archive = self.root / "out" / "llvm.tar.xz"
        archive.parent.mkdir()
        digest = release.package(tree, archive)
        self.assertEqual(archive.with_name("llvm.tar.xz.sha256").read_text(), f"{digest}  llvm.tar.xz\n")
        unpacked = setup.extract(archive, self.root / "unpacked")
        self.assertEqual((unpacked / "bin/llvm-config").read_text(), "fixture, not executable code")

    def test_unset_pin_fails_naming_the_pin_before_any_work(self):
        args = argparse.Namespace()
        with patch.object(setup, "download") as download, self.assertRaisesRegex(ValueError, "checksum pin for macos-x64.*unset"):
            setup.provision(args, "macos-x64", self.root / "output")
        download.assert_not_called()
        self.assertFalse((self.root / "output.lock").exists())
        self.assertFalse(self.root.joinpath("output").exists())

    def test_resource_failure_precedes_download(self):
        args = argparse.Namespace()
        with self.pinned(), patch.object(setup.shutil, "disk_usage", return_value=SimpleNamespace(free=0)), \
                patch.object(setup, "download") as download, self.assertRaisesRegex(ValueError, "disk"):
            setup.provision(args, "linux-x64", self.root / "output")
        download.assert_not_called()
        self.assertFalse((self.root / "output.lock").exists())

    def test_failed_install_is_not_accepted(self):
        tree = self.fixture()
        output = self.root / "output"
        args = argparse.Namespace()
        with self.pinned(), patch.object(setup.shutil, "disk_usage", return_value=SimpleNamespace(free=100 * setup.GIB)), \
                patch.object(setup, "download"), patch.object(setup, "extract", return_value=tree), \
                patch.object(setup, "inspect", side_effect=ValueError("version mismatch")), self.assertRaises(ValueError):
            setup.provision(args, "linux-x64", output)
        self.assertFalse(output.exists())
        self.assertFalse((self.root / "output.lock").exists())

    def test_existing_prefix_is_rechecked_not_downloaded(self):
        prefix = self.fixture()
        args = argparse.Namespace()
        with self.pinned():
            (prefix / common.RECEIPT).write_text(json.dumps(common.pin("linux-x64")))
        with self.pinned(), patch.object(check, "run", side_effect=self.fake_run) as run, patch.object(setup, "download") as download, \
                patch.dict(os.environ, {}, clear=True):
            result = setup.provision(args, "linux-x64", prefix)
        self.assertEqual(result["status"], "prefix-inspected")
        self.assertGreater(run.call_count, 1)
        download.assert_not_called()

    def test_bin_path_inspects_before_printing(self):
        prefix = self.fixture()
        stdout = io.StringIO()
        with patch.object(sys, "argv", ["check.py", "--bin-path"]), \
                patch.object(check, "native_host", return_value="linux-x64"), \
                patch.object(check, "prefix_for", return_value=prefix), \
                patch.object(check, "inspect", return_value={}) as inspect, redirect_stdout(stdout):
            check.main()
        inspect.assert_called_once_with(prefix, "linux-x64")
        self.assertEqual(stdout.getvalue(), str(prefix / "bin") + "\n")

    def test_clean_environment_removes_implicit_linker_inputs(self):
        with patch.dict(os.environ, {"LD_PRELOAD": "/opt/homebrew/lib/override.so", "CPATH": "/usr/local/include",
                                     "CMAKE_PREFIX_PATH": "/opt/homebrew", "LDFLAGS": "-lLLVM"}):
            env = common.clean_env()
        for key in ["LD_PRELOAD", "CPATH", "CMAKE_PREFIX_PATH", "LDFLAGS"]:
            self.assertNotIn(key, env)
        self.assertNotIn("homebrew", env["PATH"])

    def test_plan_no_execution_or_writes(self):
        stdout = io.StringIO()
        with patch.object(sys, "argv", ["setup.py", "--plan"]), patch.object(common, "host", return_value="macos-x64"), \
                patch.object(common, "ROOT", self.root), patch.object(setup.subprocess, "run") as run, \
                patch.object(setup, "download") as download, redirect_stdout(stdout):
            setup.main()
        facts = json.loads(stdout.getvalue())
        self.assertEqual(facts["status"], "plan-only")
        self.assertEqual(facts["c_api_smoke"], "not-run")
        self.assertFalse((self.root / "target").exists())
        run.assert_not_called()
        download.assert_not_called()

    def test_dependency_rejections(self):
        for output in ["libLLVM-23.so.1 => /lib/libLLVM-23.so.1", "libLLVM.dylib", "libz.so => not found",
                       "/opt/homebrew/lib/libz.dylib", "/usr/local/lib/libz.so"]:
            with patch.object(check, "run", return_value=output), self.assertRaises(ValueError):
                check.dependencies(Path("/fixture"), "linux-x64")

    def test_smoke_rejects_static_homebrew_input_trace(self):
        facts = self.inspect(self.fixture())
        def traced(command, **kwargs):
            return "/fixture/libLLVMCore.a\n/usr/local/lib/libzstd.a" if "-Wl,-t" in command else ""
        with patch.object(check, "run", side_effect=traced), self.assertRaisesRegex(ValueError, "fallback"):
            check.smoke(Path(facts["prefix"]), "linux-x64", facts)

    def test_smoke_commands_and_evidence(self):
        facts = self.inspect(self.fixture())
        with patch.object(check, "run", return_value="/fixture/libLLVMCore.a") as run, patch.object(check, "dependencies", return_value="libc.so"):
            result = check.smoke(Path(facts["prefix"]), "linux-x64", facts)
        commands = [[str(a) for a in call.args[0]] for call in run.call_args_list]
        link = next(command for command in commands if command[0] == "/usr/bin/c++")
        self.assertIn(facts["prefix"] + "/lib/libLLVMCore.a", link)
        self.assertFalse(any(arg.startswith("-lLLVM") for arg in link))
        self.assertEqual(result["status"], "c-api-static-smoke-passed")
        self.assertEqual(result["inkwell"], "unverified")
        self.assertIn("LLVMTargetMachineEmitToFile", check.SMOKE_C)


if __name__ == "__main__":
    unittest.main()
