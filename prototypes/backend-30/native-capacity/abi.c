#include "probe.h"
extern void free(void *);
extern void packet_dispatch(U64, U64, Packet *, U8 *, Context *);
NOINLINE void packet_sample(Context *c) {
  U64 sp = stack_pointer();
  if (sp < c->min_sp) c->min_sp = sp;
  if (sp > c->max_sp) c->max_sp = sp;
  ++c->samples;
}
NOINLINE void packet_finish(Packet *p, U8 *result, Context *c) {
  for (U64 i = 0; i < 257; ++i) {
    U64 hits = c->n / 257 + (i != 0 && i <= c->n % 257);
    if (p->bytes[i] != (U8)(i * 29 + 7 + hits)) c->ok = 0;
    result[i] = p->bytes[i] ^ 0x5a;
  }
  for (U64 i = 0; i < 73; ++i) if (p->owned[i] != (U8)(i ^ 0x63)) c->ok = 0;
  /* Actual moved allocation is released by the final callee, once, not by root. */
  free(p->owned);
  p->owned = (void *)0;
  ++c->releases;
}
NOINLINE int abi_root(Context *c) {
  _Alignas(64) Packet packet;
  _Alignas(64) U8 result[257];
  for (U64 i = 0; i < 257; ++i) { packet.bytes[i] = (U8)(i * 29 + 7); result[i] = 0; }
  packet.owned = c->owned;
  c->owned = (void *)0; /* Explicit move: root owes no buffer disposal. */
  packet_dispatch(c->kind, c->n, &packet, result, c); /* ordinary owner stays live */
  for (U64 i = 0; i < 257; ++i) {
    U64 hits = c->n / 257 + (i != 0 && i <= c->n % 257);
    if (result[i] != (U8)((U8)(i * 29 + 7 + hits) ^ 0x5a)) c->ok = 0;
  }
  if (packet.owned || c->releases != 1 || c->samples != c->n + 1) c->ok = 0;
  return c->ok ? 0 : 87;
}
