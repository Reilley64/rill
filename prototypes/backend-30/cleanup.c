// Explicit worklist/checked-arithmetic MODEL; no compiler-generated Rill cleanup.
#ifdef NDEBUG
#error This experiment requires assertions
#endif
#include <assert.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Node { struct Node *next; size_t id; bool released, waited; } Node;
typedef struct { Node *node; unsigned phase; } Frame;
typedef struct {
  Frame *frames; size_t length, capacity, growths, fail_after;
  size_t entered, released, yields, pending, total;
  bool cancel_requested, mask;
} Work;
static uintptr_t sp_low = UINTPTR_MAX, sp_high;
static void sample(void) {
  uintptr_t sp;
#if defined(__aarch64__)
  __asm__ volatile("mov %0, sp" : "=r"(sp));
#elif defined(__x86_64__)
  __asm__ volatile("mov %%rsp, %0" : "=r"(sp));
#else
#error Unsupported sampler architecture
#endif
  if (sp < sp_low) sp_low = sp;
  if (sp > sp_high) sp_high = sp;
}
static void push(Work *w, Node *node) {
  if (w->length == w->capacity) {
    if (w->growths++ == w->fail_after) {
      fputs("fatal injected worklist allocation failure; no recursive fallback\n", stderr);
      abort();
    }
    assert(w->capacity <= SIZE_MAX / (2 * sizeof(Frame)));
    size_t capacity = w->capacity ? w->capacity * 2 : 8;
    Frame *frames = realloc(w->frames, capacity * sizeof(Frame));
    if (!frames) abort();
    w->frames = frames; w->capacity = capacity;
  }
  w->frames[w->length++] = (Frame){node, 0};
}
// A false return is a suspended callback. Caller resumes this exact work state.
static bool dispatch(Work *w) {
  assert(w->mask);
  while (w->length) {
    sample();
    Frame *f = &w->frames[w->length - 1]; Node *node = f->node;
    assert(!node->released);
    switch (f->phase) {
    case 0:
      assert(node->id == w->entered++); f->phase = 1; break;
    case 1:
      if (!node->waited && node->id % 1024 == 0) {
        node->waited = true; assert(w->pending == 0); w->pending = 1;
        ++w->yields; return false;
      }
      if (w->pending) w->pending = 0; // Model callback completion acknowledgement.
      f->phase = 2; break;
    case 2:
      f->phase = 3;
      if (node->next) push(w, node->next);
      break;
    case 3:
      assert(!w->pending && (!node->next || node->next->released));
      assert(node->id == w->total - 1 - w->released++);
      node->released = true; --w->length; break;
    default: abort();
    }
  }
  return true;
}
static bool reserve(uint64_t used, uint64_t bytes, uint64_t align,
                    uint64_t capacity, uint64_t *end) {
  if (!align || (align & (align - 1)) || used > UINT64_MAX - (align - 1)) return false;
  uint64_t start = (used + align - 1) & ~(align - 1);
  if (start > capacity || bytes > capacity - start) return false;
  *end = start + bytes; return true;
}
static void arithmetic(void) {
  uint64_t end = 77;
  assert(reserve(33, 64, 64, 128, &end) && end == 128);
  assert(!reserve(33, 65, 64, 128, &end) && end == 128);
  assert(!reserve(UINT64_MAX, 1, 16, UINT64_MAX, &end) && end == 128);
  assert(!reserve(0, 1, 0, 128, &end) && end == 128);
  assert(!reserve(0, 1, 3, 128, &end) && end == 128);
  assert(reserve(UINT64_MAX - 15, 15, 16, UINT64_MAX, &end) && end == UINT64_MAX);
  uint8_t arena[96]; memset(arena, 0xa5, sizeof(arena));
  assert(reserve(0, 32, 16, 32, &end)); memset(arena, 0, (size_t)end);
  assert(!reserve(0, 33, 16, 32, &end));
  for (size_t i = 32; i < sizeof(arena); ++i) assert(arena[i] == 0xa5);
  puts("{\"case\":\"checked_alignment_arena_model\",\"canary_intact\":true}");
}
int main(int argc, char **argv) {
  assert(argc == 2);
  bool inject = strcmp(argv[1], "fail") == 0;
  const size_t count = 200000;
  Node *nodes = calloc(count, sizeof(Node)); assert(nodes);
  for (size_t i = 0; i < count; ++i) {
    nodes[i].id = i;
    if (i + 1 < count) nodes[i].next = &nodes[i + 1];
  }
  // Complete outer-to-inner cause-like rendering with constant native traversal
  // state. This is not Err<E>, canonical display or a total output-memory bound.
  size_t rendered = 0; uint64_t checksum = 0; char text[32];
  for (Node *n = nodes; n; n = n->next) {
    int bytes = snprintf(text, sizeof(text), "%zu", n->id);
    assert(bytes > 0 && (size_t)bytes < sizeof(text));
    for (int j = 0; j < bytes; ++j) checksum += (unsigned char)text[j];
    ++rendered; sample();
  }
  assert(rendered == count && checksum > 0);
  Work w = {.fail_after = inject ? 2 : SIZE_MAX, .mask = true, .total = count};
  push(&w, nodes);
  while (!dispatch(&w)) {
    w.cancel_requested = true;
    assert(w.mask && w.pending == 1 && !w.frames[w.length - 1].node->released);
  }
  assert(w.cancel_requested && w.entered == count && w.released == count);
  assert(w.yields == 196 && w.pending == 0 && sp_high - sp_low < 4096);
  w.mask = false; // Only now would this model permit cancellation delivery.
  printf("{\"case\":\"ordered_suspending_cleanup_cause_model\",\"nodes\":%zu,"
         "\"yields\":%zu,\"worklist_peak_bytes\":%zu,\"sp_span_bytes\":%" PRIuPTR "}\n",
         count, w.yields, w.capacity * sizeof(Frame), sp_high - sp_low);
  free(w.frames); free(nodes); arithmetic();
}
