/* Standalone, 64-bit-only fixture ABI; no platform headers in mechanism code. */
typedef unsigned long long U64;
typedef unsigned char U8;
typedef struct {
  U64 low, top, bound, admission_sp, min_sp, max_sp, samples;
  U64 entered, ok, variant, kind, n, releases;
  U8 *owned;
} Context;
typedef struct { U8 bytes[257]; U8 *owned; } Packet;
#define NOINLINE __attribute__((noinline))
static __inline__ __attribute__((always_inline)) U64 stack_pointer(void) {
  U64 p;
#if defined(__aarch64__)
  __asm__ volatile("mov %0, sp" : "=r"(p) :: "memory");
#elif defined(__x86_64__)
  __asm__ volatile("mov %%rsp, %0" : "=r"(p) :: "memory");
#else
#error unsupported
#endif
  return p;
}
int admission(Context *);
int abi_root(Context *);
int switch_stack(void *, int (*)(Context *), Context *);
