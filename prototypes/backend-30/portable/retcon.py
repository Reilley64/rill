#!/usr/bin/env python3
"""Narrow LLVM23 retcon spill/allocation probe, not proper-tail lowering.
Intrinsic signatures checked against pinned llvm/test/Transforms/Coroutines/
coro-retcon.ll at 392dfff97d29b59d77e5cac6d4b6ef2585fe7117.
"""
import argparse, hashlib, json, re, resource, subprocess
from pathlib import Path
HERE=Path(__file__).resolve().parent
CORO_CALL=re.compile(r'\bcall[^\n]*@llvm\.coro\.')


def module(fallback):
    size,align=(4,4) if fallback else (16,8)
    return f'''target datalayout = "e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-i128:128-n32:64-S128-ni:1:10:20"
target triple = "wasm32-unknown-wasip1"
@buffer = global [16 x i8] zeroinitializer, align 8
@pool = global [256 x i8] zeroinitializer, align 64
@continuation = global ptr null
@limit = global i32 0
@active = global i32 0
@allocated = global i32 0
@freed = global i32 0
@bytes = global i32 0

define ptr @worker(ptr %buffer, i64 %initial) noinline {{
entry:
 %id = call token @llvm.coro.id.retcon(i32 {size}, i32 {align}, ptr %buffer, ptr @prototype, ptr @allocate, ptr @deallocate)
 %h = call ptr @llvm.coro.begin(token %id, ptr null)
 br label %loop
loop:
 %n = phi i64 [%initial, %entry], [%next, %continue]
 call void @observe(i64 %n)
 %stop = call i1 (...) @llvm.coro.suspend.retcon.i1()
 br i1 %stop, label %end, label %continue
continue:
 %next = add i64 %n, 1
 br label %loop
end:
 call void @llvm.coro.end(ptr %h, i1 false, token none)
 unreachable
}}
define ptr @allocate(i32 %size) noinline {{
 %lim = load i32, ptr @limit
 %busy = load i32, ptr @active
 %fits = icmp ule i32 %size, %lim
 %idle = icmp eq i32 %busy, 0
 %ok = and i1 %fits, %idle
 br i1 %ok, label %success, label %error
error:
 call void @fatal(i32 1)
 unreachable
success:
 store i32 1, ptr @active
 store i32 %size, ptr @bytes
 %count = load i32, ptr @allocated
 %inc = add i32 %count, 1
 store i32 %inc, ptr @allocated
 ret ptr @pool
}}
define void @deallocate(ptr %p) noinline {{
 %right = icmp eq ptr %p, @pool
 %busy = load i32, ptr @active
 %live = icmp eq i32 %busy, 1
 %ok = and i1 %right, %live
 br i1 %ok, label %success, label %error
error:
 call void @fatal(i32 2)
 unreachable
success:
 store i32 0, ptr @active
 %count = load i32, ptr @freed
 %inc = add i32 %count, 1
 store i32 %inc, ptr @freed
 ret void
}}
define void @start(i32 %capacity) {{
 %ok = icmp ule i32 %capacity, 256
 br i1 %ok, label %begin, label %error
error:
 call void @fatal(i32 3)
 unreachable
begin:
 store i32 %capacity, ptr @limit
 %prepared = call ptr @llvm.coro.prepare.retcon(ptr @worker)
 %next = call ptr %prepared(ptr @buffer, i64 0)
 store ptr %next, ptr @continuation
 ret void
}}
define void @resume(i1 zeroext %stop) {{
 %current = load ptr, ptr @continuation
 %valid = icmp ne ptr %current, null
 br i1 %valid, label %run, label %error
error:
 call void @fatal(i32 4)
 unreachable
run:
 %next = call ptr %current(ptr @buffer, i1 zeroext %stop)
 store ptr %next, ptr @continuation
 ret void
}}
define i32 @pool_address() {{ %p = ptrtoint ptr @pool to i32
 ret i32 %p }}
define i32 @stats(i32 %kind) {{
 %a = load i32, ptr @allocated
 %f = load i32, ptr @freed
 %b = load i32, ptr @bytes
 %l = load i32, ptr @active
 %c = icmp eq i32 %kind, 0
 %d = icmp eq i32 %kind, 1
 %e = icmp eq i32 %kind, 2
 %x = select i1 %e, i32 %b, i32 %l
 %y = select i1 %d, i32 %f, i32 %x
 %z = select i1 %c, i32 %a, i32 %y
 ret i32 %z
}}
declare void @observe(i64)
declare void @fatal(i32) noreturn
declare token @llvm.coro.id.retcon(i32,i32,ptr,ptr,ptr,ptr)
declare ptr @llvm.coro.begin(token,ptr)
declare i1 @llvm.coro.suspend.retcon.i1(...)
declare void @llvm.coro.end(ptr,i1,token)
declare ptr @llvm.coro.prepare.retcon(ptr)
declare ptr @prototype(ptr,i1 zeroext)
'''


