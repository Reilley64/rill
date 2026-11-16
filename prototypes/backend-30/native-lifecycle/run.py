#!/usr/bin/env python3
"""Pinned fixed-stack lifecycle probes; no production Boost dependency selected."""
import argparse,hashlib,json,re,resource,shlex,subprocess,urllib.request
from pathlib import Path
HERE=Path(__file__).resolve().parent
TERMINATION_CALL=re.compile(r'\b(?:call|invoke)\b[^\n]*@.*terminate_guest')
assert TERMINATION_CALL.search('notail call void @terminate_guest(ptr %x)')
assert TERMINATION_CALL.search('invoke void @terminate_guest(ptr %x)')
assert not TERMINATION_CALL.search('declare void @terminate_guest(ptr)')
IMAGES={
 'arm':('linux/arm64','rust@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137','aarch64-linux-gnu'),
 'x86':('linux/amd64','rust@sha256:d841af61827b29b3efa46f5363e8f2e972b379cc0463c319e8f320d9f0c8eb0c','x86_64-linux-gnu'),
}


def recursive_bound(ir,su):
 functions={m[1]:m[2] for m in re.finditer(r'^define[^\n]*?@([^ (]+)\([^\n]*\{\n(.*?)^}',ir,re.M|re.S)}
 frames={}
 for line in su.splitlines():
  name,size,kind=line.split('\t');frames[name.rsplit(':',1)[-1]]=(int(size),kind)
 escape=next(n for n in functions if 'terminate_guest' in n)
 seen=set();visiting=set()
 def visit(name):
  if name in seen:return
  assert name not in visiting,('recursive emergency helper',name)
  visiting.add(name);assert frames[name][1]=='static',(name,frames[name])
  sites=[line for line in functions[name].splitlines() if re.search(r'\b(?:call|invoke)\b',line)]
  assert all('@' in line for line in sites),('unknown indirect emergency call',name,sites)
  for callee in re.findall(r'\b(?:call|invoke)\b[^\n]*?@([^ (]+)',functions[name]):
   if callee.startswith('llvm.') or callee in ('jump_fcontext','abort','__assert_rtn','__assert_fail'):continue
   assert callee in functions,('unknown emergency helper',name,callee)
   visit(callee)
  visiting.remove(name);seen.add(name)
 visit(escape)
 for name in ('native_ordinary_body','native_checked_admission'):
  assert name in functions and frames[name][1]=='static'
  allowed={escape,'native_ordinary_body','native_checked_admission','__assert_rtn','__assert_fail'}
  edges=re.findall(r'\b(?:call|invoke)\b[^\n]*?@([^ (]+)',functions[name])
  assert all(c in allowed or c.startswith('llvm.') for c in edges),(name,edges)
 # The ordinary body is admitted separately on every recursive edge. The sum
 # below is deliberately conservative: full static frames + extra alignment,
 # red-zone/link allowance per node, fixed emergency reserve and assembly save.
 names=seen|{'native_ordinary_body','native_checked_admission'}
 need=sum(frames[n][0]+4240 for n in names)+16384+256
 assert need<1024*1024
 return dict(need=need,frames={n:frames[n] for n in sorted(names)},per_node_extra=4240,
             emergency_reserve=16384,fcontext_save_allowance=256,
             terminal_jump_never_resumed=True,diagnostic_abort_edges_not_normal_execution=True)


