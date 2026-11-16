#!/usr/bin/env python3
"""Execute the fixed-layout native tail ABI candidate. #30 support gate stays OPEN."""
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
CACHE = Path('/tmp/rill-30-lifecycle-tools')
TARGETS = [
    ('arm-macos', 'arm64-apple-darwin27.0.0', None, None, None),
    ('x86-macos', 'x86_64-apple-darwin27.0.0', None, None, None),
    ('arm-linux', 'aarch64-unknown-linux-gnu', 'linux/arm64',
     'rust@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137', 'aarch64-linux-gnu'),
    ('x86-linux', 'x86_64-unknown-linux-gnu', 'linux/amd64',
     'rust@sha256:d841af61827b29b3efa46f5363e8f2e972b379cc0463c319e8f320d9f0c8eb0c', 'x86_64-linux-gnu'),
]
FUNCTIONS = [('self', 3, '@self')]
FUNCTIONS += [(f'mutual_{i}', i, f'@mutual_{(i + 1) % 3}') for i in range(3)]
FUNCTIONS += [(f'indirect_{i}', i, '%target') for i in range(3)]


def tail_ir(triple):
    sig = 'i64 %n, ptr %cur, ptr %staging, ptr %out, ptr %ctx'
    s = f'''target triple = "{triple}"
@targets = private constant [3 x ptr] [ptr @indirect_0, ptr @indirect_1, ptr @indirect_2]
declare void @abi_observe(ptr, ptr, i64)
declare i64 @abi_step(ptr, ptr, ptr)
declare void @abi_finish(ptr, ptr, ptr)
'''
    for name, expected, target in FUNCTIONS:
        s += f'''define tailcc void @{name}({sig}) noinline {{
  call void @abi_observe(ptr %cur, ptr %ctx, i64 {expected})
  %done = icmp eq i64 %n, 0
  br i1 %done, label %finish, label %next
finish:
  call void @abi_finish(ptr %cur, ptr %out, ptr %ctx)
  ret void
next:
  %layout = call i64 @abi_step(ptr %cur, ptr %staging, ptr %ctx)
  %nn = sub i64 %n, 1
'''
        if target == '%target':
            s += '''  %address = getelementptr [3 x ptr], ptr @targets, i64 0, i64 %layout
  %target = load volatile ptr, ptr %address, align 8
'''
        s += f'''  musttail call tailcc void {target}(i64 %nn, ptr %staging, ptr %cur, ptr %out, ptr %ctx)
  ret void
}}
'''
    s += '''define void @abi_dispatch(i64 %mode, i64 %n, ptr %a, ptr %b, ptr %out, ptr %ctx) noinline {
  switch i64 %mode, label %self [i64 1, label %mutual i64 2, label %indirect]
self:
  call tailcc void @self(i64 %n, ptr %a, ptr %b, ptr %out, ptr %ctx)
  ret void
mutual:
  call tailcc void @mutual_0(i64 %n, ptr %a, ptr %b, ptr %out, ptr %ctx)
  ret void
indirect:
  call tailcc void @indirect_0(i64 %n, ptr %a, ptr %b, ptr %out, ptr %ctx)
  ret void
}
'''
    return s


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def audit_graph(ir, assembly, darwin):
    bodies = dict(re.findall(r'^define[^\n]*@([\w.$]+)\([^\n]*\)[^\n]*\{(.*?)^\}', ir, re.M | re.S))
    graph = {}
    for name, body in bodies.items():
        graph[name] = re.findall(r'\bcall\b[^\n]*?@([\w.$]+)\(', body)
    seen = set()

    def visit(name, path):
        assert name not in path, ('ordinary helper recursion', path, name)
        if name.startswith('llvm.'):
            assert name.startswith(('llvm.memcpy.', 'llvm.memset.', 'llvm.lifetime.', 'llvm.assume')), name
            return
        assert name in bodies, ('unaccounted external helper', name)
        assert not re.search(r'\bcall\b[^\n]*%[\w.]+\(', bodies[name]), ('indirect per-step helper', name)
        seen.add(name)
        for child in graph[name]:
            visit(child, [*path, name])

    for root in ['abi_observe', 'abi_step', 'memcpy', 'memset', 'bzero', '__bzero']:
        visit(root, [])
    # Machine-code lowering must not introduce an unaccounted call either.
    machine_calls = {}
    for name in seen:
        header = re.search(r'^define[^\n]*@' + re.escape(name) + r'\([^\n]*#(\d+)', ir, re.M)
        assert header, name
        attrs = re.search(r'^attributes #' + header[1] + r' = .*$', ir, re.M)
        assert attrs and '"disable-tail-calls"="true"' in attrs[0], name
        match = re.search(r'^_?' + re.escape(name) + r':', assembly, re.M)
        assert match, name
        body = assembly[match.start():].split('-- End function', 1)[0]
        calls = re.findall(r'^\s*(?:bl|callq?)\s+([\w.$]+)', body, re.M)
        if darwin:
            calls = [x.removeprefix('_') for x in calls]
        assert not re.search(r'^\s*(?:blr\s|callq?\s+\*)', body, re.M), name
        assert set(calls) <= seen, (name, calls, seen)
        machine_calls[name] = calls
    return dict(per_transfer_acyclic_graph={n: graph[n] for n in sorted(seen)},
                machine_calls=machine_calls, no_per_transfer_allocation=True,
                all_external_transfer_helpers_rejected=True)