def main():
    if not __debug__:raise RuntimeError('Assertions required')
    assert CORO_CALL.search('%h = call ptr @llvm.coro.begin(token %id, ptr null)')
    assert not CORO_CALL.search('declare ptr @llvm.coro.begin(token, ptr)')
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--out',type=Path,required=True)
    a=ap.parse_args();a.out.mkdir(parents=True,exist_ok=True);out=a.out.resolve()
    llvm=Path('/opt/homebrew/opt/llvm/bin');bin=Path('/tmp/rill-backend-30-tools/binaryen-version_133/bin')
    resource.setrlimit(resource.RLIMIT_CORE,(0,0));commands=[];rows=[]
    def run(name,cmd):
        cmd=[str(c) for c in cmd];p=subprocess.run(cmd,capture_output=True,text=True,timeout=180)
        (out/(name+'.stdout')).write_text(p.stdout);(out/(name+'.stderr')).write_text(p.stderr)
        commands.append(dict(name=name,argv=cmd,exit=p.returncode))
        (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        assert p.returncode==0,(name,p.stderr)
        return p.stdout
    versions={}
    for name,cmd,want in [('opt',[llvm/'opt','--version'],'23.1.5'),('llc',[llvm/'llc','--version'],'23.1.5'),('linker',['/opt/homebrew/bin/wasm-ld','--version'],'23.1.5'),('node',['node','--version'],'v26.13.0')]:
        versions[name]=run('version-'+name,cmd);assert want in versions[name]
    for fallback in (False,True):
        label='fallback' if fallback else 'inline';src=out/(label+'.ll');src.write_text(module(fallback))
        run(label+'-verify',[llvm/'opt','-passes=verify','-disable-output',src])
        split=out/(label+'-split.ll')
        run(label+'-split',[llvm/'opt','-passes=module(coro-early),cgscc(coro-split),module(coro-cleanup),verify','-S',src,'-o',split])
        assert not CORO_CALL.search(split.read_text()),label
        for mode in (0,2):
            optimized=out/f'{label}-O{mode}.ll';obj=out/f'{label}-O{mode}.o';wasm=out/f'{label}-O{mode}.wasm'
            run(f'{label}-opt-{mode}',[llvm/'opt',f'-passes=default<O{mode}>,verify','-S',split,'-o',optimized])
            run(f'{label}-obj-{mode}',[llvm/'llc','-mtriple=wasm32-unknown-wasip1','-mattr=-tail-call,-bulk-memory,-reference-types,-multivalue',f'-O={mode}','-filetype=obj',optimized,'-o',obj])
            run(f'{label}-link-{mode}',['/opt/homebrew/bin/wasm-ld','--no-entry','--allow-undefined','--export-memory','--export=start','--export=resume','--export=stats','--export=pool_address',obj,'-o',wasm])
            wat=out/f'{label}-O{mode}.wat';run(f'{label}-dis-{mode}',[bin/'wasm-dis',wasm,'-o',wat]);assert 'return_call' not in wat.read_text()
            data=run(f'{label}-node-{mode}',['node',HERE/'retcon-check.mjs',wasm,int(fallback)])
            rows.extend(dict(case=label,mode=mode,**json.loads(line)) for line in data.splitlines())
    hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in (HERE/'retcon.py',HERE/'retcon-check.mjs',llvm/'opt',llvm/'llc',Path('/opt/homebrew/bin/wasm-ld'),bin/'wasm-dis')}
    (out/'results.json').write_text(json.dumps(dict(scope='Compiler machinery ONLY; no source tail replacement or complete ledger proof',versions=versions,source_and_tool_sha256=hashes,observations=rows),indent=2)+'\n')
    print(len(commands),'commands;',len(rows),'retcon observations; NOT support or adoption')


if __name__=='__main__':main()