def main():
 if not __debug__:raise RuntimeError('Assertions required')
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--out',type=Path,required=True)
 ap.add_argument('--cache',type=Path,default=Path('/tmp/rill-30-lifecycle-tools'))
 ap.add_argument('--llvm',type=Path,default=Path('/opt/homebrew/opt/llvm/bin'))
 a=ap.parse_args();a.out.mkdir(parents=True,exist_ok=True);a.cache.mkdir(parents=True,exist_ok=True)
 out=a.out.resolve();cache=a.cache.resolve();llvm=a.llvm.resolve();commands=[];rows=[];deps={};audits={}
 resource.setrlimit(resource.RLIMIT_CORE,(0,0))
 def run(name,cmd):
  cmd=[str(x) for x in cmd];print(name,shlex.join(cmd),flush=True)
  p=subprocess.run(cmd,capture_output=True,text=True,timeout=180)
  (out/(name+'.stdout')).write_text(p.stdout);(out/(name+'.stderr')).write_text(p.stderr)
  commands.append(dict(name=name,argv=cmd,cwd=str(Path.cwd()),exit=p.returncode))
  (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
  assert p.returncode==0,(name,p.stderr);return p.stdout
 pins=json.loads((HERE/'source-pins.json').read_text());sources=cache/'context';sources.mkdir(exist_ok=True)
 for name,digest in pins['files'].items():
  p=sources/name
  if not p.exists():
   url=f"https://raw.githubusercontent.com/{pins['repository']}/{pins['revision']}/src/asm/{name}"
   data=urllib.request.urlopen(url).read();assert hashlib.sha256(data).hexdigest()==digest
   p.write_bytes(data)
  assert hashlib.sha256(p.read_bytes()).hexdigest()==digest,name
 version=run('version-clang',[llvm/'clang++','--version']);assert '23.1.5' in version
 sdk=run('version-sdk',['xcrun','--show-sdk-path']).strip()
 run('version-host',['uname','-a']);run('version-os',['sw_vers'])
 run('version-linker',['xcrun','ld','-version_details'])
 run('version-rosetta',['pkgutil','--pkg-info','com.apple.pkg.RosettaUpdateAuto'])
 run('version-docker',['docker','version','--format','{{json .Server}}'])
 for arch,(platform,image,abi) in IMAGES.items():
  root=cache/('headers-'+arch);(root/'usr/lib').mkdir(parents=True,exist_ok=True)
  if not (root/'usr/include'/abi/'asm/errno.h').is_file():
   cid=run('headers-create-'+arch,['docker','create','--platform',platform,'--network=none','--entrypoint=/bin/true',image]).strip()
   try:
    run('headers-include-'+arch,['docker','cp',cid+':/usr/include',root/'usr/'])
    run('headers-uapi-'+arch,['docker','cp',cid+':/usr/lib/linux',root/'usr/lib/'])
   finally:run('headers-remove-'+arch,['docker','rm',cid])
  assert (root/'usr/include/c++/14/thread').is_file()
 targets=[('arm-macos','arm64-apple-darwin27.0.0','arm64_aapcs','macho'),
          ('x86-macos','x86_64-apple-darwin27.0.0','x86_64_sysv','macho'),
          ('arm-linux','aarch64-unknown-linux-gnu','arm64_aapcs','elf'),
          ('x86-linux','x86_64-unknown-linux-gnu','x86_64_sysv','elf')]
 for label,triple,asm_arch,fmt in targets:
  objects=[]
  for op in ['make','jump']:
   obj=out/f'{op}-{label}.o';objects.append(obj)
   run(f'asm-{op}-{label}',[llvm/'clang','--target='+triple,'-c',sources/f'{op}_{asm_arch}_{fmt}_gas.S','-o',obj])
  for mode in (0,2):
   obj=out/f'body-{label}-O{mode}.o';exe=out/f'lifecycle-{label}-O{mode}'
   flags=['--target='+triple,'-std=c++17',f'-O{mode}','-fno-stack-protector','-Wall','-Wextra','-Werror','-pthread']
   if fmt=='elf':
    arch=label.split('-')[0];platform,image,abi=IMAGES[arch];root=cache/('headers-'+arch)
    flags+=['--sysroot='+str(root),'-nostdinc++']
    for include in [root/'usr/include/c++/14',root/'usr/include'/abi/'c++/14',root/'usr/include/c++/14/backward',root/'usr/include'/abi]:
     flags+=['-isystem',str(include)]
   else:flags+=['-isysroot',sdk]
   dep=out/f'{label}-O{mode}.d'
   run(f'compile-{label}-{mode}',[llvm/'clang++',*flags,'-fstack-usage','-MD','-MF',dep,'-c',HERE/'lifecycle.cpp','-o',obj])
   ir=out/f'{label}-O{mode}.ll'
   run(f'ir-{label}-{mode}',[llvm/'clang++',*flags,'-S','-emit-llvm',HERE/'lifecycle.cpp','-o',ir])
   run(f'verify-{label}-{mode}',[llvm/'opt','-passes=verify','-disable-output',ir])
   # A suspended owning scope must not be removed by an opportunistic sibling
   # call to the terminal escape helper. This is ordinary, not eligible-tail work.
   ir_text=ir.read_text();calls=[x for x in ir_text.splitlines() if TERMINATION_CALL.search(x)]
   assert calls and all('notail call' in x or 'invoke ' in x for x in calls),calls
   scope=re.search(r'^define[^\n]*borrowed_scope[^\n]*#([0-9]+)',ir_text,re.M);assert scope
   attrs=re.search(r'^attributes #'+scope[1]+r' = .*$',ir_text,re.M);assert attrs
   assert '"disable-tail-calls"="true"' in attrs[0]
   bound=recursive_bound(ir_text,obj.with_suffix('.su').read_text())
   audits[label+'-O'+str(mode)]=dict(termination_edges=len(calls),all_ordinary=True,owning_scope_tail_calls_disabled=True,recursive_admission=bound)
   header_names=shlex.split(dep.read_text().replace('\\\n',' ').split(':',1)[1])
   deps[label+'-O'+str(mode)]={p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in header_names}
   if fmt=='macho':
    run(f'link-{label}-{mode}',[llvm/'clang++','--target='+triple,'-isysroot',sdk,'-pthread',obj,*objects,'-o',exe])
    data=run(f'execute-{label}-{mode}',(['arch','-x86_64'] if label.startswith('x86') else [])+[exe,str(bound['need'])])
   else:
    # Only the linker/runtime come from the image; every test body and context
    # switch object was generated by the above LLVM23 commands on the host.
    script=f'set -ex; c++ --version; ld --version; getconf GNU_LIBC_VERSION; uname -a; c++ -pthread /out/{obj.name} '+ ' '.join('/out/'+x.name for x in objects)+f' -o /out/{exe.name}; /out/{exe.name} {bound["need"]}'
    data=run(f'linux-link-execute-{label}-{mode}',['docker','run','--rm','--network=none','--ulimit','core=0:0','--platform',platform,'--mount',f'type=bind,src={out},dst=/out','--entrypoint=sh',image,'-c',script])
   observed=[json.loads(line) for line in data.splitlines() if line.startswith('{')]
   assert len(observed)==10
   for x in observed:
    if x['case'] in ('normal_join','cancel_masked_join','top_level_exit_retained_loan','fatal_exit_retained_loan','exclusive_native_output_join'):
     assert x['backing_releases']==1 and x['worker_mask']==3 and x['switches']>=10000
    if x['case']=='recursive_capacity_fatal_retained_loan':
     assert x['ordinary_entered']>9 and x['ordinary_completed']==9
     assert x['least_remaining']<x['admission_need']==bound['need']
     assert x['adjacent_canaries'] and x['disposers']==0 and x['backing_releases']==1
    if x['case']=='native_suspending_iterative_cleanup':
     expected=sum(sum(str(i).encode()) for i in range(200000))
     assert x['render_hash']==expected and x['resources_freed']==133333 and x['native_frame_span']<4096
   rows.extend(dict(target=label,mode=mode,execution='translated/emulated' if label.startswith('x86') else 'native' if fmt=='macho' else 'ARM64 VM',**x) for x in observed)
 files=[HERE/'lifecycle.cpp',HERE/'run.py',HERE/'source-pins.json',llvm/'clang++',llvm/'clang',llvm/'opt']
 result=dict(scope='Finite native context-switch/loan-retention mechanism, not a complete Rill runtime or dependency adoption',
             context_source=pins,linux_runtime_images=IMAGES,commands=commands,
             source_and_tool_sha256={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in files},
             used_header_sha256=deps,ir_retention_checks=audits,observations=rows,
             limits=['Recursive admission bound covers its audited fixed-frame body/entry/emergency subtree only; native-capacity separately covers large outgoing copies/results',
                     'Workers use the same round-to-nearest environment; no live thread-affine/TLS-dependent user resource crosses a yield',
                     'Raw fcontext API used without forced-unwind C++ fiber destruction; production library dependency not selected',
                     'x86 execution is translated/emulated, not physical x86 hardware'])
 (out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
 print(len(commands),'commands;',len(rows),'native lifecycle observations')


if __name__=='__main__':main()
