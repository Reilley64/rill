import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const module=await WebAssembly.compile(readFileSync(process.argv[2]));
const fallback=Number(process.argv[3]);
class Fatal extends Error {}
function open() {
 let observations=0;
 const e=new WebAssembly.Instance(module,{env:{
  observe(n){assert.equal(n,BigInt(observations));++observations;},
  fatal(reason){throw new Fatal(`fatal ${reason}`);},
 }}).exports;
 return {e,count:()=>observations};
}
{
 const {e,count}=open();e.start(8);
 for(let n=0;n<1000000;++n)e.resume(0);
 assert.equal(count(),1000001);
 assert.equal(e.stats(0),fallback);assert.equal(e.stats(1),0);assert.equal(e.stats(3),fallback);
 if(fallback)assert.equal(e.stats(2),8);
 e.resume(1);
 assert.equal(e.stats(1),fallback);assert.equal(e.stats(3),0);
 console.log(JSON.stringify({test:'spill_resume_destroy',observations:count(),allocations:e.stats(0),frees:e.stats(1),fallback_bytes:e.stats(2)}));
}
{
 const {e,count}=open();
 const guard=new Uint8Array(e.memory.buffer,e.pool_address()+7,16);guard.fill(0xa5);
 let fatal=false;try{e.start(7);}catch(error){assert(error instanceof Fatal);fatal=true;}
 assert.equal(fatal,Boolean(fallback));
 assert(guard.every(v=>v===0xa5));
 if(!fallback){assert.equal(count(),1);e.resume(1);}
 // Failed invocation is not re-entered; only host memory observation follows it.
 console.log(JSON.stringify({test:'undersized_fallback_region',fatal_expected:Boolean(fallback),fatal_observed:fatal,canary_intact:true}));
}