def audit_tails(text, mode):
    result = {}
    for name, _, dest in FUNCTIONS:
        match = re.search(r'^_?' + name + ':', text, re.M)
        assert match, name
        body = text[match.start():].split('-- End function', 1)[0]
        assert not re.search(r'^\s*(?:bl|callq?)\s+_?(?:self|mutual_\d|indirect_\d)\b', body, re.M), name
        if dest == '%target':
            assert re.search(r'^\s*(?:br\s+x\d+|jmpq?\s+\*)', body, re.M), (name, body)
        elif not (name == 'self' and mode == 2):
            assert re.search(r'^\s*(?:b|jmpq?)\s+_?' + dest[1:] + r'\b', body, re.M), (name, body)
        else:
            # O2 is allowed to turn self-tail recursion into a real local loop.
            assert re.search(r'^\s*(?:b|jmpq?)\s+\.?L', body, re.M), body
        result[name] = [x.strip() for x in body.splitlines()
                        if re.match(r'\s*(?:b(?:\.[a-z]+)?|br|bl|jmpq?|callq?)\s', x)]
    return result


def main():
    assert __debug__, 'Run with Python assertions enabled'
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    out = args.out.resolve()
    assert not out.is_relative_to(HERE), 'Raw artifacts belong outside the checkout'
    out.mkdir(parents=True, exist_ok=True)
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    commands, rows, audits, headers = [], [], {}, {}

    def run(name, argv):
        argv = [str(x) for x in argv]
        print(name, shlex.join(argv), flush=True)
        p = subprocess.run(argv, capture_output=True, text=True, timeout=600,
                           env={**os.environ, 'LLVM_DISABLE_CRASH_REPORT': '1'})
        (out / (name + '.stdout')).write_text(p.stdout)
        (out / (name + '.stderr')).write_text(p.stderr)
        commands.append(dict(name=name, argv=argv, cwd=str(Path.cwd()), exit=p.returncode))
        (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
        assert p.returncode == 0, (name, p.returncode, p.stderr[-6000:])
        return p.stdout

    for name, cmd in [
        ('clang', [LLVM/'clang++', '--version']), ('opt', [LLVM/'opt', '--version']),
        ('llc', [LLVM/'llc', '--version']), ('objdump', [LLVM/'llvm-objdump', '--version']),
        ('host', ['uname', '-a']), ('os', ['sw_vers']),
        ('sdk', ['xcrun', '--show-sdk-path']), ('sdk-version', ['xcrun', '--show-sdk-version']),
        ('sdk-build', ['xcrun', '--show-sdk-build-version']),
        ('apple-ld', ['xcrun', 'ld', '-version_details']), ('ld-path', ['xcrun', '--find', 'ld']),
        ('docker', ['docker', 'version', '--format', '{{json .Server}}']),
        ('rosetta', ['pkgutil', '--pkg-info', 'com.apple.pkg.RosettaUpdateAuto']),
    ]:
        run('version-' + name, cmd)
    assert '23.1.5' in (out/'version-clang.stdout').read_text()
    sdk = (out/'version-sdk.stdout').read_text().strip()
    for label, triple, platform, image, abi in TARGETS:
        def docker(script):
            return ['docker', 'run', '--rm', '--network=none', '--ulimit', 'core=0:0',
                    '--platform', platform, '--mount', f'type=bind,src={out},dst=/out',
                    '--entrypoint=sh', image, '-c', script]
        if platform:
            run('version-' + label, docker('set -ex; uname -a; c++ --version; ld --version; '
                'rustc --version --verbose; getconf GNU_LIBC_VERSION; '
                'sha256sum /usr/bin/g++ /usr/bin/ld /lib/*/libc.so.6 /usr/lib/*/libstdc++.so.6; '
                'dpkg-query -W libc6 libstdc++6'))
        for mode in [0, 2]:
            tag = f'{label}-O{mode}'
            base = out/tag
            flags = ['--target=' + triple, '-std=c++20', f'-O{mode}', '-fno-builtin',
                     '-fno-exceptions', '-fno-rtti', '-fno-stack-protector',
                     '-fno-optimize-sibling-calls', '-Wall', '-Wextra', '-Werror']
            if platform:
                root = CACHE/('headers-' + label.split('-')[0])
                assert (root/'usr/include/c++/14/new').is_file(), root
                flags += ['--sysroot=' + str(root), '-nostdinc++']
                for inc in [root/'usr/include/c++/14', root/'usr/include'/abi/'c++/14',
                            root/'usr/include/c++/14/backward', root/'usr/include'/abi]:
                    flags += ['-isystem', str(inc)]
            else:
                flags += ['-isysroot', sdk]
            run(tag+'-body', [LLVM/'clang++', *flags, '-fstack-usage', '-MD', '-MF', str(base)+'.d',
                              '-c', HERE/'mechanism.cpp', '-o', str(base)+'.o'])
            for suffix, extra in [('ll', ['-emit-llvm']), ('s', [])]:
                run(tag+'-body-'+suffix, [LLVM/'clang++', *flags, '-S', *extra,
                                         HERE/'mechanism.cpp', '-o', str(base)+'.'+suffix])
            body_ir = Path(str(base)+'.ll')
            run(tag+'-verify-body', [LLVM/'opt', '-passes=verify', '-disable-output', body_ir])
            source = out/(tag+'-tail-source.ll')
            source.write_text(tail_ir(triple))
            run(tag+'-verify-tail-source', [LLVM/'opt', '-passes=verify', '-disable-output', source])
            optimized = out/(tag+'-tail.ll')
            run(tag+'-opt-tail', [LLVM/'opt', f'-passes=default<O{mode}>', '-S', source, '-o', optimized])
            run(tag+'-verify-tail', [LLVM/'opt', '-passes=verify', '-disable-output', optimized])
            assert 'alloca ' not in optimized.read_text() and 'byval' not in optimized.read_text()
            for ext, extra in [('s', []), ('o', ['-filetype=obj'])]:
                run(tag+'-tail-'+ext, [LLVM/'llc', '-mtriple='+triple, f'-O={mode}',
                    '-relocation-model=pic', *extra, optimized, '-o', out/(tag+'-tail.'+ext)])
            audits[tag] = audit_graph(body_ir.read_text(), Path(str(base)+'.s').read_text(), not platform)
            audits[tag]['tail_branches'] = audit_tails((out/(tag+'-tail.s')).read_text(), mode)
            stack_usage = Path(str(base)+'.su').read_text().splitlines()
            audits[tag]['body_stack_usage'] = stack_usage
            frames = {line.split('\t')[0].rsplit(':', 1)[-1]: line.split('\t')[1:] for line in stack_usage}
            for name in audits[tag]['per_transfer_acyclic_graph']:
                assert frames[name][1] == 'static', (tag, name, frames[name])
                assert int(frames[name][0]) <= 512, (tag, name, frames[name])
            # NaN/-0 transport uses bitwise checks/copies, not FP arithmetic.
            assert not re.search(r'\b(?:fadd|fsub|fmul|fdiv|frem|fcmp)\b', body_ir.read_text())
            deps = shlex.split(Path(str(base)+'.d').read_text().replace('\\\n', ' ').split(':', 1)[1])
            headers[tag] = {p: sha(p) for p in deps}
            for part in ['', '-tail']:
                run(tag+part+'-disassemble', [LLVM/'llvm-objdump', '-d', out/(tag+part+'.o')])
            exe = out/(tag+'-exe')
            objects = [Path(str(base)+'.o'), out/(tag+'-tail.o')]
            if platform:
                script = 'set -ex; c++ ' + ' '.join('/out/'+x.name for x in objects)
                script += f' -o /out/{exe.name}; /out/{exe.name}'
                data = run(tag+'-link-execute', docker(script))
            else:
                run(tag+'-link', [LLVM/'clang++', '--target='+triple, '-isysroot', sdk, *objects, '-o', exe])
                data = run(tag+'-execute', [*(['arch', '-x86_64'] if label.startswith('x86') else []), exe])
            run(tag+'-exe-disassemble', [LLVM/'llvm-objdump', '-d', exe])
            observed = [json.loads(x) for x in data.splitlines() if x.startswith('{')]
            assert len(observed) == 3 and [x['mode'] for x in observed] == [0, 1, 2]
            for row in observed:
                assert row['ok'] and row['transfers'] == 1000000 and row['samples'] == 1000001
                assert row['moves'] == 1000001 and row['edges'] == [333334, 333333, 333333]
                assert row['capture_frees'] == row['buffer_frees'] == row['copy_frees'] == 1
                assert row['sp_span'] <= 4096 and row['sampled_depth'] <= 8192
                assert row['two_packet_bytes'] <= 1024 and row['result_bytes'] <= 256
                rows.append(dict(target=label, optimization=mode,
                    execution='Rosetta' if label == 'x86-macos' else 'translated Linux VM' if label == 'x86-linux'
                    else 'ARM Linux VM' if platform else 'Apple ARM64', **row))
            (out/'partial-results.json').write_text(json.dumps(rows, indent=2)+'\n')
    header_manifest = out/'header-sha256.json'
    header_manifest.write_text(json.dumps(headers, sort_keys=True, indent=2)+'\n')
    versions = {p.stem: p.read_text() for p in sorted(out.glob('version-*.stdout'))}
    artifacts = {p.name: sha(p) for p in sorted(out.iterdir()) if p.is_file() and
                 (p.suffix in ['.ll', '.s', '.o', '.su', '.d'] or p.name.endswith('-exe'))}
    report = dict(gate='OPEN', scope='Executed fixed-layout private native ABI mechanism candidate only',
        targets=TARGETS, llvm_source_commit='392dfff97d29b59d77e5cac6d4b6ef2585fe7117',
        versions=versions, file_sha256={str(p): sha(p) for p in
            [HERE/'mechanism.cpp', HERE/'run.py', Path((out/'version-ld-path.stdout').read_text().strip()),
             *[LLVM/n for n in ['clang++', 'opt', 'llc', 'llvm-objdump']]]},
        header_manifest_sha256=sha(header_manifest), artifact_sha256=artifacts,
        audits=audits, observations=rows,
        limits=['No physical x86 execution', 'No capacity validation or managed NativeStack in this fixture',
                'malloc/free/printf and final capture invocation are outside the per-transfer acyclic graph; no all-runtime-call bound',
                'No production ABI, source eligibility/ownership/drop checker, compiler, target support closure or adoption',
                'No arbitrary/dynamic/overlapping layouts, escaping packets, generic indirect ABI, public interop, suspension or lifecycle validation'])
    (out/'results.json').write_text(json.dumps(report, indent=2)+'\n')
    print(f'{len(commands)} successful commands; {len(rows)} executed chains; gate OPEN; {out}')


if __name__ == '__main__':
    main()
