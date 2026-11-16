// Synchronous Wasmtime embedding: no async future/fiber is parked at a yield.
#ifdef NDEBUG
#error Assertions required
#endif
#include <assert.h>
#include <inttypes.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wasmtime.h>
#define M 1048576
#define MAGIC UINT64_C(0x123456789abcdef0)
typedef struct {
  wasm_engine_t *engine; wasmtime_store_t *store; wasmtime_context_t *ctx;
  wasmtime_instance_t instance; wasmtime_memory_t memory; unsigned fatal_reason;
} Env;
static void check(wasmtime_error_t *e) {
  if (!e) return;
  wasm_name_t msg; wasmtime_error_message(e,&msg);
  fprintf(stderr,"%.*s\n",(int)msg.size,msg.data); abort();
}
static wasm_trap_t *fatal_callback(void *env,wasmtime_caller_t *caller,
                                   const wasmtime_val_t *a,size_t n,
                                   wasmtime_val_t *result,size_t nr) {
  (void)caller; (void)result; assert(n==2 && nr==0);
  ((Env *)env)->fatal_reason=(unsigned)a[1].of.i32;
  return wasmtime_trap_new("fatal probe invocation",22);
}
static uint64_t invoke(Env *e,const char *name,const uint64_t *args,size_t n) {
  wasmtime_extern_t x;
  assert(wasmtime_instance_export_get(e->ctx,&e->instance,name,strlen(name),&x));
  assert(x.kind==WASMTIME_EXTERN_FUNC);
  wasm_functype_t *ty=wasmtime_func_type(e->ctx,&x.of.func);
  const wasm_valtype_vec_t *params=wasm_functype_params(ty), *returns=wasm_functype_results(ty);
  assert(params->size==n && n<=6 && returns->size<=1);
  size_t nr=returns->size; wasmtime_val_t a[6],result;
  for (size_t i=0;i<n;++i) {
    wasm_valkind_t k=wasm_valtype_kind(params->data[i]);
    if (k==WASM_I32) a[i]=(wasmtime_val_t){.kind=WASMTIME_I32,.of.i32=(int32_t)args[i]};
    else { assert(k==WASM_I64); a[i]=(wasmtime_val_t){.kind=WASMTIME_I64,.of.i64=(int64_t)args[i]}; }
  }
  wasm_functype_delete(ty);
  wasm_trap_t *trap=NULL;
  check(wasmtime_func_call(e->ctx,&x.of.func,a,n,&result,nr,&trap));
  if (trap) { wasm_name_t msg; wasm_trap_message(trap,&msg); fprintf(stderr,"%.*s\n",(int)msg.size,msg.data); abort(); }
  return !nr?0:result.kind==WASMTIME_I32?(uint32_t)result.of.i32:(uint64_t)result.of.i64;
}
#define CALL(e,name,...) invoke(e,name,(uint64_t[]){__VA_ARGS__},sizeof((uint64_t[]){__VA_ARGS__})/sizeof(uint64_t))
#define GET(e,id,field) CALL(e,"inspect",id,field)
static unsigned finish(Env *e,unsigned id) {
  unsigned waits=0;
  for (unsigned rounds=0;rounds<100000;++rounds) {
    uint64_t status=CALL(e,"poll",id,4096);
    assert(CALL(e,"canaries",id)==1);
    if (!status) return waits;
    if (status==1) { ++waits; assert(CALL(e,"wake",id,GET(e,id,7))==1); }
  }
  abort();
}
static void setup(Env *e,unsigned id,unsigned mode,uint64_t n) {
  CALL(e,"init",id,M,M,mode,n);
}
typedef struct {
  pthread_mutex_t mutex; pthread_cond_t cond; int go,done;
  uint64_t input[2],output[2];
} Job;
static void *worker(void *ptr) {
  Job *j=ptr; assert(pthread_mutex_lock(&j->mutex)==0);
  while (!j->go) assert(pthread_cond_wait(&j->cond,&j->mutex)==0);
  j->output[0]=j->input[0]^0x55; j->output[1]=j->input[1]; j->done=1;
  assert(pthread_mutex_unlock(&j->mutex)==0); return NULL;
}
static void foreign_growth(Env *e) {
  setup(e,0,3,1000000);
  while (CALL(e,"poll",0,4096)!=1) {}
  uint64_t token=GET(e,0,7), input=GET(e,0,14), output=GET(e,0,15);
  assert(GET(e,0,16)==1 && GET(e,0,29)==1);
  Job *j=calloc(1,sizeof(*j)); assert(j);
  assert(pthread_mutex_init(&j->mutex,NULL)==0 && pthread_cond_init(&j->cond,NULL)==0);
  uint8_t *base=wasmtime_memory_data(e->ctx,&e->memory);
  size_t size=wasmtime_memory_data_size(e->ctx,&e->memory);
  assert(input<=size && sizeof(j->input)<=size-input);
  memcpy(j->input,base+input,sizeof(j->input));
  assert(j->input[0]==MAGIC && j->input[1]==~MAGIC);
  uintptr_t before=(uintptr_t)base; base=NULL; // Never retain/reuse the guest pointer.
  pthread_t thread; assert(pthread_create(&thread,NULL,worker,j)==0);
  CALL(e,"drop_handle",0); CALL(e,"request_cancel",0);
  assert(CALL(e,"wake",0,token)==0 && GET(e,0,16)==1);
  // poll has RETURNED: no C future or live guest call is accessed concurrently.
  uint64_t old_pages; check(wasmtime_memory_grow(e->ctx,&e->memory,1,&old_pages));
  uint8_t *fresh=wasmtime_memory_data(e->ctx,&e->memory);
  assert((uintptr_t)fresh!=before && GET(e,0,16)==1);
  assert(memcmp(fresh+input,j->input,sizeof(j->input))==0);
  assert(pthread_mutex_lock(&j->mutex)==0); assert(!j->done); j->go=1;
  assert(pthread_cond_signal(&j->cond)==0 && pthread_mutex_unlock(&j->mutex)==0);
  assert(pthread_join(thread,NULL)==0 && j->done);
  // Actual foreign access has ceased. Reacquire extent/base for exclusive copyback.
  fresh=wasmtime_memory_data(e->ctx,&e->memory); size=wasmtime_memory_data_size(e->ctx,&e->memory);
  assert(output<=size && sizeof(j->output)<=size-output && GET(e,0,16)==1);
  memcpy(fresh+output,j->output,sizeof(j->output));
  assert(pthread_cond_destroy(&j->cond)==0 && pthread_mutex_destroy(&j->mutex)==0); free(j);
  assert(CALL(e,"complete_foreign",0,token)==1 && CALL(e,"wake",0,token)==1);
  unsigned waits=finish(e,0); assert(GET(e,0,9)==1000000 && GET(e,0,18)==1);
  printf("{\"case\":\"native_copy_completion_after_legal_relocation\",\"relocated\":true,\"early_wake_rejected\":true,\"remaining_waits\":%u,\"result\":1000000}\n",waits);
}
static void fatal_pending_foreign(Env *e) {
  setup(e,0,3,1000000);
  while (CALL(e,"poll",0,4096)!=1) {}
  uint64_t input=GET(e,0,14);
  assert(GET(e,0,16)==1 && GET(e,0,29)==1 && GET(e,0,30)==0);
  Job *j=calloc(1,sizeof(*j)); assert(j);
  assert(pthread_mutex_init(&j->mutex,NULL)==0 && pthread_cond_init(&j->cond,NULL)==0);
  size_t size=wasmtime_memory_data_size(e->ctx,&e->memory);
  assert(input<=size && sizeof(j->input)<=size-input);
  memcpy(j->input,wasmtime_memory_data(e->ctx,&e->memory)+input,sizeof(j->input));
  pthread_t thread; assert(pthread_create(&thread,NULL,worker,j)==0);
  CALL(e,"drop_handle",0); CALL(e,"request_cancel",0);
  // Inject terminal guest failure while the logical operation is still pending.
  // Only the external embedding observes the trap; never resume this invocation.
  wasmtime_extern_t f;
  assert(wasmtime_instance_export_get(e->ctx,&e->instance,"test_reserve",12,&f));
  assert(f.kind==WASMTIME_EXTERN_FUNC);
  wasmtime_val_t args[6]={
    {.kind=WASMTIME_I32,.of.i32=0},{.kind=WASMTIME_I32,.of.i32=0},
    {.kind=WASMTIME_I64,.of.i64=0},{.kind=WASMTIME_I64,.of.i64=1},
    {.kind=WASMTIME_I64,.of.i64=0},{.kind=WASMTIME_I64,.of.i64=M}},result;
  wasm_trap_t *trap=NULL;
  wasmtime_error_t *error=wasmtime_func_call(e->ctx,&f.of.func,args,6,&result,1,&trap);
  // A host-created trap can be surfaced as a C-API error, unlike a core trap.
  assert((trap || error) && e->fatal_reason==4);
  if(trap)wasm_trap_delete(trap);
  if(error)wasmtime_error_delete(error);
  wasmtime_store_delete(e->store); e->store=NULL; e->ctx=NULL;
  // The worker owns no guest address and has not yet accessed its copy.
  assert(pthread_mutex_lock(&j->mutex)==0); assert(!j->done); j->go=1;
  assert(pthread_cond_signal(&j->cond)==0 && pthread_mutex_unlock(&j->mutex)==0);
  assert(pthread_join(thread,NULL)==0 && j->done);
  assert(j->output[0]==(MAGIC^0x55) && j->output[1]==~MAGIC);
  assert(pthread_cond_destroy(&j->cond)==0 && pthread_mutex_destroy(&j->mutex)==0); free(j);
  puts("{\"case\":\"fatal_pending_guest_store_teardown\",\"worker_access_after_store_drop\":true,\"copy_freed_after_join\":true,\"guest_reentered_after_fatal\":false,\"copyback_after_fatal\":false}");
}
int main(int argc,char **argv) {
  assert(argc==3); Env e={0};
  wasm_config_t *cfg=wasm_config_new();
  wasmtime_config_strategy_set(cfg,WASMTIME_STRATEGY_CRANELIFT);
  wasmtime_config_cranelift_opt_level_set(cfg,atoi(argv[2])?WASMTIME_OPT_LEVEL_SPEED:WASMTIME_OPT_LEVEL_NONE);
  wasmtime_config_wasm_tail_call_set(cfg,false);
  wasmtime_config_max_wasm_stack_set(cfg,32768);
  wasmtime_config_memory_may_move_set(cfg,true);
  wasmtime_config_memory_reservation_set(cfg,0);
  wasmtime_config_memory_reservation_for_growth_set(cfg,0);
  e.engine=wasm_engine_new_with_config(cfg);
  e.store=wasmtime_store_new(e.engine,NULL,NULL); e.ctx=wasmtime_store_context(e.store);
  FILE *file=fopen(argv[1],"rb"); assert(file);
  assert(fseek(file,0,SEEK_END)==0); long length=ftell(file); assert(length>0); rewind(file);
  uint8_t *bytes=malloc((size_t)length); assert(bytes);
  assert(fread(bytes,1,(size_t)length,file)==(size_t)length); fclose(file);
  wasmtime_module_t *module; check(wasmtime_module_new(e.engine,bytes,(size_t)length,&module)); free(bytes);
  wasm_functype_t *ty=wasm_functype_new_2_0(wasm_valtype_new_i32(),wasm_valtype_new_i32());
  wasmtime_func_t func; wasmtime_func_new(e.ctx,ty,fatal_callback,&e,NULL,&func); wasm_functype_delete(ty);
  wasmtime_extern_t import={.kind=WASMTIME_EXTERN_FUNC,.of.func=func}; wasm_trap_t *trap=NULL;
  check(wasmtime_instance_new(e.ctx,module,&import,1,&e.instance,&trap)); assert(!trap);
  wasmtime_extern_t memory; assert(wasmtime_instance_export_get(e.ctx,&e.instance,"memory",6,&memory));
  assert(memory.kind==WASMTIME_EXTERN_MEMORY); e.memory=memory.of.memory;
  for (unsigned mode=0;mode<3;++mode) {
    setup(&e,0,mode,1000000); unsigned waits=finish(&e,0);
    assert(GET(&e,0,9)==1000000 && GET(&e,0,6)==1000001 && GET(&e,0,5)==2);
    printf("{\"case\":\"wasmtime_sync_dispatch\",\"mode\":%u,\"n\":1000000,\"waits\":%u,\"saved_peak\":%" PRIu64 ",\"frames_peak\":2,\"engine_stack_limit\":32768,\"tail_feature\":false}\n",mode,waits,GET(&e,0,4));
  }
  setup(&e,0,4,8000); setup(&e,1,4,8000);
  for (unsigned id=0;id<2;++id) while (CALL(&e,"poll",id,4096)!=1) {}
  uint64_t sum=GET(&e,0,3)+GET(&e,1,3); assert(sum>M && GET(&e,0,3)<M && GET(&e,1,3)<M);
  finish(&e,0); finish(&e,1); assert(GET(&e,0,9)==8001 && GET(&e,1,9)==8001);
  printf("{\"case\":\"wasmtime_two_live_ordinary_chains\",\"each_depth\":8000,\"combined_saved\":%" PRIu64 "}\n",sum);
  foreign_growth(&e); assert(!e.fatal_reason);
  fatal_pending_foreign(&e); assert(!e.store && e.fatal_reason==4);
  wasmtime_module_delete(module); wasm_engine_delete(e.engine);
}
