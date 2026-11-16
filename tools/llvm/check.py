"""Inspect only the expected prefix; --smoke additionally compiles and runs C-API code."""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile

from common import (TARGETS, VERSION, evidence, inside, native_host, no_fallback,
                    pin, prefix_for, read_receipt, require, run)


def inspect(prefix, name, receipt=True):
    require(prefix.is_dir() and prefix.resolve() == prefix, "missing or symlinked LLVM prefix; run llvm setup explicitly")
    if receipt:
        read_receipt(prefix, name)
    for path in prefix.rglob("*"):
        require(not path.is_symlink() or inside(path, prefix), f"escaping prefix symlink: {path}")
    override = os.environ.get("LLVM_SYS_231_PREFIX")
    require(not override or Path(override).absolute() == prefix, "LLVM_SYS_231_PREFIX conflicts with expected prefix")

    def file(relative):
        path = prefix / relative
        require(path.is_file() and inside(path, prefix), f"missing/escaping prefix file: {path}")
        return path

    config = file("bin/llvm-config")
    def query(*args):
        value = run([config, *args])
        no_fallback(value)
        return value

    require(query("--version") == VERSION, "llvm-config version mismatch")
    for option, relative in [("--prefix", "."), ("--includedir", "include"), ("--libdir", "lib")]:
        require(Path(query(option)) == prefix / relative, f"mixed prefix from {option}")
    for header in ["llvm-c/Core.h", "llvm-c/Target.h", "llvm-c/TargetMachine.h", "llvm/Pass.h"]:
        file("include/" + header)
    header = file("include/llvm/Config/llvm-config.h").read_text()
    for key, number in zip(["MAJOR", "MINOR", "PATCH"], VERSION.split(".")):
        require(re.search(rf"^#define LLVM_VERSION_{key}\s+{number}\s*$", header, re.M), "generated header version mismatch")
    targets = query("--targets-built").split()
    require(set(TARGETS) <= set(targets), "missing LLVM backend targets")
    triple = query("--host-target")
    arch = "x86_64" if name.endswith("x64") else "aarch64"
    require(triple.split("-")[0] in ({"aarch64", "arm64"} if arch == "aarch64" else {arch})
            and ("linux" in triple if name.startswith("linux") else "darwin" in triple), "incompatible LLVM host target")
    versions = {}
    for tool in ["clang", "clang++", "ld.lld", "wasm-ld", "llc", "opt"]:
        path = file("bin/" + tool)
        require(os.access(path, os.X_OK), f"tool is not executable: {tool}")
        output = run([path, "--version"])
        no_fallback(output)
        require(re.search(r"(?<![\d.])23\.1\.5(?![\w.])", output), f"{tool} version mismatch")
        versions[tool] = output
    file("bin/lld")  # Generic lld intentionally refuses invocation; use its drivers above.
    require(query("--link-static", "--shared-mode", "all") == "static", "LLVM static linkage unavailable")
    libraries = query("--link-static", "--libnames", "all").split()
    require(libraries and "libLLVMCore.a" in libraries, "missing static LLVM libraries")
    for library in libraries:
        require(Path(library).name == library and library.endswith(".a"), "non-static LLVM library/fallback")
        with file("lib/" + library).open("rb") as stream:
            require(stream.read(8) == b"!<arch>\n", f"not a self-contained static archive: {library}")
    system = query("--link-static", "--system-libs", "all")
    for flag in shlex.split(system):
        if flag.startswith("/") or flag.startswith("-L"):
            path = Path(flag[2:] if flag.startswith("-L") else flag)
            require(inside(path, prefix), f"external library search path: {flag}; explicit closure required")
        else:
            require(re.fullmatch(r"-l[\w+.-]+|-pthread", flag), f"unexpected system library flag: {flag}")
    flags = {option: query(option) for option in ["--cflags", "--ldflags"]}
    for value in flags.values():
        for flag in shlex.split(value):
            if flag.startswith(("-I", "-L")):
                require(len(flag) > 2 and inside(Path(flag[2:]), prefix), f"mixed prefix compiler/linker flag: {flag}")
    return evidence("prefix-inspected", prefix=str(prefix), version=VERSION, host=name,
                    targets=targets, host_target=triple, libraries=libraries,
                    system_libraries=system, tool_versions=versions, flags=flags,
                    c_api_smoke="not-run", provisioning_pin=pin(name), provenance_signatures="unverified")


