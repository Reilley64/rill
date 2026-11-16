#include "probe.h"
/* These are fixture limits, not proposed Rill policy. */
NOINLINE int validate_capacity(U64 requested, U64 *effective) {
  if (!requested || requested < 4096 || requested > ~(U64)0 - 15) return 86;
  U64 rounded = (requested + 15) & ~(U64)15;
  *effective = rounded;
  if (rounded > 1048576) return 86;
  return 0;
}
/* Deliberately actual native stack objects, not a simulated arena. */
typedef struct { U8 bytes[32768]; } Big;
NOINLINE void *memcpy(void *to, const void *from, __SIZE_TYPE__ n) {
  volatile U8 *d = to; const volatile U8 *s = from;
  for (__SIZE_TYPE__ i = 0; i < n; ++i) d[i] = s[i];
  return to;
}
NOINLINE void *memset(void *to, int value, __SIZE_TYPE__ n) {
  volatile U8 *d = to;
  for (__SIZE_TYPE__ i = 0; i < n; ++i) d[i] = (U8)value;
  return to;
}
static NOINLINE void sample(Context *c) {
  U64 sp = stack_pointer();
  if (sp < c->min_sp) c->min_sp = sp;
  if (sp > c->max_sp) c->max_sp = sp;
  ++c->samples;
}
NOINLINE Big aggregate_callee(Big value, Context *c,
    U64 a, U64 b, U64 d, U64 e, U64 f, U64 g, U64 h, U64 i,
    U64 j, U64 k, U64 l, U64 m) {
  _Alignas(4096) volatile U8 frame[262144];
  for (U64 x = 0; x < sizeof(frame); ++x) frame[x] = (U8)(x ^ 0x69);
  sample(c);
  if (a!=1 || b!=2 || d!=3 || e!=4 || f!=5 || g!=6 || h!=7 || i!=8 ||
      j!=9 || k!=10 || l!=11 || m!=12) c->ok = 0;
  for (U64 x = 0; x < sizeof(value.bytes); ++x) {
    if (value.bytes[x] != (U8)(x * 17 + 3)) c->ok = 0;
    value.bytes[x] ^= 0xa5;
  }
  for (U64 x = 0; x < sizeof(frame); ++x)
    if (frame[x] != (U8)(x ^ 0x69)) c->ok = 0;
  return value;
}
static NOINLINE void argument_builder(Context *c) {
  _Alignas(4096) volatile U8 frame[196608];
  Big input;
  ++c->entered;
  for (U64 x = 0; x < sizeof(frame); ++x) frame[x] = (U8)(x ^ 0xb7);
  for (U64 x = 0; x < sizeof(input.bytes); ++x) input.bytes[x] = (U8)(x * 17 + 3);
  Big result = aggregate_callee(input, c, 1,2,3,4,5,6,7,8,9,10,11,12);
  sample(c);
  for (U64 x = 0; x < sizeof(result.bytes); ++x) {
    if (result.bytes[x] != (U8)((U8)(x * 17 + 3) ^ 0xa5)) c->ok = 0;
    if (input.bytes[x] != (U8)(x * 17 + 3)) c->ok = 0;
  }
  for (U64 x = 0; x < sizeof(frame); ++x)
    if (frame[x] != (U8)(x ^ 0xb7)) c->ok = 0;
}
static NOINLINE void oversized(Context *c) {
  _Alignas(4096) volatile U8 frame[2097152];
  ++c->entered;
  for (U64 i = 0; i < sizeof(frame); ++i) frame[i] = (U8)i;
  sample(c);
  for (U64 i = 0; i < sizeof(frame); ++i) if (frame[i] != (U8)i) c->ok = 0;
}
/* This is OUTSIDE both large prologues and the aggregate argument builder.
 * The small thunk itself needs separately reserved bootstrap bytes. */
NOINLINE int admission(Context *c) {
  U64 sp = stack_pointer();
  c->admission_sp = sp;
  if (sp < c->low || sp > c->top || c->bound > sp - c->low) return 86;
  if (c->variant == 1) oversized(c);
  else argument_builder(c);
  return c->ok ? 0 : 87;
}
/* Logical-boundary counterexample only: physically mapped stack is large enough.
 * No object access outside actual frames, and no guard-page fault is required. */
NOINLINE int unchecked_control(Context *c) {
  c->admission_sp = stack_pointer();
  argument_builder(c);
  return c->ok ? 0 : 87;
}
