// Standalone mechanism probe, NOT a Rill runtime or overflow implementation.
#ifdef NDEBUG
#error This experiment requires assertions
#endif
#include <assert.h>
#include <inttypes.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

extern uint64_t run_self(uint64_t);
extern uint64_t run_mutual(uint64_t);
extern uint64_t run_indirect(uint64_t);
extern uint64_t run_arguments(uint64_t);
extern void run_aggregate(uint64_t, uint64_t *);
extern void run_sret(uint64_t, uint64_t *);
extern uint64_t ordinary_control(uint64_t);
#ifdef HAVE_BYVAL
extern uint64_t run_byval(uint64_t);
#endif

static uintptr_t low = UINTPTR_MAX, high;
static uint64_t samples, local_reads;
static uintptr_t stack_base, stack_end;

__attribute__((noinline)) void sample_step(uint64_t n) {
  (void)n;
  uintptr_t sp;
#if defined(__aarch64__)
  __asm__ volatile("mov %0, sp" : "=r"(sp));
#elif defined(__x86_64__)
  __asm__ volatile("mov %%rsp, %0" : "=r"(sp));
#else
#error Unsupported sampler architecture
#endif
  assert(sp >= stack_base && sp < stack_end);
  if (sp < low) low = sp;
  if (sp > high) high = sp;
  ++samples;
}

// Synchronous access ends before returning. No local pointer crosses a tail.
__attribute__((noinline)) void observe_local(const uint64_t *p) {
  sample_step(*p);
  ++local_reads;
}

static void reset(void) { low = UINTPTR_MAX; high = 0; samples = 0; }
static void report(const char *name, uint64_t n, int bounded) {
  uintptr_t span = high - low;
  if (bounded) assert(span < 4096);
  printf("{\"case\":\"%s\",\"n\":%" PRIu64
         ",\"samples\":%" PRIu64 ",\"sp_span_bytes\":%" PRIuPTR "}\n",
         name, n, samples, span);
}

static void *exercise(void *arg) {
  uint64_t n = *(uint64_t *)arg;
  uint64_t (*fns[])(uint64_t) = {run_self, run_mutual, run_indirect,
                                run_arguments};
  const char *names[] = {"self", "mutual", "indirect", "stack_arguments"};
  for (size_t i = 0; i < 4; ++i) {
    reset();
    uint64_t answer = fns[i](n);
    assert(answer == n + (i == 3 ? 1015 : 0));
    assert(samples == n + 1);
    report(names[i], n, 1);
  }
#ifdef HAVE_BYVAL
  reset(); assert(run_byval(n) == 42);
  assert(samples == n + 1); report("byval_256_bytes", n, 1);
#endif
  uint64_t pair[2];
  reset(); run_aggregate(n, pair);
  assert(pair[0] == n && pair[1] == 7);
  assert(samples == n + 1); report("aggregate", n, 1);
  reset(); run_sret(n, pair);
  assert(pair[0] == n && pair[1] == 9);
  assert(samples == n + 1); report("explicit_sret", n, 1);
  reset(); local_reads = 0;
  assert(ordinary_control(128) == 128);
  assert(local_reads == 257 && high - low >= 4096);
  report("ordinary_live_local_control", 128, 0);
  return NULL;
}

int main(int argc, char **argv) {
  assert(argc == 2);
  uint64_t n = strtoull(argv[1], NULL, 10);
  assert(n > 0 && n <= 100000000);
  size_t page = (size_t)sysconf(_SC_PAGESIZE);
  size_t capacity = 1024 * 1024;
  assert(capacity % page == 0);
  uint8_t *mapping = mmap(NULL, capacity + 2 * page, PROT_NONE,
                          MAP_PRIVATE | MAP_ANON, -1, 0);
  assert(mapping != MAP_FAILED);
  assert(mprotect(mapping + page, capacity, PROT_READ | PROT_WRITE) == 0);
  stack_base = (uintptr_t)(mapping + page);
  stack_end = stack_base + capacity;
  pthread_attr_t attr;
  assert(pthread_attr_init(&attr) == 0);
  assert(pthread_attr_setguardsize(&attr, 0) == 0);
  assert(pthread_attr_setstack(&attr, mapping + page, capacity) == 0);
  pthread_t thread;
  assert(pthread_create(&thread, &attr, exercise, &n) == 0);
  assert(pthread_join(thread, NULL) == 0);
  assert(pthread_attr_destroy(&attr) == 0);
  assert(munmap(mapping, capacity + 2 * page) == 0);
}
