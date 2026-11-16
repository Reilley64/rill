#define _GNU_SOURCE
#include <assert.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include "probe.h"
extern int validate_capacity(U64, U64 *);
extern int unchecked_control(Context *);
int main(int argc, char **argv) {
  assert(argc == 5);
  const char *scenario = argv[1];
  U64 requested = strtoull(argv[2], NULL, 10), effective = 0;
  U64 bound = strtoull(argv[3], NULL, 10), kind = strtoull(argv[4], NULL, 10);
  if (validate_capacity(requested, &effective) != 0) {
    printf("{\"case\":\"%s\",\"fatal\":\"invalid capacity\",\"requested\":%llu,\"effective\":%llu}\n", scenario, requested, effective);
    fflush(stdout); _exit(86);
  }
  size_t page = (size_t)sysconf(_SC_PAGESIZE), physical = 4 * 1024 * 1024;
  U8 *map = mmap(NULL, physical + 2 * page, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
  assert(map != MAP_FAILED);
  assert(mprotect(map + page, physical, PROT_READ | PROT_WRITE) == 0);
  U8 *top = map + page + 3 * 1024 * 1024;
  U8 *low = top - effective;
  memset(low - 64, 0xd3, 64); memset(top, 0x9e, 64);
  Context c = {0};
  c.low = (U64)low; c.top = (U64)top; c.bound = bound;
  c.min_sp = ~(U64)0; c.ok = 1;
  c.variant = !strcmp(scenario, "oversized"); c.kind = kind; c.n = 1000000;
  int (*entry)(Context *) = admission;
  if (!strcmp(scenario, "unchecked")) entry = unchecked_control;
  if (!strcmp(scenario, "abi")) {
    entry = abi_root; c.owned = malloc(73); assert(c.owned);
    for (U64 i = 0; i < 73; ++i) c.owned[i] = (U8)(i ^ 0x63);
  }
  int status = switch_stack(top, entry, &c);
  int lower = 1, upper = 1;
  for (int i = 0; i < 64; ++i) { if (low[i-64] != 0xd3) lower = 0; if (top[i] != 0x9e) upper = 0; }
  if (!strcmp(scenario, "unchecked")) { assert(!lower && upper && status == 0); }
  else { assert(lower && upper); }
  if (status == 86) assert(c.entered == 0);
  if (!strcmp(scenario, "abi")) {
    assert(status == 0 && c.owned == NULL && c.releases == 1);
    assert(c.max_sp >= c.min_sp && c.max_sp - c.min_sp < 4096);
    assert(c.min_sp > c.low && c.max_sp < c.top);
  }
  printf("{\"case\":\"%s\",\"status\":%d,\"requested\":%llu,\"capacity\":%llu,\"bound\":%llu,\"bootstrap\":%llu,\"entered\":%llu,\"samples\":%llu,\"sp_span\":%llu,\"observed_depth\":%llu,\"lower_canary\":%s,\"upper_canary\":%s,\"ok\":%llu,\"kind\":%llu,\"releases\":%llu}\n",
      scenario,status,requested,effective,bound,c.admission_sp ? c.top-c.admission_sp : 0,c.entered,c.samples,
      c.samples ? c.max_sp-c.min_sp : 0,c.samples ? c.top-c.min_sp : 0,
      lower ? "true":"false",upper ? "true":"false",c.ok,c.kind,c.releases);
  assert(munmap(map, physical + 2 * page) == 0);
  /* External scheduler only. Fatal guest invocation is NEVER resumed; report
   * and exit run on original OS stack, not an exhausted managed guest stack. */
  fflush(stdout);
  if (status) _exit(status);
  return 0;
}