# Exercise real C API codegen, not merely a program that prints llvm-config's version.
SMOKE_C = r'''#include <llvm-c/Core.h>
#include <llvm-c/Target.h>
#include <llvm-c/TargetMachine.h>
int main(int argc, char **argv) {
  if (argc != 2 || LLVMInitializeNativeTarget() || LLVMInitializeNativeAsmPrinter()) return 1;
  LLVMModuleRef m = LLVMModuleCreateWithName("installation-smoke");
  LLVMTypeRef i32 = LLVMInt32Type();
  LLVMValueRef f = LLVMAddFunction(m, "answer", LLVMFunctionType(i32, 0, 0, 0));
  LLVMBuilderRef b = LLVMCreateBuilder();
  LLVMPositionBuilderAtEnd(b, LLVMAppendBasicBlock(f, "entry"));
  LLVMBuildRet(b, LLVMConstInt(i32, 42, 0));
  char *triple = LLVMGetDefaultTargetTriple(), *error = 0;
  LLVMTargetRef target;
  if (LLVMGetTargetFromTriple(triple, &target, &error)) return 2;
  LLVMTargetMachineRef tm = LLVMCreateTargetMachine(target, triple, "generic", "",
      LLVMCodeGenLevelDefault, LLVMRelocPIC, LLVMCodeModelDefault);
  if (!tm) return 3;
  LLVMSetTarget(m, triple);
  LLVMTargetDataRef data = LLVMCreateTargetDataLayout(tm);
  char *layout = LLVMCopyStringRepOfTargetData(data);
  LLVMSetDataLayout(m, layout);
  int result = LLVMTargetMachineEmitToFile(tm, m, argv[1], LLVMObjectFile, &error);
  LLVMDisposeMessage(layout); LLVMDisposeTargetData(data); LLVMDisposeMessage(triple);
  LLVMDisposeTargetMachine(tm); LLVMDisposeBuilder(b); LLVMDisposeModule(m);
  return result;
}
'''


def dependencies(executable, name):
    command = ["/usr/bin/otool", "-L"] if name.startswith("macos") else ["/usr/bin/ldd"]
    pending, seen, reports = [Path(executable)], set(), []
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        require(len(seen) <= 128, "runtime dependency inspection limit exceeded")
        output = run([*command, current])
        no_fallback(output)
        require("not found" not in output, "unresolved runtime dependency")
        reports.append(output)
        for line in output.splitlines()[1:]:
            # ldd lists the transitive closure; otool needs non-system dylibs visited.
            paths = re.findall(r"(?:^|\s)(/[^\s()]+)", line)
            for raw in paths:
                path = Path(raw)
                no_fallback(str(path.resolve()))
                if name.startswith("macos") and not raw.startswith(("/usr/lib/", "/System/Library/")):
                    require(path.is_file(), f"unresolved runtime dependency: {raw}")
                    pending.append(path)
            if name.startswith("macos"):
                require(not line.lstrip().startswith("@"), "unresolved loader-relative dependency; explicit closure required")
    return "\n".join(reports)


def smoke(prefix, name, facts, sdk=None):
    sdk_flags = []
    if name.startswith("macos"):
        require(sdk is not None and sdk.is_absolute() and (sdk / "usr/lib/libSystem.tbd").is_file(),
                "macOS smoke requires --sdk with an explicit installed macOS SDK")
        no_fallback(str(sdk))
        sdk_flags = ["-isysroot", str(sdk)]
    with tempfile.TemporaryDirectory(prefix="rill-llvm-smoke-") as temp:
        out = Path(temp)
        (out / "smoke.c").write_text(SMOKE_C)
        (out / "main.c").write_text("extern int answer(void); int main(void) { return answer() == 42 ? 0 : 1; }\n")
        clang = [prefix / "bin/clang", "--no-default-config", *sdk_flags]
        headers = run([*clang, "-H", "-I" + str(prefix / "include"), "-c", out / "smoke.c", "-o", out / "smoke.o"], timeout=120, combined=True)
        no_fallback(headers)
        # Absolute LLVM archives plus linker input tracing also catch static Homebrew
        # system libraries that would be invisible in the executable's loader dependencies.
        linked = run([prefix / "bin/clang++", "--no-default-config", *sdk_flags, out / "smoke.o",
                      *[prefix / "lib" / lib for lib in facts["libraries"]],
                      *shlex.split(facts["system_libraries"]), "-Wl,-t", "-o", out / "smoke"], timeout=180, combined=True)
        no_fallback(linked)
        require("libLLVMCore.a" in linked, "linker did not provide static LLVM input trace")
        deps = dependencies(out / "smoke", name)
        run([out / "smoke", out / "answer.o"])
        no_fallback(run([*clang, "-H", out / "main.c", out / "answer.o", "-Wl,-t", "-o", out / "answer"], timeout=120, combined=True))
        native_deps = dependencies(out / "answer", name)
        run([out / "answer"])
    return dict(facts, status="c-api-static-smoke-passed", c_api_smoke="passed",
                dependency_inspection=deps, generated_program_dependencies=native_deps,
                c_api_header_trace=headers, static_link_input_trace=linked)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--smoke", action="store_true", help="compile/link/run real C-API native codegen (not inkwell/Rill acceptance)")
    mode.add_argument("--bin-path", action="store_true", help="inspect the pinned installation and print only its bin directory")
    parser.add_argument("--sdk", type=Path, help="explicit macOS SDK for --smoke")
    args = parser.parse_args()
    name = native_host()
    prefix = prefix_for(name)
    facts = inspect(prefix, name)
    if args.smoke:
        facts = smoke(prefix, name, facts, args.sdk)
    print(prefix / "bin" if args.bin_path else json.dumps(facts, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f"LLVM check failed: {error}")
