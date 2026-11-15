#!/usr/bin/env python3
"""Reproduce #30 mechanism evidence. Exit 0 is NOT a cleared support gate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import shlex
import subprocess
import tarfile
import urllib.request

HERE = Path(__file__).resolve().parent
ASSETS = {
    "binaryen-version_133-arm64-macos.tar.gz": (
        "WebAssembly/binaryen/version_133", "ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8"),
    "wasmtime-v50.0.1-aarch64-macos-c-api.tar.xz": (
        "bytecodealliance/wasmtime/v50.0.1", "a49327ca596fae51cd92e230265132112bb7896c5a24e4d58cd05b56612e26bb"),
    "wasmtime-v50.0.1-aarch64-macos.tar.xz": (
        "bytecodealliance/wasmtime/v50.0.1", "960e5216c7ebd7e9b45e6561d15821b8b801b7d54aee28c7b91921aeb30c65c8"),
}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fetch(tools):
    tools.mkdir(parents=True, exist_ok=True)
    for name, (release, digest) in ASSETS.items():
        path = tools / name
        if not path.exists():
            repo, tag = release.rsplit("/", 1)
            urllib.request.urlretrieve(f"https://github.com/{repo}/releases/download/{tag}/{name}", path)
        assert sha(path) == digest, f"Archive checksum mismatch: {path}"
        with tarfile.open(path) as archive:
            # Archives are trusted only after checking the pinned digest above.
            assert all(not x.name.startswith("/") and ".." not in Path(x.name).parts
                       for x in archive.getmembers())
            archive.extractall(tools)


def ir(wasm=False):
    cc = "" if wasm else "tailcc "
    text = "declare void @sample_step(i64)\n"
    for name, target, indirect in [("self", "self", False), ("mutual_a", "mutual_b", False),
                                   ("mutual_b", "mutual_a", False), ("indirect", "indirect", True)]:
        extra = ", ptr %fn" if indirect else ""
        callee = "%fn" if indirect else "@" + target
        text += f"""
define {cc}i64 @{name}(i64 %n, i64 %acc{extra}) noinline {{
  call void @sample_step(i64 %n)
  %done = icmp eq i64 %n, 0
  br i1 %done, label %end, label %next
end:
  ret i64 %acc
next:
  %nn = sub i64 %n, 1
  %aa = add i64 %acc, 1
  %r = musttail call {cc}i64 {callee}(i64 %nn, i64 %aa{extra})
  ret i64 %r
}}
"""
    for name, fn in [("self", "self"), ("mutual", "mutual_a"), ("indirect", "indirect")]:
        extra = ", ptr @indirect" if name == "indirect" else ""
        text += f"define i64 @run_{name}(i64 %n) {{\n %r = call {cc}i64 @{fn}(i64 %n, i64 0{extra})\n ret i64 %r\n}}\n"
    params = ", ".join(f"i64 %a{i}" for i in range(1, 15))
    text += f"""
define {cc}i64 @arguments(i64 %n, i64 %acc, {params}) noinline {{
 call void @sample_step(i64 %n)
 %done = icmp eq i64 %n, 0
 br i1 %done, label %end, label %next
end:
 %s0 = add i64 %acc, 0
"""
    for i in range(1, 15):
        text += f" %w{i} = mul i64 %a{i}, {i}\n %s{i} = add i64 %s{i-1}, %w{i}\n"
    text += f""" ret i64 %s14
next:
 %nn = sub i64 %n, 1
 %aa = add i64 %acc, 1
 %r = musttail call {cc}i64 @arguments(i64 %nn, i64 %aa, {params})
 ret i64 %r
}}
define i64 @run_arguments(i64 %n) {{
 %r = call {cc}i64 @arguments(i64 %n, i64 0, {', '.join(f'i64 {i}' for i in range(1, 15))})
 ret i64 %r
}}
"""
    text += f"""
