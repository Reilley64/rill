// THROWAWAY manually lowered mechanism probe, NOT a Rill compiler/runtime.
// No allocator, recursive resume, Asyncify, engine fiber or required return_call.
typedef unsigned int u32;
typedef unsigned long long u64;
typedef unsigned char u8;
_Static_assert(sizeof(void *) == 4 && sizeof(u64) == 8, "wasm32 only");
#define MAX 1048576u
#define RUNTIME 65536u
#define NONE 0xffffffffu
#define ROOT 5u
#define REC 4u
#define MAGIC 0x123456789abcdef0ull
#define EXPORT __attribute__((visibility("default")))
#define NOINLINE __attribute__((noinline))
__attribute__((import_module("env"), import_name("fatal"), noreturn))
extern void fatal(u32 routine, u32 reason);

enum { DONE, WAIT, READY };
enum { BAD_CONFIG=1, SAVED_LIMIT, ADDRESS_LIMIT, ALIGNMENT, PROTOCOL, CORRUPTION };
// Prefix of a variable-size/alignment record. Every field is retained state.
typedef struct {
  u32 tag, pc, previous, restore_s, restore_a, slot, flags, pad;
  u64 n, acc, parent_slot, output;
} Frame;
_Static_assert(sizeof(Frame) == 64, "record prefix");
typedef struct {
  u32 a_limit, s_limit, a_used, s_used, a_peak, s_peak;
  u32 current, frames, frames_peak, mode, status, generation;
  u32 pending, serial, native_needed, native_done, foreign_count;
  u32 cancel_requested, cancel_observed, handle_live, root_slot;
  u32 mask, guards, joins, wraps, borrow_checks, stale_wakes;
  u64 token, requested, result, effects, steps;
  // Staging is INSIDE the charged header, not an uncharged outgoing packet.
  _Alignas(16) u8 staging[256];
} Control;
#define HEADER ((u32)((sizeof(Control) + 15u) & ~15u))
typedef struct {
  Control c;
  _Alignas(64) u8 address[MAX + 16];
  _Alignas(64) u8 saved[MAX + 16];
} Routine;
static Routine routines[2];
static const u32 sizes[] = {64, 96, 192, 256, 96, 80};
static const u32 aligns[] = {16, 32, 64, 32, 16, 16};

