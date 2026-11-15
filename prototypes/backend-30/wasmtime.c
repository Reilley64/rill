// Candidate embedding experiment only; NOT selection of a Rill architecture.
#ifdef NDEBUG
#error This experiment requires assertions
#endif
#include <assert.h>
#include <inttypes.h>
#include <pthread.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wasmtime.h>

static void check(wasmtime_error_t *e) {
  if (!e) return;
  wasm_name_t msg;
  wasmtime_error_message(e, &msg);
  fprintf(stderr, "%.*s\n", (int)msg.size, msg.data);
  wasm_name_delete(&msg);
  wasmtime_error_delete(e);
  abort();
}

typedef struct { uint8_t *base; size_t length, mapped; } Stack;
static _Atomic size_t stacks_live, stacks_peak, stack_bytes_live, stack_bytes_peak, stacks_created;
static _Atomic uintptr_t fiber_low, fiber_high;
static void peak(_Atomic size_t *slot, size_t value) {
  size_t old = atomic_load(slot);
  while (old < value && !atomic_compare_exchange_weak(slot, &old, value)) {}
}
static uint8_t *stack_top(void *env, size_t *length) {
  Stack *s = env; *length = s->length;
  return s->base + s->mapped;
}
static void stack_free(void *env) {
  Stack *s = env;
  assert(atomic_fetch_sub(&stacks_live, 1) > 0);
  assert(atomic_fetch_sub(&stack_bytes_live, s->length) >= s->length);
  assert(munmap(s->base, s->mapped) == 0); free(s);
}
static wasmtime_error_t *stack_new(void *env, size_t size, bool zeroed,
                                   wasmtime_stack_memory_t *out) {
  (void)env; (void)zeroed;
  size_t page = (size_t)sysconf(_SC_PAGESIZE);
  assert(size <= SIZE_MAX - (2 * page));
  size = (size + page - 1) / page * page;
  Stack *s = malloc(sizeof(*s)); assert(s);
  s->length = size; s->mapped = size + page;
  s->base = mmap(NULL, s->mapped, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
  assert(s->base != MAP_FAILED);
  assert(mprotect(s->base + page, size, PROT_READ | PROT_WRITE) == 0);
  fiber_low = (uintptr_t)(s->base + page);
  fiber_high = fiber_low + size;
  ++stacks_created;
  peak(&stacks_peak, atomic_fetch_add(&stacks_live, 1) + 1);
  peak(&stack_bytes_peak, atomic_fetch_add(&stack_bytes_live, size) + size);
  *out = (wasmtime_stack_memory_t){s, stack_top, stack_free};
  return NULL;
}

typedef struct {
  uint64_t calls, yields, active, peak, finalized;
  uintptr_t low, high;
  bool yield_once;
} Stats;
static bool resume(void *env) {
  Stats *s = env;
  if (s->yield_once) { s->yield_once = false; ++s->yields; return false; }
  return true;
}
static void finalize(void *env) {
  Stats *s = env; assert(s->active == 1); --s->active; ++s->finalized;
}
static void sample(void *env, wasmtime_caller_t *caller,
                   const wasmtime_val_t *args, size_t nargs,
                   wasmtime_val_t *results, size_t nresults,
                   wasm_trap_t **trap, wasmtime_async_continuation_t *cont) {
  (void)caller; (void)results; (void)trap;
  assert(nargs == 1 && nresults == 0 && args[0].kind == WASMTIME_I64);
  Stats *s = env;
  uintptr_t sp;
#if defined(__aarch64__)
  __asm__ volatile("mov %0, sp" : "=r"(sp));
#elif defined(__x86_64__)
  __asm__ volatile("mov %%rsp, %0" : "=r"(sp));
#else
#error Unsupported sampler architecture
#endif
  assert(sp >= fiber_low && sp < fiber_high);
  if (sp < s->low) s->low = sp;
  if (sp > s->high) s->high = sp;
  ++s->calls; assert(s->active == 0); ++s->active;
  if (s->active > s->peak) s->peak = s->active;
  s->yield_once = ((uint64_t)args[0].of.i64 % 1024 == 0);
  *cont = (wasmtime_async_continuation_t){resume, s, finalize};
}
static void finish(wasmtime_call_future_t *f) {
  uint64_t polls = 0;
  while (!wasmtime_call_future_poll(f)) assert(++polls < 10000000);
  wasmtime_call_future_delete(f);
}
static wasmtime_module_t *module_wat(wasm_engine_t *e, const char *wat) {
  wasm_byte_vec_t bytes; check(wasmtime_wat2wasm(wat, strlen(wat), &bytes));
  wasmtime_module_t *m; check(wasmtime_module_new(e, (uint8_t *)bytes.data, bytes.size, &m));
  wasm_byte_vec_delete(&bytes); return m;
}
static wasmtime_func_t exported(wasmtime_context_t *ctx,
                                wasmtime_instance_t *i, const char *name) {
  wasmtime_extern_t x;
  assert(wasmtime_instance_export_get(ctx, i, name, strlen(name), &x));
  assert(x.kind == WASMTIME_EXTERN_FUNC); return x.of.func;
}

// Worker owns the independent copy until joined. No guest pointer is retained.
typedef struct {
  pthread_mutex_t lock; pthread_cond_t cond; bool go, done;
  uint8_t bytes[4096]; uint64_t checksum;
} Copy;
static void *borrower(void *p) {
  Copy *c = p;
  assert(pthread_mutex_lock(&c->lock) == 0);
  while (!c->go) assert(pthread_cond_wait(&c->cond, &c->lock) == 0);
  for (size_t i = 0; i < sizeof(c->bytes); ++i) c->checksum += c->bytes[i];
  c->done = true;
  assert(pthread_mutex_unlock(&c->lock) == 0); return NULL;
}
static void growth_copy(wasm_engine_t *engine) {
  const char *wat = "(module (memory (export \"memory\") 1 64)"
      "(func (export \"grow\") (result i32) i32.const 4 memory.grow)"
      "(func (export \"crash\") unreachable))";
  wasmtime_module_t *m = module_wat(engine, wat);
  wasmtime_store_t *store = wasmtime_store_new(engine, NULL, NULL);
  wasmtime_context_t *ctx = wasmtime_store_context(store);
  wasmtime_instance_t instance; wasm_trap_t *trap = NULL;
  check(wasmtime_instance_new(ctx, m, NULL, 0, &instance, &trap)); assert(!trap);
  wasmtime_extern_t memory;
  assert(wasmtime_instance_export_get(ctx, &instance, "memory", 6, &memory));
  assert(memory.kind == WASMTIME_EXTERN_MEMORY);
  uint8_t *base = wasmtime_memory_data(ctx, &memory.of.memory);
  assert(wasmtime_memory_data_size(ctx, &memory.of.memory) >= 4096);
  Copy *c = calloc(1, sizeof(*c)); assert(c);
  assert(pthread_mutex_init(&c->lock, NULL) == 0);
  assert(pthread_cond_init(&c->cond, NULL) == 0);
  for (size_t i = 0; i < sizeof(c->bytes); ++i) base[i] = (uint8_t)i;
  memcpy(c->bytes, base, sizeof(c->bytes));
  uintptr_t before = (uintptr_t)base; // Integer observation; never reuse base.
  base = NULL;
  pthread_t thread; assert(pthread_create(&thread, NULL, borrower, c) == 0);
  wasmtime_func_t grow = exported(ctx, &instance, "grow");
  wasmtime_val_t result;
  check(wasmtime_func_call(ctx, &grow, NULL, 0, &result, 1, &trap)); assert(!trap);
  assert(result.kind == WASMTIME_I32 && result.of.i32 == 1);
  uint8_t *fresh = wasmtime_memory_data(ctx, &memory.of.memory);
  assert((uintptr_t)fresh != before);
  assert(wasmtime_memory_data_size(ctx, &memory.of.memory) == 5 * 65536);
  assert(memcmp(fresh, c->bytes, sizeof(c->bytes)) == 0);
  wasmtime_func_t crash = exported(ctx, &instance, "crash");
  check(wasmtime_func_call(ctx, &crash, NULL, 0, NULL, 0, &trap)); assert(trap);
  wasm_trap_delete(trap);
  wasmtime_store_delete(store); // External copy/worker deliberately outlive store.
  assert(pthread_mutex_lock(&c->lock) == 0);
  assert(!c->done); c->go = true;
  assert(pthread_cond_signal(&c->cond) == 0);
  assert(pthread_mutex_unlock(&c->lock) == 0);
  assert(pthread_join(thread, NULL) == 0);
  assert(c->done && c->checksum == 522240);
  assert(pthread_mutex_destroy(&c->lock) == 0);
  assert(pthread_cond_destroy(&c->cond) == 0); free(c);
  wasmtime_module_delete(m);
  puts("{\"case\":\"growth_copy_trap_store_drop\",\"relocated\":true,\"borrower_completed_after_store_drop\":true}");
}
static void host_overflow(wasm_engine_t *e) {
  const char *wat = "(module (global $g (mut i32) (i32.const 0))"
      "(func $f (export \"run\") (param i32) (result i32)"
      "local.get 0 i32.eqz if (result i32) i32.const 0 else "
      "local.get 0 i32.const 1 i32.sub call $f "
      "global.get $g i32.const 1 i32.add global.set $g end))";
  wasmtime_module_t *m = module_wat(e, wat);
  wasmtime_store_t *s = wasmtime_store_new(e, NULL, NULL);
  wasmtime_context_t *ctx = wasmtime_store_context(s);
  wasmtime_instance_t i; wasm_trap_t *trap = NULL;
  check(wasmtime_instance_new(ctx, m, NULL, 0, &i, &trap)); assert(!trap);
  wasmtime_func_t f = exported(ctx, &i, "run");
  wasmtime_val_t arg = {.kind = WASMTIME_I32, .of.i32 = 1000000}, result;
  check(wasmtime_func_call(ctx, &f, &arg, 1, &result, 1, &trap)); assert(trap);
  wasmtime_trap_code_t code;
  assert(wasmtime_trap_code(trap, &code) && code == WASMTIME_TRAP_CODE_STACK_OVERFLOW);
  wasm_trap_delete(trap); wasmtime_store_delete(s); wasmtime_module_delete(m);
  puts("{\"case\":\"host_stack_overflow_isolated_instance\",\"stack_overflow_trap\":true}");
}
int main(int argc, char **argv) {
  assert(argc == 4);
  uint64_t n = strtoull(argv[2], NULL, 10); assert(n > 0 && n <= 100000000);
  wasm_config_t *cfg = wasm_config_new();
  wasmtime_config_strategy_set(cfg, WASMTIME_STRATEGY_CRANELIFT);
  wasmtime_config_cranelift_opt_level_set(cfg,
      atoi(argv[3]) == 0 ? WASMTIME_OPT_LEVEL_NONE : WASMTIME_OPT_LEVEL_SPEED);
  wasmtime_config_wasm_tail_call_set(cfg, true);
  wasmtime_config_max_wasm_stack_set(cfg, 256 * 1024);
  wasmtime_config_async_stack_size_set(cfg, 1024 * 1024);
  wasmtime_config_memory_may_move_set(cfg, true);
  wasmtime_config_memory_reservation_set(cfg, 0);
  wasmtime_config_memory_reservation_for_growth_set(cfg, 0);
  wasmtime_stack_creator_t creator = {NULL, stack_new, NULL};
  wasmtime_config_host_stack_creator_set(cfg, &creator);
  wasm_engine_t *e = wasm_engine_new_with_config(cfg);
  FILE *file = fopen(argv[1], "rb"); assert(file);
  assert(fseek(file, 0, SEEK_END) == 0); long len = ftell(file); assert(len > 0);
  rewind(file); uint8_t *bytes = malloc((size_t)len); assert(bytes);
  assert(fread(bytes, 1, (size_t)len, file) == (size_t)len); fclose(file);
  wasmtime_module_t *m; check(wasmtime_module_new(e, bytes, (size_t)len, &m)); free(bytes);
  wasmtime_store_t *store = wasmtime_store_new(e, NULL, NULL);
  wasmtime_context_t *ctx = wasmtime_store_context(store);
  wasmtime_linker_t *linker = wasmtime_linker_new(e);
  Stats stats = {0};
  wasm_functype_t *ty = wasm_functype_new_1_0(wasm_valtype_new_i64());
  check(wasmtime_linker_define_async_func(linker, "env", 3, "sample_step", 11,
                                         ty, sample, &stats, NULL));
  wasm_functype_delete(ty);
  wasmtime_instance_t instance; wasm_trap_t *trap = NULL; wasmtime_error_t *error = NULL;
  finish(wasmtime_linker_instantiate_async(linker, ctx, m, &instance, &trap, &error));
  check(error); assert(!trap);
  const char *names[] = {"run_self", "run_mutual", "run_indirect", "run_arguments", "run_sret"};
  for (size_t j = 0; j < 5; ++j) {
    stats = (Stats){.low = UINTPTR_MAX};
    wasmtime_func_t f = exported(ctx, &instance, names[j]);
    wasmtime_val_t arg = {.kind = WASMTIME_I64, .of.i64 = (int64_t)n}, result;
    // sret wrapper returns scalar after reading its root-owned return buffer.
    finish(wasmtime_func_call_async(ctx, &f, &arg, 1, &result, 1, &trap, &error));
    check(error); assert(!trap);
    assert(result.kind == WASMTIME_I64 && (uint64_t)result.of.i64 == n + (j == 3 ? 1015 : 0));
    assert(stats.calls == n + 1 && stats.finalized == stats.calls);
    assert(stats.active == 0 && stats.peak == 1 && stats.yields > 0);
    assert(stats.high - stats.low < 4096 && stacks_peak == 1);
    printf("{\"case\":\"async_%s\",\"n\":%" PRIu64 ",\"samples\":%" PRIu64
           ",\"sp_span_bytes\":%" PRIuPTR ",\"yields\":%" PRIu64
           ",\"live_continuations_peak\":%" PRIu64
           ",\"fiber_allocations\":%zu,\"fiber_bytes_peak\":%zu}\n",
           names[j], n, stats.calls, stats.high - stats.low, stats.yields,
           stats.peak, stacks_created, stack_bytes_peak);
  }
  wasmtime_store_delete(store); wasmtime_linker_delete(linker); wasmtime_module_delete(m);
  growth_copy(e); host_overflow(e);
  wasm_engine_delete(e); assert(stacks_live == 0);
}
