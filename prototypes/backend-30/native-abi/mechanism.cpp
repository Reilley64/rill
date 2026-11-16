// Standalone manual lowering fixture. Not Rill source, a checker, or public ABI.
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <new>
#include <type_traits>

using U8 = std::uint8_t;
using U64 = std::uint64_t;
using U128 = unsigned __int128;
using I128 = __int128;
#define NI __attribute__((noinline))

// Keep any compiler-generated aggregate copy/zero helpers inside the audited
// LLVM23 body, rather than silently depending on libc in the transfer graph.
extern "C" NI void *memcpy(void *d, const void *s, std::size_t n) noexcept {
  auto *dst = static_cast<volatile U8 *>(d);
  auto *src = static_cast<const volatile U8 *>(s);
  for (std::size_t i = 0; i < n; ++i) dst[i] = src[i];
  return d;
}
extern "C" NI void *memset(void *d, int x, std::size_t n) noexcept {
  auto *dst = static_cast<volatile U8 *>(d);
  for (std::size_t i = 0; i < n; ++i) dst[i] = static_cast<U8>(x);
  return d;
}

// Darwin lowers some zero intrinsics to bzero even with -fno-builtin.
extern "C" NI void bzero(void *d, std::size_t n) noexcept { memset(d, 0, n); }
extern "C" NI void __bzero(void *d, std::size_t n) noexcept { memset(d, 0, n); }

struct Capture { U64 salt; U8 bytes[41]; };
using Invoke = U64 (*)(const Capture *, U64);
NI U64 capture_invoke(const Capture *p, U64 x) { return p->salt ^ x ^ p->bytes[17]; }
union Variant { U128 wide; double real; U8 bytes[23]; };
struct Value {
  U128 wide;
  I128 negative;
  std::int64_t i64;
  std::int32_t i32;
  std::int16_t i16;
  std::int8_t i8;
  bool boolean;
  float f;
  double d;
  unsigned tag;
  Variant variant;
  const U8 *borrow;
  U8 *buffer;
  std::size_t length;
  Capture *capture;
  Invoke invoke;
  bool owns_buffer, owns_capture;
};
struct alignas(16) A { Value value; U8 shape[7]; };
struct alignas(32) B { U8 shape[39]; Value value; };
struct alignas(64) C { U64 prefix[5]; Value value; U8 shape[67]; };
union Slot { A a; B b; C c; };
struct Packet { Slot slot; unsigned active; };
// Every source-like function returns this SAME record. No public callable ABI.
struct Result { Value value; U64 transfers; unsigned final_layout; };
static_assert(std::is_trivially_copyable_v<Value> && std::is_trivially_destructible_v<Slot>);
static_assert(sizeof(A) != sizeof(B) && sizeof(B) != sizeof(C));
static_assert(alignof(A) == 16 && alignof(B) == 32 && alignof(C) == 64);
static_assert(sizeof(Packet) <= 512 && sizeof(Result) <= 256);
constexpr unsigned DEAD = 3;
constexpr U64 N = 1000000;
constexpr U64 SALT = 0x9182736455aabbccULL;
struct Context {
  U64 steps = 0, samples = 0, min_sp = ~U64(0), max_sp = 0;
  U64 moves = 0, capture_frees = 0, buffer_frees = 0, copy_frees = 0;
  U64 root_sp = 0, max_depth = 0, edges[3][3] = {};
  bool ok = true;
  const U8 *borrow_identity = nullptr;
  U8 *buffer_identity = nullptr;
  Capture *capture_identity = nullptr;
};

