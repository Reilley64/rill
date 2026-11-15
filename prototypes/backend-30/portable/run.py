#!/usr/bin/env python3
"""Standalone explicit-record candidate. Successful probes are NOT adoption."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import resource
import shlex
import subprocess

HERE=Path(__file__).resolve().parent
FLAGS=['--target=wasm32-unknown-wasip1','-ffreestanding','-fno-builtin',
       '-fno-strict-aliasing','-fno-stack-protector','-fstack-usage',
       '-mno-tail-call','-mno-bulk-memory','-mno-reference-types',
       '-mno-multivalue','-mno-sign-ext','-mno-nontrapping-fptoint',
       '-Wall','-Wextra','-Werror']
EXPORTS=['init','poll','wake','request_cancel','drop_handle','complete_foreign',
         'canaries','inspect','test_reserve','test_blocked_tail']


def analyze(wat,su):
    # Binaryen's generated WAT only: parentheses/quoted tokens, line comments.
    tokens=re.findall(r'"(?:[^"\\]|\\.)*"|\(|\)|[^\s()]+',re.sub(r';;[^\n]*','',wat))
    stack=[]; roots=[]
    for t in tokens:
        if t=='(':
            node=[]
            (stack[-1] if stack else roots).append(node); stack.append(node)
        elif t==')': stack.pop()
        else: stack[-1].append(t)
    assert not stack and len(roots)==1 and roots[0][0]=='module'
    module=roots[0][1:]
    functions={n[1]:n for n in module if isinstance(n,list) and n and n[0]=='func'}
    imports={}
    for n in module:
        if isinstance(n,list) and n and n[0]=='import':
            assert n[3][0]=='func',n
            imports[n[3][1]]=n[1:3]
    assert list(imports.values())==[['"env"','"fatal"']],imports
    table=set()
    for n in module:
        if isinstance(n,list) and n and n[0]=='elem':
            table.update(t for t in n[1:] if isinstance(t,str) and t in functions)
    assert table and len(table)<=6,table
    graph={name:set() for name in functions}
    has_global_get=set()
    def visit(node,name):
        if not isinstance(node,list) or not node:return
        op=node[0]
        assert op not in ('return_call','return_call_indirect','return_call_ref','call_ref'),op
        assert not (isinstance(op,str) and op.startswith('table.')),('mutable dispatch table',op)
        if op=='call':graph[name].add(node[1])
        if op=='call_indirect':graph[name].update(table)
        if op=='global.get':has_global_get.add(name)
        for c in node:
            if isinstance(c,list):visit(c,name)
    for name,node in functions.items():visit(node,name)
    visiting=set(); done=set()
    def dfs(name):
        if name in imports:return
        assert name in functions,name
        assert name not in visiting,('recursive machine call graph',name)
        if name in done:return
        visiting.add(name)
        for callee in graph[name]:dfs(callee)
        visiting.remove(name);done.add(name)
    for name in functions:dfs(name)
    sizes={}
    for line in su.splitlines():
        head,size,kind=line.rsplit('\t',2)
        assert kind=='static',line
        sizes[head.rsplit(':',1)[1]]=int(size)
    matched={}
    for name in functions:
        key=name[1:]
        if key in sizes:matched[key]=sizes[key]
        else:
            # Linker-only helpers without stack-pointer reads have no C frame.
            assert name not in has_global_get,('unaccounted generated frame',name)
            matched[key]=0
    # All functions combined is conservative for an acyclic graph. Include
    # 128-byte ABI leaf red zone + 16-byte alignment slack PER function too.
    bound=sum(matched.values())+144*len(functions)
    assert bound<=65536,bound
    return dict(acyclic=True,indirect_targets=sorted(table),frames=matched,
                static_frames_sum=sum(matched.values()),
                conservative_linear_bound=bound,charged_linear_reserve=65536,
                guest_imports=imports,no_tail_instructions=True,
                graph={k:sorted(v) for k,v in graph.items()})


def main():
    if not __debug__:raise RuntimeError('Do not disable assertions')
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--out',type=Path,required=True)
    ap.add_argument('--llvm',type=Path,default=Path('/opt/homebrew/opt/llvm/bin'))
    ap.add_argument('--linker',type=Path,default=Path('/opt/homebrew/bin/wasm-ld'))
    ap.add_argument('--tools',type=Path,default=Path('/tmp/rill-backend-30-tools'))
    a=ap.parse_args();a.out.mkdir(parents=True,exist_ok=True)
    out=a.out.resolve();llvm=a.llvm.resolve();linker=a.linker.absolute()
    binaryen=a.tools.resolve()/'binaryen-version_133/bin'
    capi=a.tools.resolve()/'wasmtime-v50.0.1-aarch64-macos-c-api'
    resource.setrlimit(resource.RLIMIT_CORE,(0,0))
    commands=[]; observations=[]; analyses={}
    def run(name,cmd):
        cmd=[str(c) for c in cmd];print(name,shlex.join(cmd),flush=True)
        p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=180)
        (out/(name+'.stdout')).write_text(p.stdout);(out/(name+'.stderr')).write_text(p.stderr)
        commands.append(dict(name=name,argv=cmd,cwd=str(Path.cwd()),exit=p.returncode))
        (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        assert p.returncode==0,(name,p.stderr)
        return p.stdout
    versions={}
    for name,cmd,want in [('llvm',[llvm/'llvm-config','--version'],'23.1.5'),
                          ('clang',[llvm/'clang','--version'],'23.1.5'),
                          ('linker',[linker,'--version'],'23.1.5'),
                          ('binaryen',[binaryen/'wasm-opt','--version'],'133'),
                          ('wasmtime',[a.tools/'wasmtime-v50.0.1-aarch64-macos/wasmtime','--version'],'50.0.1'),
                          ('node',['node','--version'],'v26.13.0'),
                          ('node-details',['node','-p','JSON.stringify({execPath:process.execPath,versions:process.versions})'],None),
                          ('host',['uname','-a'],None),('os',['sw_vers'],None),
                          ('sdk',['xcrun','--show-sdk-path'],None),
                          ('apple-linker',['xcrun','ld','-version_details'],None)]:
        versions[name]=run('version-'+name,cmd)
        if want:assert want in versions[name]
    for mode in (0,2):
        obj=out/f'machine-O{mode}.o';ir=out/f'machine-O{mode}.ll';wasm=out/f'machine-O{mode}.wasm'
        run(f'ir-{mode}',[llvm/'clang',*FLAGS,f'-O{mode}','-S','-emit-llvm',HERE/'machine.c','-o',ir])
        run(f'verify-{mode}',[llvm/'opt','-passes=verify','-disable-output',ir])
        run(f'object-{mode}',[llvm/'clang',*FLAGS,f'-O{mode}','-c',HERE/'machine.c','-o',obj])
        run(f'link-{mode}',[linker,'--no-entry','--stack-first','-z','stack-size=65536',
                           '--initial-memory=6291456','--max-memory=33554432','--export-memory',
                           *['--export='+n for n in EXPORTS],obj,'-o',wasm])
        wat=out/f'machine-O{mode}.wat'
        run(f'dis-{mode}',[binaryen/'wasm-dis',wasm,'-o',wat])
        run(f'mvp-validate-{mode}',[binaryen/'wasm-opt','--mvp-features',wasm,'-o',out/f'validated-O{mode}.wasm'])
        analyses[str(mode)]=analyze(wat.read_text(),(out/f'machine-O{mode}.su').read_text())
        (out/f'callgraph-O{mode}.json').write_text(json.dumps(analyses[str(mode)],indent=2)+'\n')
        data=run(f'node-{mode}',['node',HERE/'check.mjs',wasm])
        observations.extend(dict(compiler_mode=mode,engine='Node/V8',**json.loads(line)) for line in data.splitlines())
        host=out/f'host-O{mode}'
        run(f'host-build-{mode}',[llvm/'clang',f'-O{mode}','-Wall','-Wextra','-Werror','-pthread',
                                HERE/'host.c','-I',capi/'include','-L',capi/'lib','-lwasmtime',
                                f'-Wl,-rpath,{capi/"lib"}','-o',host])
        data=run(f'wasmtime-{mode}',[host,wasm,mode])
        observations.extend(dict(compiler_mode=mode,engine='Wasmtime/Cranelift/macOS/AArch64',**json.loads(line)) for line in data.splitlines())
    node_path=Path(json.loads(versions['node-details'])['execPath'])
    files=list(p for p in HERE.iterdir() if p.is_file())+[llvm/'clang',llvm/'opt',llvm/'llc',linker,binaryen/'wasm-opt',binaryen/'wasm-dis',node_path,capi/'lib/libwasmtime.dylib']
    result=dict(gate='OPEN; manually lowered candidate, not adopted architecture or complete Rill backend',
                versions=versions,source_and_tool_sha256={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in files},
                callgraph_analysis=analyses,observations=observations,
                limits=['No automated Rill MIR/ABI/ownership/checker transform',
                        'Primitive/byte-pattern aggregates only; not owned-resource move/drop proof',
                        'Cleanup/joins/wrapping/cancellation are modeled record phases, not production Rill semantics',
                        '64KiB conservative runtime reservation charged from entry; admission policy not approved',
                        'Native M:N migration, complete provider integrations and full compiler-generated bounds remain open'])
    (out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
    print(f'{len(commands)} commands; {len(observations)} observations; candidate only; support gate OPEN: {out}')


if __name__=='__main__':main()