define {cc}void @sret_chain(ptr sret({{i64, i64}}) align 8 %out, i64 %n, i64 %acc) noinline {{
 call void @sample_step(i64 %n)
 %done = icmp eq i64 %n, 0
 br i1 %done, label %end, label %next
end:
 store i64 %acc, ptr %out, align 8
 %second = getelementptr i64, ptr %out, i32 1
 store i64 9, ptr %second, align 8
 ret void
next:
 %nn = sub i64 %n, 1
 %aa = add i64 %acc, 1
 musttail call {cc}void @sret_chain(ptr sret({{i64, i64}}) align 8 %out, i64 %nn, i64 %aa)
 ret void
}}
"""
    if wasm:
        # Root-owned buffer stays live. Only the internal chain is proper-tail.
        text += f"""
define i64 @run_sret(i64 %n) {{
 %out = alloca {{i64, i64}}, align 8
 call {cc}void @sret_chain(ptr sret({{i64, i64}}) align 8 %out, i64 %n, i64 0)
 %r = load i64, ptr %out, align 8
 %second = getelementptr i64, ptr %out, i32 1
 %y = load i64, ptr %second, align 8
 %ok = icmp eq i64 %y, 9
 br i1 %ok, label %good, label %bad
bad:
 call void @llvm.trap()
 unreachable
good:
 ret i64 %r
}}
declare void @llvm.trap()
"""
    else:
        text += f"""
define void @run_sret(i64 %n, ptr %out) {{
 call {cc}void @sret_chain(ptr sret({{i64, i64}}) align 8 %out, i64 %n, i64 0)
 ret void
}}
define {cc}{{i64, i64}} @aggregate(i64 %n, {{i64, i64}} %v) noinline {{
 call void @sample_step(i64 %n)
 %done = icmp eq i64 %n, 0
 br i1 %done, label %end, label %next
end:
 ret {{i64, i64}} %v
next:
 %x = extractvalue {{i64, i64}} %v, 0
 %xx = add i64 %x, 1
 %vv = insertvalue {{i64, i64}} %v, i64 %xx, 0
 %nn = sub i64 %n, 1
 %r = musttail call {cc}{{i64, i64}} @aggregate(i64 %nn, {{i64, i64}} %vv)
 ret {{i64, i64}} %r
}}
define void @run_aggregate(i64 %n, ptr %out) {{
 %r = call {cc}{{i64, i64}} @aggregate(i64 %n, {{i64, i64}} {{i64 0, i64 7}})
 store {{i64, i64}} %r, ptr %out, align 8
 ret void
}}
declare void @observe_local(ptr)
define i64 @ordinary_control(i64 %n) noinline optnone {{
 %slot = alloca i64, align 8
 store i64 %n, ptr %slot, align 8
 call void @observe_local(ptr %slot)
 %done = icmp eq i64 %n, 0
 br i1 %done, label %end, label %next
end:
 ret i64 0
next:
 %nn = sub i64 %n, 1
 %r = call i64 @ordinary_control(i64 %nn)
 call void @observe_local(ptr %slot)
 %answer = add i64 %r, 1
 ret i64 %answer
}}
"""
    return text


def byval_ir():
    text = """declare void @sample_step(i64)