NI Value *value(Packet *p) {
  switch (p->active) {
    case 0: return &p->slot.a.value;
    case 1: return &p->slot.b.value;
    default: return &p->slot.c.value;
  }
}
NI U8 shape_byte(unsigned layout, std::size_t i) { return static_cast<U8>(layout * 43 + i * 7); }
NI void construct(Packet *p, unsigned layout) {
  // Placement new explicitly starts the chosen trivial union member's lifetime.
  // No reinterpret_cast, inactive-member reads, or aliasing extension is used.
  p->active = layout;
  switch (layout) {
    case 0:
      ::new (&p->slot.a) A{};
      for (std::size_t i = 0; i < sizeof(A::shape); ++i) p->slot.a.shape[i] = shape_byte(layout, i);
      break;
    case 1:
      ::new (&p->slot.b) B{};
      for (std::size_t i = 0; i < sizeof(B::shape); ++i) p->slot.b.shape[i] = shape_byte(layout, i);
      break;
    default:
      ::new (&p->slot.c) C{};
      for (unsigned i = 0; i < 5; ++i) p->slot.c.prefix[i] = SALT + i;
      for (std::size_t i = 0; i < sizeof(C::shape); ++i) p->slot.c.shape[i] = shape_byte(layout, i);
  }
}
NI void retire(Packet *p) {
  switch (p->active) {
    case 0: p->slot.a.~A(); break;
    case 1: p->slot.b.~B(); break;
    default: p->slot.c.~C();
  }
  p->active = DEAD;
}
NI U128 wide_bits(U64 t) { return (U128(SALT ^ t) << 64) | (0xfedcba9876543210ULL ^ t); }
NI std::uint32_t float_bits(U64 t) { return t & 1 ? 0x7fc12345U : 0x80000000U; }
NI U64 double_bits(U64 t) { return t & 1 ? 0x7ff8123456789abcULL : 0x8000000000000000ULL; }
NI void scalar_fields(Value *v, U64 t) {
  v->wide = wide_bits(t);
  v->negative = -I128(wide_bits(t) >> 1);
  v->i64 = -std::int64_t(t) - 0x123456789LL;
  v->i32 = -std::int32_t(t) - 1234567;
  v->i16 = -std::int16_t(t % 30000);
  v->i8 = -std::int8_t(t % 120);
  v->boolean = (t & 1) != 0;
  v->f = __builtin_bit_cast(float, float_bits(t));
  v->d = __builtin_bit_cast(double, double_bits(t));
  v->tag = unsigned(t % 3);
  switch (v->tag) {
    case 0: ::new (&v->variant.wide) U128(wide_bits(t)); break;
    case 1: ::new (&v->variant.real) double(__builtin_bit_cast(double, double_bits(t))); break;
    default:
      // Explicit placement constructs the array member (C++20 nonallocating new).
      ::new (static_cast<void *>(&v->variant.bytes)) U8[23]{};
      for (unsigned i = 0; i < 23; ++i) v->variant.bytes[i] = U8(t + i * 11);
  }
}
NI void check_value(const Value *v, Context *c, U64 t, bool has_capture) {
  c->ok &= v->wide == wide_bits(t) && v->negative == -I128(wide_bits(t) >> 1);
  c->ok &= v->i64 == -std::int64_t(t) - 0x123456789LL && v->i32 == -std::int32_t(t) - 1234567;
  c->ok &= v->i16 == -std::int16_t(t % 30000) && v->i8 == -std::int8_t(t % 120);
  c->ok &= v->boolean == ((t & 1) != 0);
  c->ok &= __builtin_bit_cast(std::uint32_t, v->f) == float_bits(t);
  c->ok &= __builtin_bit_cast(U64, v->d) == double_bits(t);
  c->ok &= v->tag == t % 3;
  switch (v->tag) {
    case 0: c->ok &= v->variant.wide == wide_bits(t); break;
    case 1: c->ok &= __builtin_bit_cast(U64, v->variant.real) == double_bits(t); break;
    default: for (unsigned i = 0; i < 23; ++i) c->ok &= v->variant.bytes[i] == U8(t + i * 11);
  }
  c->ok &= v->borrow == c->borrow_identity && v->buffer == c->buffer_identity;
  c->ok &= v->length == 73 && v->owns_buffer;
  for (unsigned i = 0; i < 31; ++i) c->ok &= v->borrow[i] == U8(i * 3 + 5);
  for (unsigned i = 0; i < 73; ++i) c->ok &= v->buffer[i] == U8(i ^ 0x63);
  c->ok &= v->owns_capture == has_capture;
  if (has_capture) {
    c->ok &= v->capture == c->capture_identity && v->invoke == capture_invoke;
    c->ok &= v->capture->salt == SALT;
    for (unsigned i = 0; i < 41; ++i) c->ok &= v->capture->bytes[i] == U8(i * 13 + 9);
  } else c->ok &= !v->capture && !v->invoke;
}
NI void check_shape(Packet *p, Context *c) {
  c->ok &= reinterpret_cast<std::uintptr_t>(&p->slot) % alignof(Slot) == 0;
  switch (p->active) {
    case 0:
      for (std::size_t i = 0; i < sizeof(A::shape); ++i) c->ok &= p->slot.a.shape[i] == shape_byte(0, i);
      break;
    case 1:
      for (std::size_t i = 0; i < sizeof(B::shape); ++i) c->ok &= p->slot.b.shape[i] == shape_byte(1, i);
      break;
    default:
      for (unsigned i = 0; i < 5; ++i) c->ok &= p->slot.c.prefix[i] == SALT + i;
      for (std::size_t i = 0; i < sizeof(C::shape); ++i) c->ok &= p->slot.c.shape[i] == shape_byte(2, i);
  }
}
NI void check_poison(const Value *src, Context *c) {
  c->ok &= !src->buffer && !src->capture && !src->invoke && src->length == 0;
  c->ok &= !src->owns_buffer && !src->owns_capture;
}
NI void move_value(Value *dst, Value *src, Context *c) {
  // Trivial union copy also starts the corresponding active variant's lifetime.
  // Ownership is logical, explicit, and poisoned BEFORE the source is retired.
  *dst = *src;
  src->buffer = nullptr;
  src->capture = nullptr;
  src->invoke = nullptr;
  src->length = 0;
  src->owns_buffer = src->owns_capture = false;
  check_poison(src, c);
  ++c->moves;
}
static inline U64 sp() {
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
extern "C" NI void abi_observe(Packet *p, Context *c, U64 expected) {
  U64 here = sp();
  if (here < c->min_sp) c->min_sp = here;
  if (here > c->max_sp) c->max_sp = here;
  if (c->root_sp - here > c->max_depth) c->max_depth = c->root_sp - here;
  ++c->samples;
  c->ok &= p->active < DEAD && (expected == DEAD || p->active == expected);
  check_shape(p, c);
  check_value(value(p), c, c->steps, true);
}
extern "C" NI U64 abi_step(Packet *src, Packet *dst, Context *c) {
  c->ok &= dst->active == DEAD;
  unsigned old = src->active, next = (old + 1) % 3;
  construct(dst, next);
  move_value(value(dst), value(src), c);
  // Check the transported bits BEFORE advancing the fixture's scalar/tag state.
  // Otherwise regenerating expected values could mask a broken record move.
  check_value(value(dst), c, c->steps, true);
  check_shape(dst, c);
  retire(src);
  c->ok &= src->active == DEAD;
  ++c->steps;
  ++c->edges[old][next];
  scalar_fields(value(dst), c->steps);
  return next;
}
extern "C" NI void abi_finish(Packet *src, Result *out, Context *c) {
  out->final_layout = src->active;
  out->transfers = c->steps;
  move_value(&out->value, value(src), c);
  retire(src);
  Value *v = &out->value;
  c->ok &= v->invoke(v->capture, c->steps) == (SALT ^ c->steps ^ U8(17 * 13 + 9));
  // The final callee discharges the independent capture obligation. The result
  // still owns the distinct buffer; root consumes and disposes that later.
  v->capture->~Capture();
  std::free(v->capture);
  v->capture = nullptr;
  v->invoke = nullptr;
  v->owns_capture = false;
  ++c->capture_frees;
}
extern "C" void abi_dispatch(U64, U64, Packet *, Packet *, Result *, Context *);
NI bool root(unsigned mode) {
  Packet slots[2];
  slots[0].active = slots[1].active = DEAD;
  Result result{};
  U8 borrowed[31];
  for (unsigned i = 0; i < 31; ++i) borrowed[i] = U8(i * 3 + 5);
  Context ctx;
  ctx.borrow_identity = borrowed;
  construct(&slots[0], 0);
  Value *v = value(&slots[0]);
  v->buffer = static_cast<U8 *>(std::malloc(73));
  void *capture_storage = std::malloc(sizeof(Capture));
  U8 *independent_copy = static_cast<U8 *>(std::malloc(73));
  if (!v->buffer || !capture_storage || !independent_copy) std::abort();
  v->capture = ::new (capture_storage) Capture{};
  ctx.buffer_identity = v->buffer;
  ctx.capture_identity = v->capture;
  v->length = 73;
  v->owns_buffer = v->owns_capture = true;
  v->borrow = borrowed;
  v->invoke = capture_invoke;
  v->capture->salt = SALT;
  for (unsigned i = 0; i < 41; ++i) v->capture->bytes[i] = U8(i * 13 + 9);
  for (unsigned i = 0; i < 73; ++i) v->buffer[i] = U8(i ^ 0x63);
  // One independent explicit deep copy, created BEFORE entering the tail chain.
  for (unsigned i = 0; i < 73; ++i) independent_copy[i] = v->buffer[i];
  ctx.ok &= independent_copy != v->buffer;
  scalar_fields(v, 0);
  ctx.root_sp = sp();
  abi_dispatch(mode, N, &slots[0], &slots[1], &result, &ctx); // ordinary lifetime root
  check_value(&result.value, &ctx, N, false);
  ctx.ok &= slots[0].active == DEAD && slots[1].active == DEAD;
  ctx.ok &= ctx.steps == N && ctx.samples == N + 1 && ctx.moves == N + 1;
  ctx.ok &= result.transfers == N && result.final_layout == N % 3;
  ctx.ok &= ctx.capture_frees == 1 && ctx.max_sp - ctx.min_sp <= 4096 && ctx.max_depth <= 8192;
  U64 consumed = 0;
  for (unsigned i = 0; i < 73; ++i) {
    ctx.ok &= independent_copy[i] == U8(i ^ 0x63);
    ctx.ok &= result.value.buffer[i] == independent_copy[i];
    consumed += result.value.buffer[i];
  }
  std::free(result.value.buffer);
  result.value.buffer = nullptr;
  result.value.owns_buffer = false;
  ++ctx.buffer_frees;
  std::free(independent_copy);
  ++ctx.copy_frees;
  ctx.ok &= ctx.buffer_frees == 1 && ctx.copy_frees == 1;
  std::printf("{\"mode\":%u,\"ok\":%s,\"transfers\":%llu,\"samples\":%llu,\"moves\":%llu,"
              "\"sp_span\":%llu,\"sampled_depth\":%llu,\"capture_frees\":%llu,\"buffer_frees\":%llu,"
              "\"copy_frees\":%llu,\"consumed_sum\":%llu,\"edges\":[%llu,%llu,%llu],"
              "\"record_sizes\":[%zu,%zu,%zu],\"record_alignments\":[%zu,%zu,%zu],"
              "\"value_offsets\":[%zu,%zu,%zu],\"slot_bytes\":%zu,\"two_packet_bytes\":%zu,"
              "\"result_bytes\":%zu,\"fixed_root_objects\":%zu}\n",
              mode, ctx.ok ? "true" : "false", (unsigned long long)ctx.steps,
              (unsigned long long)ctx.samples, (unsigned long long)ctx.moves,
              (unsigned long long)(ctx.max_sp - ctx.min_sp), (unsigned long long)ctx.max_depth,
              (unsigned long long)ctx.capture_frees, (unsigned long long)ctx.buffer_frees,
              (unsigned long long)ctx.copy_frees, (unsigned long long)consumed,
              (unsigned long long)ctx.edges[0][1], (unsigned long long)ctx.edges[1][2],
              (unsigned long long)ctx.edges[2][0], sizeof(A), sizeof(B), sizeof(C),
              alignof(A), alignof(B), alignof(C), offsetof(A, value), offsetof(B, value), offsetof(C, value),
              sizeof(Slot), sizeof(slots), sizeof(result), sizeof(slots) + sizeof(result) + sizeof(borrowed) + sizeof(ctx));
  return ctx.ok;
}
int main() {
  bool ok = true;
  for (unsigned mode = 0; mode < 3; ++mode) ok &= root(mode);
  return ok ? 0 : 1;
}
