// External embedding observations. A trapped/fatal routine is NEVER resumed.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const module = await WebAssembly.compile(readFileSync(process.argv[2]));
class Fatal extends Error { constructor(id, reason) { super(`fatal ${id}:${reason}`); this.reason=reason; } }
function open() {
  return new WebAssembly.Instance(module,{env:{fatal(id,reason){throw new Fatal(id,reason);}}}).exports;
}
const M=1048576, R=65536;
const get=(e,id,field)=>e.inspect(id,field);
const number=(e,id,field)=>Number(get(e,id,field));
const out=row=>console.log(JSON.stringify(row,(_,v)=>typeof v==='bigint'?v.toString():v));
const setup=(e,id,mode,n,a=M,s=M)=>e.init(id,BigInt(a),BigInt(s),mode,BigInt(n));
function snapshot(e,id) {
  return {saved_peak:number(e,id,4),addressable_peak:number(e,id,2),frames_peak:number(e,id,5),
    effects:number(e,id,6),steps:number(e,id,8),result:number(e,id,9),
    backing_bytes:number(e,id,24),header_bytes:number(e,id,0)};
}
function finish(e,id,{cancel=false}={}) {
  let waits=0, rounds=0, lastToken=0n;
  for (;;) {
    assert(++rounds<100000);
    const state=e.poll(id,4096);
    assert.equal(e.canaries(id),1);
    if (state===0) break;
    if (state===1) {
      ++waits;
      const token=get(e,id,7), effects=get(e,id,6), frames=get(e,id,12), pc=get(e,id,32);
      assert.notEqual(token,lastToken); lastToken=token;
      assert.equal(e.poll(id,4096),1); // Persistent wait: no replay or progress.
      assert.equal(get(e,id,6),effects); assert.equal(get(e,id,12),frames);
      assert.equal(get(e,id,32),pc);
      if (cancel && number(e,id,28)) {
        e.request_cancel(id); assert.equal(number(e,id,18),0);
        assert.equal(e.poll(id,1),1); assert.equal(number(e,id,16),1);
      }
      assert.equal(e.wake(id,token-1n),0);
      assert.equal(e.wake(id,token),1);
      assert.equal(e.wake(id,token),0);
    }
  }
  assert.equal(number(e,id,12),0); assert.equal(e.canaries(id),1);
  return waits;
}
for (const mode of [0,1,2]) {
  const e=open(); const peaks=[];
  for (const n of [1024,1000000]) {
    setup(e,0,mode,n);
    const waits=finish(e,0);
    const s=snapshot(e,0); peaks.push(s);
    assert.equal(s.result,n); assert.equal(s.effects,n+1); assert.equal(s.frames_peak,2);
    assert.equal(s.addressable_peak,R+64);
    assert.equal(waits,Math.floor(n/1024)+1);
    out({case:'tail_replacement',mode,n,waits,...s});
  }
  assert.equal(peaks[0].saved_peak,peaks[1].saved_peak);
  assert.equal(peaks[0].backing_bytes,peaks[1].backing_bytes);
}
{
  const e=open(), n=2000; setup(e,0,4,n);
  const waits=finish(e,0,{cancel:true}), s=snapshot(e,0);
  assert.equal(s.result,n+1); assert.equal(s.effects,n+1); assert.equal(s.frames_peak,n+2);
  assert.equal(number(e,0,22),n);
  assert.equal(number(e,0,19),500); assert.equal(number(e,0,20),500); assert.equal(number(e,0,21),500);
  assert.equal(number(e,0,18),1);
  out({case:'ordinary_borrow_guard_join_wrap_MODEL',n,waits,...s});
}
{
  const e=open(), n=8000;
  for(const id of [0,1]) setup(e,id,4,n);
  for(const id of [0,1]) { let state; do {state=e.poll(id,4096);}while(state===2); assert.equal(state,1); }
  const perRoutine=[number(e,0,3),number(e,1,3)];
  assert(perRoutine.every(v=>v<M) && perRoutine[0]+perRoutine[1]>M);
  assert.notEqual(get(e,0,7),get(e,1,7));
  assert.equal(e.wake(1,get(e,0,7)),0); // Cross-routine wake cannot match.
  finish(e,0); finish(e,1);
  out({case:'per_routine_saved_setting',simultaneous_saved:perRoutine,total:perRoutine[0]+perRoutine[1]});
}
// All fatal cases get a separate instance. Read canaries from memory only after
// the external trap: do not re-enter canaries()/inspect()/poll() after failure.
function fatalCase(name,action,reason) {
  const e=open(); setup(e,0,0,16);
  const guards=[number(e,0,26),number(e,0,27)];
  let caught;
  try { action(e); } catch(error) { caught=error; }
  assert(caught instanceof Fatal,name); assert.equal(caught.reason,reason,name);
  for(const p of guards) assert(new Uint8Array(e.memory.buffer,p,16).every(v=>v===0xa5),name);
  out({case:name,outcome:'FATAL_EXPECTED',reason,adjacent_canaries_intact:true});
}
fatalCase('invalid_address_capacity',e=>setup(e,0,0,16,R-1),1);
fatalCase('invalid_saved_capacity',e=>setup(e,0,0,16,M,number(e,0,0)+79),1);
fatalCase('u64_capacity_unrepresentable',e=>e.init(0,1n<<63n,BigInt(M),0,16n),1);
// Guard locations must match the actual smaller logical regions in these tests.
function budgetCase(name,mode,n,a,s,reason) {
  const e=open(), header=number(e,0,0);
  setup(e,0,mode,n,a,s===null?header+80:s(header));
  const guards=[number(e,0,26),number(e,0,27)];
  let caught;
  try { for(let i=0;i<100000;++i) {const st=e.poll(0,4096); if(st===1)e.wake(0,get(e,0,7)); if(st===0)break;} }
  catch(error){caught=error;}
  assert(caught instanceof Fatal,name); assert.equal(caught.reason,reason,name);
  for(const p of guards) assert(new Uint8Array(e.memory.buffer,p,16).every(v=>v===0xa5),name);
  out({case:name,outcome:'FATAL_EXPECTED',reason,adjacent_canaries_intact:true});
}
budgetCase('saved_exact_root_no_callee_room',0,10,M,null,2);
budgetCase('tail_larger_aligned_packet_no_room',1,1025,M,h=>h+192,2);
budgetCase('ordinary_saved_exhaustion',4,100,M,h=>h+80+8*96,2);
budgetCase('addressable_borrowed_slots_exhaustion',4,100,R+64+8*16,()=>M,3);
budgetCase('oversized_addressable_request',5,1000000,M,()=>M,3);
for(const [name,used,bytes,align,cap,reason] of [
 ['u64_alignment_overflow',(1n<<64n)-1n,1n,16n,(1n<<64n)-1n,4],
 ['u32_range_overflow',0xfffffff0n,32n,16n,(1n<<64n)-1n,3],
 ['zero_alignment',0n,1n,0n,100n,4],
 ['non_power_two_alignment',0n,1n,3n,100n,4],
 ['unsupported_alignment',0n,1n,128n,1000n,4],
]) fatalCase(name,e=>e.test_reserve(0,0,used,bytes,align,cap),reason);
for(const obligation of [1,2,4,8]) fatalCase(`malformed_tail_with_obligation_${obligation}`,e=>{
 e.poll(0,2); e.test_blocked_tail(0,obligation);
},5);
{
 const e=open(); setup(e,0,0,1,R+64,number(e,0,0)+144);
 finish(e,0); assert.equal(number(e,0,9),1);
 assert.equal(e.test_reserve(0,0,33n,64n,64n,128n),64n);
 assert.equal(e.test_reserve(0,0,0xfffffff0n,15n,16n,0xffffffffn),0xfffffff0n);
 out({case:'exact_fit_and_alignment',...snapshot(e,0)});
}
// Mutable-memory JS compatibility control. Actual native relocation + worker
// copy/completion is separately exercised by the C/Wasmtime embedding.
{
 const e=open(); setup(e,0,3,1000000);
 while(e.poll(0,4096)!==1){}
 const token=get(e,0,7), root=number(e,0,14), dest=number(e,0,15);
 const copy=new Uint8Array(e.memory.buffer,root,16).slice();
 const old=e.memory.buffer;
 e.drop_handle(0); e.request_cancel(0);
 assert.equal(e.wake(0,token),0); assert.equal(number(e,0,16),1);
 e.memory.grow(1); assert.equal(old.byteLength,0);
 assert.deepEqual(new Uint8Array(e.memory.buffer,root,16),copy);
 const v=new DataView(e.memory.buffer), input=new DataView(copy.buffer);
 v.setBigUint64(dest,input.getBigUint64(0,true)^0x55n,true);
 v.setBigUint64(dest+8,input.getBigUint64(8,true),true);
 assert.equal(e.complete_foreign(0,token),1); assert.equal(e.complete_foreign(0,token),0);
 assert.equal(e.wake(0,token),1); assert.equal(e.wake(0,token),0);
 finish(e,0); assert.equal(number(e,0,9),1000000);
 out({case:'legal_between_poll_growth_copy_control',old_view_detached:true,owner_retained_until_completion:true,...snapshot(e,0)});
}