declare void @llvm.trap()
define tailcc i64 @byval_chain(i64 %n, ptr byval([32 x i64]) align 16 %p) noinline {
 call void @sample_step(i64 %n)
 %done = icmp eq i64 %n, 0
 br i1 %done, label %end, label %next
end:
"""
    for i in range(32):
        text += f" %p{i} = getelementptr i64, ptr %p, i32 {i}\n %v{i} = load i64, ptr %p{i}, align 8\n %ok{i} = icmp eq i64 %v{i}, {i+1}\n"
        text += f" %all{i} = and i1 {'true' if i == 0 else '%all'+str(i-1)}, %ok{i}\n"
    text += """ br i1 %all31, label %good, label %bad
good:
 ret i64 42
bad:
 call void @llvm.trap()
 unreachable
next:
 %nn = sub i64 %n, 1
 %r = musttail call tailcc i64 @byval_chain(i64 %nn, ptr byval([32 x i64]) align 16 %p)
 ret i64 %r
}
define i64 @run_byval(i64 %n) {
 %p = alloca [32 x i64], align 16
"""
    text += " store [32 x i64] [" + ", ".join(f"i64 {i}" for i in range(1, 33)) + "], ptr %p, align 16\n"
    return text + """ %r = call tailcc i64 @byval_chain(i64 %n, ptr byval([32 x i64]) align 16 %p)
 ret i64 %r
}
"""

ASYNC_CONTROL = """(module
 (import "env" "pause" (func $pause))
 (memory (export "memory") 2)
 (func $f (export "run") (param $n i32) (result i32)
  local.get $n i32.eqz
  if (result i32) call $pause i32.const 0
  else local.get $n i32.const 1 i32.sub call $f local.get $n i32.add end))
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--llvm", type=Path, default=Path("/opt/homebrew/opt/llvm/bin"))
    ap.add_argument("--linker", type=Path, default=Path("/opt/homebrew/bin/wasm-ld"))
    ap.add_argument("--tools", type=Path, default=Path("/tmp/rill-backend-30-tools"))
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--fetch-tools", action="store_true")
    ap.add_argument("--extended-targets", action="store_true", help="Also execute macOS/x86-64 under Rosetta and Linux through the existing Docker daemon")
    args = ap.parse_args()
    if not __debug__:
        raise RuntimeError("Do not disable the experiment's assertions with python -O")
    assert platform.system() == "Darwin" and platform.machine() == "arm64", "This recorded runner is macOS/AArch64 only"
    if args.fetch_tools:
        fetch(args.tools)
    args.out.mkdir(parents=True, exist_ok=True)
    out = args.out.resolve()
    llvm = args.llvm.resolve()
    linker = args.linker.absolute()  # Preserve wasm-ld argv[0] dispatch through its symlink.
    binaryen = args.tools.resolve() / "binaryen-version_133/bin"
    capi = args.tools.resolve() / "wasmtime-v50.0.1-aarch64-macos-c-api"
    cli = args.tools.resolve() / "wasmtime-v50.0.1-aarch64-macos/wasmtime"
    commands, checks = [], []
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    def run(name, cmd, reject=None):
        cmd = [str(c) for c in cmd]
        print(name, shlex.join(cmd), flush=True)
        p = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           timeout=180, env={**os.environ, "LLVM_DISABLE_CRASH_REPORT": "1"})
        (out / (name + ".stdout")).write_text(p.stdout)
        (out / (name + ".stderr")).write_text(p.stderr)
        good = (p.returncode != 0 and reject in p.stderr) if reject else p.returncode == 0
        commands.append(dict(name=name, argv=cmd, cwd=str(Path.cwd()), exit=p.returncode,
                             expected_rejection=reject, matched_expectation=good))
        (out / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        assert good, f"Unexpected outcome: {name}; see {out / (name + '.stderr')}"
        return p.stdout

    def record(name, cmd, expected=None):
        value = run("version-" + name, cmd)
        if expected:
            assert expected in value, (name, value)
        return value

    record("llvm", [llvm / "llvm-config", "--version"], "23.1.5")
    record("clang", [llvm / "clang", "--version"], "23.1.5")
    record("llc", [llvm / "llc", "--version"])
    record("opt", [llvm / "opt", "--version"])
    record("lld", [linker, "--version"], "23.1.5")
    record("binaryen", [binaryen / "wasm-opt", "--version"], "133")
    record("wasmtime", [cli, "--version"], "50.0.1")
    for name, cmd in [("host", ["uname", "-a"]), ("os", ["sw_vers"]),
                      ("sdk", ["xcrun", "--show-sdk-path"]),
                      ("apple-linker", ["xcrun", "ld", "-version_details"]),
                      ("rust", ["rustc", "--version", "--verbose"]),
                      ("cargo", ["cargo", "--version"]), ("node", ["node", "--version"]),
                      ("brew-llvm", ["brew", "info", "--json=v2", "llvm"])]:
        record(name, cmd)
    native = out / "native.ll"; native.write_text(ir())
    wasm = out / "wasm.ll"; wasm.write_text(ir(True))
    byval = out / "byval.ll"; byval.write_text(byval_ir())
    unsupported = out / "wasm-tailcc.ll"
    # Independent small, legal IR: unsupported target convention, not malformed IR.
    unsupported.write_text("define tailcc i64 @f(i64 %n) {\n %r = musttail call tailcc i64 @f(i64 %n)\n ret i64 %r\n}\n")
    for name, path in [("native", native), ("wasm", wasm), ("byval", byval), ("wasm-tailcc", unsupported)]:
        run("verify-" + name, [llvm / "opt", "-passes=verify", "-disable-output", path])
    for mode in [0, 2]:
        native_opt = out / f"native-O{mode}.ll"
        wasm_opt = out / f"wasm-O{mode}.ll"
        for label, src, dest in [("native", native, native_opt), ("wasm", wasm, wasm_opt)]:
            run(f"opt-{label}-{mode}", [llvm / "opt", f"-passes=default<O{mode}>", "-S", src, "-o", dest])
            run(f"verify-{label}-{mode}", [llvm / "opt", "-passes=verify", "-disable-output", dest])
        for label, triple in [("arm-macos", "arm64-apple-darwin27.0.0"),
                              ("arm-linux", "aarch64-unknown-linux-gnu"),
                              ("x86-macos", "x86_64-apple-darwin"),
                              ("x86-linux", "x86_64-unknown-linux-gnu")]:
            run(f"cross-{label}-{mode}", [llvm / "llc", f"-mtriple={triple}", f"-O={mode}",
                                          native_opt, "-o", out / f"{label}-O{mode}.s"])
            run(f"byval-{label}-{mode}", [llvm / "llc", f"-mtriple={triple}", f"-O={mode}",
                                          byval, "-o", out / f"byval-{label}-O{mode}.s"],
                reject="failed to perform tail call elimination" if label.startswith("arm-") else None)
        if args.extended_targets:
            for label, triple in [("arm-linux", "aarch64-unknown-linux-gnu"), ("x86-linux", "x86_64-unknown-linux-gnu"), ("x86-macos", "x86_64-apple-darwin")]:
                run(f"object-{label}-{mode}", [llvm / "llc", f"-mtriple={triple}", f"-O={mode}", "-filetype=obj", native_opt, "-o", out / f"{label}-O{mode}.o"])
                if label.startswith("x86-"):
                    run(f"byval-object-{label}-{mode}", [llvm / "llc", f"-mtriple={triple}", f"-O={mode}", "-filetype=obj", byval, "-o", out / f"byval-{label}-O{mode}.o"])
            xexe = out / f"native-x86-macos-O{mode}"
            run(f"x86-macos-link-{mode}", [llvm / "clang", "-target", "x86_64-apple-darwin27.0.0", "-isysroot", (out / "version-sdk.stdout").read_text().strip(), f"-O{mode}", "-Wall", "-Wextra", "-Werror", "-pthread", "-DHAVE_BYVAL", HERE / "native.c", out / f"x86-macos-O{mode}.o", out / f"byval-x86-macos-O{mode}.o", "-o", xexe])
            for n in [1024, 1000000]:
                data = run(f"x86-macos-execute-{mode}-{n}", ["arch", "-x86_64", xexe, n])
                checks.extend(dict(target="macOS/x86-64/Rosetta", mode=mode, **json.loads(line)) for line in data.splitlines())
        cleanup = out / f"cleanup-O{mode}"
        run(f"cleanup-link-{mode}", [llvm / "clang", f"-O{mode}", "-Wall", "-Wextra", "-Werror", HERE / "cleanup.c", "-o", cleanup])
        data = run(f"cleanup-execute-{mode}", [cleanup, "normal"])
        checks.extend(dict(target="C model/macOS/AArch64", mode=mode, **json.loads(line)) for line in data.splitlines())
        run(f"cleanup-fatal-{mode}", [cleanup, "fail"], reject="fatal injected worklist allocation failure; no recursive fallback")
        obj = out / f"native-O{mode}.o"
        run(f"native-object-{mode}", [llvm / "llc", "-mtriple=arm64-apple-darwin27.0.0", f"-O={mode}", "-filetype=obj", native_opt, "-o", obj])
        exe = out / f"native-O{mode}"
        run(f"native-link-{mode}", [llvm / "clang", f"-O{mode}", "-Wall", "-Wextra", "-Werror", "-pthread", HERE / "native.c", obj, "-o", exe])
        for n in [1024, 1000000]:
            data = run(f"native-execute-{mode}-{n}", [exe, n])
            checks.extend(dict(target="macOS/AArch64", mode=mode, **json.loads(line)) for line in data.splitlines())
        wobj = out / f"wasm-O{mode}.o"
        run(f"wasm-object-{mode}", [llvm / "llc", "-mtriple=wasm32-unknown-wasip1", "-mattr=+tail-call", f"-O={mode}", "-filetype=obj", wasm_opt, "-o", wobj])
        wbin = out / f"wasm-O{mode}.wasm"
        run(f"wasm-link-{mode}", [linker, "--no-entry", "--allow-undefined", "--export-memory", "-z", "stack-size=1048576",
                                  *[f"--export=run_{s}" for s in ["self", "mutual", "indirect", "arguments", "sret"]], wobj, "-o", wbin])
        run(f"wasm-dis-{mode}", [binaryen / "wasm-dis", wbin, "--enable-tail-call", "-o", out / f"wasm-O{mode}.wat"])
        run(f"asyncify-reject-{mode}", [binaryen / "wasm-opt", wbin, "--enable-tail-call", "--asyncify", "-o", out / f"rejected-O{mode}.wasm"],
            reject="tail calls not yet supported in asyncify")
        host = out / f"wasmtime-host-O{mode}"
        run(f"host-link-{mode}", [llvm / "clang", f"-O{mode}", "-Wall", "-Wextra", "-Werror", "-pthread", HERE / "wasmtime.c",
                                  "-I", capi / "include", "-L", capi / "lib", "-lwasmtime", f"-Wl,-rpath,{capi / 'lib'}", "-o", host])
        for n in [1024, 1000000]:
            data = run(f"wasmtime-execute-{mode}-{n}", [host, wbin, n, mode])
            checks.extend(dict(target="Wasmtime/Cranelift/macOS/AArch64", mode=mode, **json.loads(line)) for line in data.splitlines())
    if args.extended_targets:
        record("rosetta", ["pkgutil", "--pkg-info", "com.apple.pkg.RosettaUpdateAuto"])
        record("docker", ["docker", "version", "--format", "{{json .Server}}"])
        images = [("arm", "linux/arm64", "rust@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137"),
                  ("x86", "linux/amd64", "rust@sha256:d841af61827b29b3efa46f5363e8f2e972b379cc0463c319e8f320d9f0c8eb0c")]
        for arch, docker_platform, image in images:
            script = "set -ex; uname -a; cc --version; ld --version; rustc --version --verbose; getconf GNU_LIBC_VERSION; dpkg-query -W; "
            script += "for mode in 0 2; do "
            extra = f"-DHAVE_BYVAL /out/byval-x86-linux-O$mode.o" if arch == "x86" else ""
            script += f"cc -O$mode -Wall -Wextra -Werror -pthread {extra} /src/native.c /out/{arch}-linux-O$mode.o -o /out/native-{arch}-linux-O$mode; "
            script += f"for n in 1024 1000000; do /out/native-{arch}-linux-O$mode $n > /out/linux-{arch}-O$mode-$n.jsonl; done; done"
            run(f"linux-{arch}-container", ["docker", "run", "--rm", "--network=none", "--ulimit", "core=0:0", "--platform", docker_platform,
                                            "--mount", f"type=bind,src={HERE},dst=/src,readonly", "--mount", f"type=bind,src={out},dst=/out",
                                            "--entrypoint=sh", image, "-c", script])
            for mode in [0, 2]:
                for n in [1024, 1000000]:
                    data = (out / f"linux-{arch}-O{mode}-{n}.jsonl").read_text()
                    checks.extend(dict(target=f"Linux/{docker_platform}/Docker" + ("/emulated" if arch == "x86" else "/VM"), mode=mode, **json.loads(line)) for line in data.splitlines())
    indirect = out / "indirect-only.ll"
    indirect.write_text("define i64 @f(i64 %n, ptr %fn) {\n %r = musttail call i64 %fn(i64 %n, ptr %fn)\n ret i64 %r\n}\n")
    run("verify-indirect-only", [llvm / "opt", "-passes=verify", "-disable-output", indirect])
    run("indirect-only-object", [llvm / "llc", "-mtriple=wasm32-unknown-wasip1", "-mattr=+tail-call", "-O=0", "-filetype=obj", indirect, "-o", out / "indirect-only.o"])
    run("indirect-only-link", [linker, "--no-entry", "--export=f", out / "indirect-only.o", "-o", out / "indirect-only.wasm"])
    run("asyncify-indirect-reject", [binaryen / "wasm-opt", out / "indirect-only.wasm", "--enable-tail-call", "--asyncify", "-o", out / "indirect-rejected.wasm"], reject="tail calls not yet supported in asyncify")
    run("wasm-tailcc-reject", [llvm / "llc", "-mtriple=wasm32-unknown-wasip1", "-mattr=+tail-call", unsupported, "-o", out / "unsupported.s"], reject="calling convention")
    control = out / "asyncify-control.wat"; control.write_text(ASYNC_CONTROL)
    transformed = out / "asyncify-control.wasm"
    run("asyncify-control", [binaryen / "wasm-opt", control, "--asyncify", "--pass-arg=asyncify-imports@env.pause", "-o", transformed])
    leaf = out / "leaf.ll"
    leaf.write_text("define void @leaf() {\n %slot = alloca [64 x i8], align 16\n store volatile i8 7, ptr %slot, align 16\n ret void\n}\n")
    run("verify-leaf", [llvm / "opt", "-passes=verify", "-disable-output", leaf])
    for mode in [0, 2]:
        run(f"leaf-object-{mode}", [llvm / "llc", "-mtriple=wasm32-unknown-wasip1", f"-O={mode}", "-filetype=obj", leaf, "-o", out / f"leaf-O{mode}.o"])
        run(f"leaf-link-{mode}", [linker, "--no-entry", "--export=leaf", "--export=__stack_pointer", "--export-memory", "-z", "stack-size=1048576", out / f"leaf-O{mode}.o", "-o", out / f"leaf-O{mode}.wasm"])
        checked = out / f"leaf-checked-O{mode}.wasm"
        run(f"leaf-check-{mode}", [binaryen / "wasm-opt", out / f"leaf-O{mode}.wasm", "--stack-check", "-o", checked])
        run(f"leaf-dis-{mode}", [binaryen / "wasm-dis", checked, "-o", out / f"leaf-checked-O{mode}.wat"])
        data = run(f"asyncify-budgets-{mode}", ["node", HERE / "asyncify.mjs", transformed, checked])
        checks.extend(dict(target="Binaryen/Node/macOS/AArch64", mode=mode, **json.loads(line)) for line in data.splitlines())
    inputs = list(HERE.glob("*")) + [native, wasm, byval, unsupported, indirect, control, leaf]
    hashes = {str(p): sha(p) for p in inputs if p.is_file()}
    for p in [llvm / "llc", llvm / "opt", llvm / "clang", linker, binaryen / "wasm-opt", cli, capi / "lib/libwasmtime.dylib"]:
        hashes[str(p)] = sha(p)
    report = dict(gate="OPEN: incompatible portable tail+suspension pipeline and incomplete target/budget/runtime evidence",
                  note="Successful assertions reproduce finite standalone probes, NOT Rill implementation or support",
                  archive_pins=ASSETS, file_sha256=hashes, observations=checks,
                  untested=(["Linux and x86-64 execution (enable --extended-targets)"] if not args.extended_targets else ["x86-64 physical-hardware execution (translated/emulated only)"]) + ["Rill source classification/checker",
                            "complete aggregate ABI", "managed pre-write stack/saved-budget checks and probes",
                            "compiler-generated masked cleanup/cause processing and native joins (only an explicit C worklist model exercised)",
                            "native M:N migration and zero-copy borrower leases",
                            "same unshared memory growth while a C async future is pending (API forbids outer-driver access)"])
    (out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Recorded {len(checks)} observations. SUPPORT GATE OPEN. Evidence: {out}")


if __name__ == "__main__":
    main()
