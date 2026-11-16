#!/usr/bin/env python3
"""Executed fixture-specific native capacity and private tail ABI candidate. Gate OPEN."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shlex
import subprocess

HERE = Path(__file__).resolve().parent
LLVM = Path('/opt/homebrew/opt/llvm/bin')
TARGETS = [
    ('arm-macos', 'arm64-apple-darwin27.0.0', None, None),
    ('x86-macos', 'x86_64-apple-darwin27.0.0', None, None),
    ('arm-linux', 'aarch64-unknown-linux-gnu', 'linux/arm64',
     'rust@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137'),
    ('x86-linux', 'x86_64-unknown-linux-gnu', 'linux/amd64',
     'rust@sha256:d841af61827b29b3efa46f5363e8f2e972b379cc0463c319e8f320d9f0c8eb0c')]


def tail_ir():
    s = '''declare void @packet_sample(ptr)
declare void @packet_finish(ptr, ptr, ptr)
'''
    sig = 'i64 %n, ptr %packet, ptr %out, ptr %ctx, ptr %fn'
    for name, dest in [('self', '@self'), ('mutual_a', '@mutual_b'),
                       ('mutual_b', '@mutual_a'), ('indirect', '%fn')]:
        s += f'''define tailcc void @{name}({sig}) noinline {{
  call void @packet_sample(ptr %ctx)
  %done = icmp eq i64 %n, 0
  br i1 %done, label %end, label %next
end:
  call void @packet_finish(ptr %packet, ptr %out, ptr %ctx)
  ret void
next:
  %idx = urem i64 %n, 257
  %p = getelementptr i8, ptr %packet, i64 %idx
  %old = load i8, ptr %p
  %new = add i8 %old, 1
  store i8 %new, ptr %p
  %nn = sub i64 %n, 1
  musttail call tailcc void {dest}(i64 %nn, ptr %packet, ptr %out, ptr %ctx, ptr %fn)
  ret void
}}
'''
    s += '''define void @packet_dispatch(i64 %kind, i64 %n, ptr %packet, ptr %out, ptr %ctx) noinline {
  switch i64 %kind, label %a [ i64 1, label %b i64 2, label %c ]
a:
  call tailcc void @self(i64 %n, ptr %packet, ptr %out, ptr %ctx, ptr @self)
  ret void
b:
  call tailcc void @mutual_a(i64 %n, ptr %packet, ptr %out, ptr %ctx, ptr @mutual_a)
  ret void
c:
  call tailcc void @indirect(i64 %n, ptr %packet, ptr %out, ptr %ctx, ptr @indirect)
  ret void
}
'''
    return s


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    assert __debug__
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    out = args.out.resolve(); out.mkdir(parents=True, exist_ok=True)
    commands = []; outcomes = []; audits = []

    def run(name, argv, expected=0, reject=None):
        argv = [str(a) for a in argv]
        print(name, shlex.join(argv), flush=True)
        p = subprocess.run(argv, capture_output=True, text=True, timeout=600,
                           env={**os.environ, 'LLVM_DISABLE_CRASH_REPORT': '1'})
        (out / (name + '.stdout')).write_text(p.stdout)
        (out / (name + '.stderr')).write_text(p.stderr)
        commands.append(dict(name=name, argv=argv, cwd=str(Path.cwd()), exit=p.returncode,
                             expected='nonzero' if reject else expected,
                             matched_expectation=(p.returncode != 0 and reject in p.stderr)
                             if reject else p.returncode == expected))
        (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
        if reject:
            assert p.returncode != 0 and reject in p.stderr, (name, p.returncode, p.stderr[-2000:])
            commands[-1]['expected_rejection'] = reject
            (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
        else:
            assert p.returncode == expected, (name, p.returncode, p.stderr[-2000:])
        return p.stdout

    for name, cmd in [('clang', [LLVM/'clang', '--version']), ('llc', [LLVM/'llc', '--version']),
                      ('opt', [LLVM/'opt', '--version']), ('lld', ['/opt/homebrew/bin/ld.lld', '--version']),
                      ('host', ['uname', '-a']), ('os', ['sw_vers']),
                      ('sdk', ['xcrun', '--show-sdk-path']),
                      ('sdk-version', ['xcrun', '--show-sdk-version']),
                      ('sdk-build', ['xcrun', '--show-sdk-build-version']),
                      ('ld-path', ['xcrun', '--find', 'ld']),
                      ('apple-ld', ['xcrun', 'ld', '-version_details']),
                      ('docker', ['docker', 'version']),
                      ('rosetta', ['pkgutil', '--pkg-info', 'com.apple.pkg.RosettaUpdateAuto'])]:
        run('version-' + name, cmd)
    (out/'incoming-byval.ll').write_text('''define tailcc void @incoming_byval(i64 %n, ptr byval([32 x i64]) align 16 %p) noinline {
  %done = icmp eq i64 %n, 0
  br i1 %done, label %end, label %next
end:
  ret void
next:
  %nn = sub i64 %n, 1
  musttail call tailcc void @incoming_byval(i64 %nn, ptr byval([32 x i64]) align 16 %p)
  ret void
}
''')
    run('verify-incoming-byval', [LLVM/'opt', '-passes=verify', '-disable-output', out/'incoming-byval.ll'])
    (out/'packet.ll').write_text(tail_ir())
    run('verify-packet', [LLVM/'opt', '-passes=verify', '-disable-output', out/'packet.ll'])
    for label, triple, plat, image in TARGETS:
        def docker(cmd):
            return ['docker', 'run', '--rm', '--network=none', '--ulimit', 'core=0:0',
                    '--platform', plat, '--mount', f'type=bind,src={HERE},dst=/src,readonly',
                    '--mount', f'type=bind,src={out},dst=/out', '--entrypoint=sh', image, '-c', cmd]
        if plat:
            run(label+'-versions', docker('set -ex; uname -a; cc --version; ld --version; rustc --version --verbose; getconf GNU_LIBC_VERSION; sha256sum /usr/bin/gcc /usr/bin/ld /lib/*/libc.so.6; dpkg-query -W'))
        for mode in [0, 2]:
            tag = f'{label}-O{mode}'
            run(tag+'-byval-negative', [LLVM/'llc', f'-mtriple={triple}', f'-O={mode}',
                out/'incoming-byval.ll', '-o', out/f'{tag}-byval.s'],
                reject='failed to perform tail call elimination' if label.startswith('arm') else None)
            flags = ['-target', triple, f'-O{mode}', '-ffreestanding', '-fno-builtin',
                     '-fno-stack-protector', '-fno-inline', '-fno-optimize-sibling-calls',
                     '-Wall', '-Wextra', '-Werror']
            for src in ['capacity', 'abi']:
                base = out/f'{tag}-{src}'
                run(f'{tag}-{src}-obj', [LLVM/'clang', *flags, '-fstack-usage', '-c', HERE/f'{src}.c', '-o', str(base)+'.o'])
                run(f'{tag}-{src}-asm', [LLVM/'clang', *flags, '-S', HERE/f'{src}.c', '-o', str(base)+'.s'])
                run(f'{tag}-{src}-ir', [LLVM/'clang', *flags, '-S', '-emit-llvm', HERE/f'{src}.c', '-o', str(base)+'.ll'])
            frames = {}
            for line in (out/f'{tag}-capacity.su').read_text().splitlines():
                loc, size, kind = line.split('\t')
                assert kind == 'static', (tag, line)  # no dynamic or unknown helper accepted
                frames[loc.rsplit(':', 1)[-1]] = int(size)
            assert frames['admission'] < 512
            # Finite acyclic fixture: over-count all possible helpers rather than
            # incorrectly treating .su as inclusive of the entire call subtree.
            small = ['argument_builder', 'aggregate_callee', 'sample', 'memcpy', 'memset']
            padding = 4096 + 128 + 16  # extra realignment, leaf red zone, return/link per node
            headroom = 16384  # no OS/native calls in this admitted subtree
            bound = sum(frames[f] + padding for f in small) + headroom + frames['admission'] + padding
            bound = (bound + 15) & ~15
            huge = frames['oversized'] + frames['sample'] + 3*padding + headroom + frames['admission']
            huge = (huge + 15) & ~15
            # Reject unaccounted native calls, including runtime stack probes.
            asm = (out/f'{tag}-capacity.s').read_text()
            calls = re.findall(r'^\s*(?:bl|callq?)\s+_?([A-Za-z][A-Za-z0-9_]*)', asm, re.M)
            assert set(calls) <= set(frames), (tag, calls)
            ir = (out/f'{tag}-capacity.ll').read_text()
            # External linkage prevents O2 IPA from deleting constant scalar
            # parameters and accidentally removing the stack-argument fixture.
            callee_header = re.search(r'^define[^\n]*@aggregate_callee\([^\n]+', ir, re.M).group()
            aggregate_call = re.search(r'\bcall\b[^\n]*@aggregate_callee\([^\n]+', ir).group()
            assert len(re.findall(r'\bi64\b', callee_header)) >= 12
            assert len(re.findall(r'\bi64\b', aggregate_call)) >= 12
            allowed = set(frames)
            actual = re.findall(r'\bcall\b[^\n]*?@([\w.]+)\(', ir)
            assert all(a in allowed or a.startswith('llvm.') for a in actual), (tag, actual)
            graph = {}
            for fn, body in re.findall(r'^define[^\n]*@([\w.]+)\([^\n]*\)[^\n]*\{(.*?)^\}', ir, re.M | re.S):
                graph[fn] = [c for c in re.findall(r'\bcall\b[^\n]*?@([\w.]+)\(', body) if not c.startswith('llvm.')]
            def visit(fn, path):
                assert fn not in path, ('recursive helper rejected', tag, path, fn)
                for child in graph[fn]: visit(child, [*path, fn])
            for fn in graph: visit(fn, [])
            assert set(graph) == set(frames), (tag, graph.keys(), frames.keys())
            assert not re.search(r'^\s*(?:blr\s|callq?\s+\*)', asm, re.M), 'unknown indirect helper'
            start = re.search(r'^_?admission:', asm, re.M).start()
            thunk = asm[start:].split('-- End function', 1)[0]
            first_call = re.search(r'^\s*(?:bl|callq?)\s', thunk, re.M).start()
            prelude = thunk[:first_call]
            branches = re.findall(r'^\s*(?:b\.(?:lo|hi|ls|hs)|j(?:b|a|be|ae))\s', prelude, re.M)
            assert len(branches) >= 3, (tag, prelude)
            audits.append(dict(target=label, mode=mode, frames=frames, bound=bound,
                               oversized_bound=huge, per_node_extra=padding, headroom=headroom,
                               calls=calls, acyclic_fixture=True, graph=graph,
                               pre_large_call_conditional_branches=len(branches),
                               admission_assembly=thunk, aggregate_ir_header=callee_header,
                               aggregate_ir_call=aggregate_call))
            optll = out/f'{tag}-packet.ll'
            run(tag+'-packet-opt', [LLVM/'opt', f'-passes=default<O{mode}>', '-S', out/'packet.ll', '-o', optll])
            run(tag+'-packet-verify', [LLVM/'opt', '-passes=verify', '-disable-output', optll])
            for typ in ['asm', 'obj']:
                run(tag+'-packet-'+typ, [LLVM/'llc', f'-mtriple={triple}', f'-O={mode}',
                    *(['-filetype=obj'] if typ == 'obj' else []), optll, '-o', out/f'{tag}-packet.{"o" if typ == "obj" else "s"}'])
            tailasm = (out/f'{tag}-packet.s').read_text()
            transfers = {}
            for fn, dest in [('self', 'self'), ('mutual_a', 'mutual_b'), ('mutual_b', 'mutual_a'), ('indirect', None)]:
                pos = re.search(r'^_?'+fn+':', tailasm, re.M).start()
                body = tailasm[pos:].split('-- End function', 1)[0]
                assert not re.search(r'^\s*(?:bl|callq?)\s+_?(?:self|mutual_a|mutual_b|indirect)\b', body, re.M)
                if dest and not (fn == 'self' and mode == 2):
                    assert re.search(r'^\s*(?:b|jmpq?)\s+_?'+dest+r'\b', body, re.M), (tag, fn)
                if not dest:
                    assert re.search(r'^\s*(?:br\s+x\d+|jmpq?\s+\*)', body, re.M), (tag, fn)
                transfers[fn] = [l.strip() for l in body.splitlines() if re.match(r'\s*(?:b(?:\.[a-z]+)?|br|bl|jmpq?|callq?)\s', l)]
            audits[-1]['tail_transfers'] = transfers
            audits[-1]['abi_stack_usage'] = (out/f'{tag}-abi.su').read_text().splitlines()
            for part in ['capacity', 'packet']:
                run(tag+'-'+part+'-disassembly', [LLVM/'llvm-objdump', '-d', out/f'{tag}-{part}.o'])
            run(tag+'-switch', [LLVM/'clang', '-target', triple, '-c', HERE/'switch.S', '-o', out/f'{tag}-switch.o'])
            objects = [f'{tag}-{s}.o' for s in ['capacity', 'abi', 'packet', 'switch']]
            exe = out/tag
            if plat:
                run(tag+'-link', docker('set -ex; cc -O2 -Wall -Wextra -Werror /src/host.c ' +
                    ' '.join('/out/'+o for o in objects) + ' -o /out/'+tag))
                q = run(tag+'-query', docker(f'/out/{tag} query 1048576 {bound} 0'))
            else:
                run(tag+'-link', [LLVM/'clang', '-target', triple, '-O2', '-Wall', '-Wextra', '-Werror',
                    HERE/'host.c', *[out/o for o in objects], '-o', exe])
                prefix = ['arch', '-x86_64'] if label.startswith('x86') else []
                q = run(tag+'-query', [*prefix, exe, 'query', 1048576, bound, 0])
            query = json.loads(q); bootstrap = query['bootstrap']
            assert bootstrap <= 4096 and query['status'] == 0 and query['entered'] == 1
            # x86 O2 shrink-wrapping can sample SP before the thunk pushes.
            # Its complete frame is already counted above. Add at most 15 more
            # bytes so an aligned capacity can meet the admission bound exactly.
            bound += (-bootstrap) % 16
            audits[-1]['bound'] = bound
            audits[-1]['bootstrap'] = bootstrap
            exact = bootstrap + bound
            # Exact means the conservative admission boundary, NOT the last
            # actual touched byte: runtime/alignment slack remains intentional.
            assert exact % 16 == 0
            cases = [('default', 1048576, bound, 0, 0),
                     ('tiny', 4096, bound, 0, 86),
                     ('near', exact+16, bound, 0, 0),
                     ('exact', exact, bound, 0, 0),
                     ('one-over', exact, bound+1, 0, 86),
                     ('one-unit-short', exact-16, bound, 0, 86),
                     ('oversized', 1048576, huge, 0, 86),
                     ('unchecked', 131072, bound, 0, 0),
                     ('zero', 0, bound, 0, 86), ('below-min', 1, bound, 0, 86),
                     ('align-up', 1048561, bound, 0, 0),
                     ('too-large', 1048577, bound, 0, 86),
                     ('u64-overflow', 18446744073709551615, bound, 0, 86),
                     ('align-overflow', 18446744073709551608, bound, 0, 86),
                     *[('abi', 1048576, bound, kind, 0) for kind in range(3)]]
            if plat:
                script = 'set -x\n'
                for i, (case, cap, need, kind, status) in enumerate(cases):
                    script += f'/out/{tag} {case} {cap} {need} {kind} > /out/{tag}-case-{i}.json; s=$?; test "$s" -eq {status} || exit 1\n'
                script += 'exit 0\n'
                run(tag+'-suite', docker(script))
                data = [json.loads((out/f'{tag}-case-{i}.json').read_text()) for i in range(len(cases))]
            else:
                data = []
                for i, (case, cap, need, kind, status) in enumerate(cases):
                    data.append(json.loads(run(f'{tag}-case-{i}', [*prefix, exe, case, cap, need, kind], expected=status)))
            for case, row in zip(cases, data):
                if 'status' in row:
                    assert row['status'] == case[4]
                    assert row['lower_canary'] == (case[0] != 'unchecked')
                    assert row['upper_canary'] and row['ok'] == 1
                    if case[4] == 86: assert row['entered'] == 0
                    if case[0] == 'abi': assert row['samples'] == 1000001 and row['releases'] == 1
                else: assert row['fatal'] == 'invalid capacity' and case[4] == 86
                outcomes.append(dict(target=label, mode=mode, **row))
            (out/'partial-results.json').write_text(json.dumps(outcomes, indent=2)+'\n')
    tools = [LLVM/x for x in ['clang', 'opt', 'llc', 'llvm-objdump']]
    tools += [Path('/opt/homebrew/bin/ld.lld'), Path((out/'version-ld-path.stdout').read_text().strip())]
    hashes = {str(p): sha(p) for p in [*tools, *sorted(HERE.glob('*'))] if p.is_file()}
    artifact_hashes = {p.name: sha(p) for p in sorted(out.iterdir()) if p.is_file() and
                       (p.suffix in ['.s', '.su', '.ll', '.o'] or p.name in [f'{t[0]}-O{m}' for t in TARGETS for m in [0,2]])}
    versions = {p.stem: p.read_text() for p in sorted(out.glob('version-*.stdout'))}
    for arch in ['arm', 'x86']:
        versions[arch+'-linux'] = '\n'.join((out/f'{arch}-linux-versions.stdout').read_text().splitlines()[:24])
    report = dict(gate='OPEN', versions=versions, artifact_sha256=artifact_hashes, scope='finite native fixture and private ABI mechanism candidate only',
                  targets=TARGETS, file_sha256=hashes, frame_audits=audits, observations=outcomes,
                  not_validated=['physical x86 hardware', 'complete Rill ABI/checker/ownership/cleanup',
                                 'production native admission/instrumentation, recursion, dynamic frames, external/native runtime bounds',
                                 'M:N lifecycle/cancellation/suspension/migration', 'signal/altstack fatal path (external scheduler termination instead)'])
    (out/'results.json').write_text(json.dumps(report, indent=2)+'\n')
    print(f'{len(outcomes)} observations; support gate OPEN; {out}')


if __name__ == '__main__':
    main()