static NOINLINE Routine *get(u32 id) {
  if (id >= 2) fatal(id, PROTOCOL);
  return &routines[id];
}
static NOINLINE void fill(u8 *p, u32 n, u8 value) {
  for (u32 i=0; i<n; ++i) p[i]=value;
}
static NOINLINE void copy(u8 *to, const u8 *from, u32 n) {
  // Distinct staging vs arena: no overlapping memcpy or dangling step local.
  for (u32 i=0; i<n; ++i) to[i]=from[i];
}
static NOINLINE u32 reserve(u32 id, u64 used, u64 bytes, u64 alignment,
                            u64 limit, u32 reason) {
  if (!alignment || alignment>64 || (alignment & (alignment-1)) || used > ~0ull-(alignment-1))
    fatal(id, ALIGNMENT);
  u64 start=(used+alignment-1) & ~(alignment-1);
  if (start > limit || bytes > limit-start || start > 0xffffffffull ||
      bytes > 0xffffffffull-start) fatal(id, reason);
  return (u32)start;
}
static NOINLINE Frame *frame(u32 id) {
  Routine *r=get(id);
  if (r->c.current==NONE || r->c.current > r->c.s_used ||
      r->c.s_used-r->c.current < sizeof(Frame)) fatal(id, CORRUPTION);
  return (Frame *)(r->saved+r->c.current);
}
static NOINLINE void peaks(Routine *r) {
  if (r->c.a_used>r->c.a_peak) r->c.a_peak=r->c.a_used;
  if (r->c.s_used>r->c.s_peak) r->c.s_peak=r->c.s_used;
  if (r->c.frames>r->c.frames_peak) r->c.frames_peak=r->c.frames;
}
static NOINLINE u32 address(u32 id, u64 bytes, u64 alignment) {
  Routine *r=get(id);
  u32 p=reserve(id,r->c.a_used,bytes,alignment,r->c.a_limit,ADDRESS_LIMIT);
  fill(r->address+p,(u32)bytes,0);
  r->c.a_used=p+(u32)bytes; peaks(r); return p;
}
static NOINLINE u64 *words(u32 id, u32 offset, u32 length) {
  Routine *r=get(id);
  if ((offset&7) || offset>r->c.a_used || length>r->c.a_used-offset)
    fatal(id,CORRUPTION);
  return (u64 *)(r->address+offset);
}
static NOINLINE void install(u32 id,u32 tag,u64 n,u64 acc,u32 parent,u32 tail) {
  Routine *r=get(id);
  if (tag>ROOT) fatal(id,PROTOCOL);
  u32 previous=r->c.current, restore_s=r->c.s_used, restore_a=r->c.a_used;
  if (tail) {
    Frame *old=frame(id);
    if (old->flags || r->c.pending || r->c.a_used!=old->restore_a)
      fatal(id,PROTOCOL);
    previous=old->previous; restore_s=old->restore_s; restore_a=old->restore_a;
  }
  u32 p=reserve(id,restore_s,sizes[tag],aligns[tag],r->c.s_limit,SAVED_LIMIT);
  fill(r->c.staging,sizes[tag],0);
  Frame *next=(Frame *)r->c.staging;
  next->tag=tag; next->previous=previous; next->restore_s=restore_s;
  next->restore_a=restore_a; next->n=n; next->acc=acc; next->parent_slot=parent;
  next->output=r->c.root_slot;
  // Touch the whole selected packet, not only its prefix.
  for (u32 i=sizeof(Frame); i<sizes[tag]; ++i) r->c.staging[i]=(u8)(i^tag);
  copy(r->saved+p,r->c.staging,sizes[tag]);
  r->c.current=p; r->c.s_used=p+sizes[tag];
  if (!tail) ++r->c.frames;
  peaks(r);
}
static NOINLINE void verify_packet(u32 id) {
  Routine *r=get(id); Frame *f=frame(id);
  if (f->tag>ROOT || sizes[f->tag]>r->c.s_used-r->c.current) fatal(id,CORRUPTION);
  for (u32 i=sizeof(Frame); i<sizes[f->tag]; ++i)
    if (r->saved[r->c.current+i]!=(u8)(i^f->tag)) fatal(id,CORRUPTION);
}
static NOINLINE u32 wait_for_host(u32 id) {
  Routine *r=get(id);
  if (r->c.pending || r->c.serial==0xffffffffu) fatal(id,PROTOCOL);
  r->c.pending=1;
  r->c.token=((u64)r->c.generation<<33)|((u64)id<<32)|++r->c.serial;
  if (r->c.mode==3 && !r->c.foreign_count) {
    r->c.native_needed=1; r->c.foreign_count=1;
  }
  return WAIT;
}
static NOINLINE void returning(u32 id,u64 answer) {
  Routine *r=get(id); Frame *f=frame(id);
  if (r->c.pending || r->c.mask) fatal(id,PROTOCOL);
  u32 previous=f->previous, smark=f->restore_s, amark=f->restore_a;
  r->c.a_used=amark; r->c.s_used=smark; r->c.current=previous; --r->c.frames;
  if (previous==NONE) {
    r->c.result=answer; r->c.status=DONE;
    r->c.cancel_observed=r->c.cancel_requested;
  } else frame(id)->acc=answer;
}
static NOINLINE u32 tail_step(u32 id,u32 selection) {
  Routine *r=get(id); Frame *f=frame(id); verify_packet(id);
  if (!f->pc) {
    ++r->c.effects;
    // Earlier ordinary caller owns this output/borrowed slot.
    if (words(id,(u32)f->output,64)[0]!=MAGIC) fatal(id,CORRUPTION);
    f->pc=1;
    if (f->n%1024==0) return wait_for_host(id);
  }
  if (!f->n) {
    words(id,(u32)f->output,64)[2]=f->acc;
    words(id,(u32)f->output,64)[3]=~f->acc;
    returning(id,f->acc); return READY;
  }
  u64 n=f->n-1, acc=f->acc+1;
  u32 target=selection==0?0:selection==1?(f->tag==1?2:1):(u32)(n&3);
  install(id,target,n,acc,0,1); return READY;
}
static NOINLINE u32 step_self(u32 id) { return tail_step(id,get(id)->c.mode==2?2:0); }
static NOINLINE u32 step_a(u32 id) { return tail_step(id,get(id)->c.mode==2?2:1); }
static NOINLINE u32 step_b(u32 id) { return tail_step(id,get(id)->c.mode==2?2:1); }
static NOINLINE u32 step_indirect(u32 id) { return tail_step(id,2); }
static NOINLINE u32 step_ordinary(u32 id) {
  Routine *r=get(id); Frame *f=frame(id); verify_packet(id);
  if (!f->pc) {
    f->slot=address(id,16,16); f->flags=1; // Deliberately NOT eligible for replacement.
    words(id,f->slot,16)[0]=f->n; words(id,f->slot,16)[1]=~f->n;
    if (f->parent_slot!=NONE) {
      if (words(id,(u32)f->parent_slot,16)[0]!=f->n+1) fatal(id,CORRUPTION);
      ++r->c.borrow_checks;
    }
    ++r->c.effects; f->pc=1;
    if (f->n) { install(id,REC,f->n-1,0,f->slot,0); return READY; }
    return wait_for_host(id);
  }
  if (words(id,f->slot,16)[0]!=f->n || words(id,f->slot,16)[1]!=~f->n)
    fatal(id,CORRUPTION);
  if (f->pc==1 && f->n%4==2) {
    f->pc=2; r->c.mask=1; return wait_for_host(id); // Modeled required join.
  }
  if (f->n%4==1) ++r->c.guards;
  if (f->n%4==2) { ++r->c.joins; r->c.mask=0; }
  if (f->n%4==3) ++r->c.wraps; // Modeled post-call wrapping, NOT an Err implementation.
  returning(id,f->acc+1); return READY;
}
static NOINLINE u32 step_root(u32 id) {
  Routine *r=get(id); Frame *f=frame(id); verify_packet(id);
  if (!f->pc) {
    if (r->c.mode==5) (void)address(id,r->c.requested,64); // Fatal/near-limit fixture.
    r->c.root_slot=address(id,64,32);
    words(id,r->c.root_slot,64)[0]=MAGIC;
    words(id,r->c.root_slot,64)[1]=~MAGIC;
    f->pc=1; f->flags=1;
    u32 tag=r->c.mode==4?REC:r->c.mode==1?1:r->c.mode==2?3:0;
    install(id,tag,r->c.requested,0,NONE,0); return READY;
  }
  u64 answer=f->acc;
  if (r->c.mode!=4 && (words(id,r->c.root_slot,64)[2]!=answer ||
                       words(id,r->c.root_slot,64)[3]!=~answer)) fatal(id,CORRUPTION);
  if (r->c.mode==3 && !r->c.native_done) fatal(id,PROTOCOL);
  returning(id,answer); return DONE;
}
static u32 (*const steps[])(u32)={step_self,step_a,step_b,step_indirect,step_ordinary,step_root};

EXPORT void init(u32 id,u64 address_cap,u64 saved_cap,u32 mode,u64 count) {
  Routine *r=get(id); u32 generation=r->c.generation;
  if (address_cap>MAX || address_cap<RUNTIME+64 || saved_cap>MAX ||
      saved_cap<HEADER+80 || mode>5 || (mode!=5 && count>1000000) ||
      generation==0x7fffffffu) fatal(id,BAD_CONFIG);
  fill((u8 *)&r->c,sizeof(Control),0); r->c.generation=generation+1;
  r->c.a_limit=(u32)address_cap-RUNTIME; r->c.s_limit=(u32)saved_cap-HEADER;
  fill(r->address+r->c.a_limit,16,0xa5); fill(r->saved+r->c.s_limit,16,0xa5);
  r->c.current=NONE; r->c.mode=mode; r->c.requested=count;
  r->c.status=READY; r->c.handle_live=1;
  install(id,ROOT,0,0,NONE,0);
}
EXPORT u32 poll(u32 id,u32 quantum) {
  Routine *r=get(id);
  if (!quantum || r->c.current==NONE) return r->c.status;
  if (r->c.pending) return WAIT;
  while (quantum-- && r->c.current!=NONE) {
    u32 tag=frame(id)->tag; if (tag>ROOT) fatal(id,CORRUPTION);
    ++r->c.steps;
    u32 action=steps[tag](id);
    if (action==WAIT || action==DONE) return action;
  }
  return r->c.current==NONE?DONE:READY;
}
EXPORT u32 wake(u32 id,u64 token) {
  Routine *r=get(id);
  if (!r->c.pending || token!=r->c.token || (r->c.native_needed&&!r->c.native_done)) {
    ++r->c.stale_wakes; return 0;
  }
  r->c.pending=0; return 1;
}
EXPORT void request_cancel(u32 id) { get(id)->c.cancel_requested=1; }
EXPORT void drop_handle(u32 id) { get(id)->c.handle_live=0; }
EXPORT u32 complete_foreign(u32 id,u64 token) {
  Routine *r=get(id);
  if (!r->c.pending || token!=r->c.token || !r->c.native_needed || r->c.native_done) return 0;
  u64 *p=words(id,r->c.root_slot,64);
  if (p[4]!=(MAGIC^0x55ull) || p[5]!=~MAGIC) fatal(id,CORRUPTION);
  r->c.native_done=1; return 1;
}
EXPORT u32 canaries(u32 id) {
  Routine *r=get(id);
  for (u32 i=0;i<16;++i)
    if (r->address[r->c.a_limit+i]!=0xa5 || r->saved[r->c.s_limit+i]!=0xa5) return 0;
  return 1;
}
EXPORT u64 inspect(u32 id,u32 field) {
  Routine *r=get(id); Control *c=&r->c;
  switch (field) {
  case 0:return HEADER; case 1:return c->a_used+RUNTIME; case 2:return c->a_peak+RUNTIME;
  case 3:return c->s_used+HEADER; case 4:return c->s_peak+HEADER;
  case 5:return c->frames_peak; case 6:return c->effects; case 7:return c->token;
  case 8:return c->steps; case 9:return c->result; case 10:return c->status;
  case 11:return c->current; case 12:return c->frames; case 13:return c->generation;
  case 14:return (u32)(r->address+c->root_slot);
  case 15:return (u32)(r->address+c->root_slot+32);
  case 16:return c->a_used>=c->root_slot+64 && c->current!=NONE;
  case 17:return c->cancel_requested; case 18:return c->cancel_observed;
  case 19:return c->guards; case 20:return c->joins; case 21:return c->wraps;
  case 22:return c->borrow_checks; case 23:return c->stale_wakes;
  case 24:return sizeof(Routine); case 25:return RUNTIME;
  case 26:return (u32)(r->address+c->a_limit);
  case 27:return (u32)(r->saved+c->s_limit);
  case 28:return c->mask; case 29:return c->native_needed; case 30:return c->native_done;
  case 31:return c->current==NONE?NONE:frame(id)->tag;
  case 32:return c->current==NONE?NONE:frame(id)->pc;
  default:fatal(id,PROTOCOL);
  }
}
EXPORT void test_blocked_tail(u32 id,u32 obligation) {
  Frame *f=frame(id); f->flags=obligation;
  install(id,0,f->n,f->acc,0,1); // Deliberately malformed lowering, not a valid source call.
}
EXPORT u64 test_reserve(u32 id,u32 domain,u64 used,u64 bytes,u64 alignment,u64 cap) {
  return reserve(id,used,bytes,alignment,cap,domain?SAVED_LIMIT:ADDRESS_LIMIT);
}
