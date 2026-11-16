// Ordinary-recursion control, NOT a proper-tail workaround.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const bytes = readFileSync(process.argv[2]);

async function unwind(depth, capacity) {
  let exports;
  const { instance } = await WebAssembly.instantiate(bytes, {
    env: { pause() { exports.asyncify_start_unwind(0); } },
  });
  exports = instance.exports;
  const start = 1024;
  const end = start + capacity;
  const view = new DataView(exports.memory.buffer);
  view.setUint32(0, start, true);
  view.setUint32(4, end, true);
  const canary = new Uint8Array(exports.memory.buffer, end, 64);
  canary.fill(0xa5);
  let trapped = false;
  try {
    exports.run(depth);
    exports.asyncify_stop_unwind();
  } catch (error) {
    assert(error instanceof WebAssembly.RuntimeError);
    trapped = true; // External embedding observation, not catchable Rill overflow.
  }
  return {
    depth, capacity, saved_bytes: view.getUint32(0, true) - start,
    adjacent_canary_changed: canary.some(byte => byte !== 0xa5), trapped,
  };
}
for (const depth of [16, 128, 1024]) {
  const observation = await unwind(depth, 32768);
  assert(!observation.trapped && !observation.adjacent_canary_changed);
  assert(observation.saved_bytes > depth * 4);
  console.log(JSON.stringify({ case: 'ordinary_asyncify_saved_history', ...observation }));
}
if (process.argv[3]) {
  const { instance } = await WebAssembly.instantiate(readFileSync(process.argv[3]));
  const e = instance.exports;
  const top = e.__stack_pointer.value;
  assert.equal(top, 1048576);
  const lower = 1024;
  const current = lower + 16; // Simulate earlier frames, inside the valid bounds.
  const canary = new Uint8Array(e.memory.buffer, current - 64, 16);
  canary.fill(0xa5);
  e.__set_stack_limits(top, lower); // Binaryen: upper base, lower limit.
  e.__stack_pointer.value = current;
  e.leaf(); // LLVM's 64-byte leaf red zone does not assign __stack_pointer.
  assert.equal(canary[0], 7);
  console.log(JSON.stringify({
    case: 'linear_stack_leaf_redzone_containment', contract: 'FAIL',
    capacity: top - lower, remaining_bytes: 16, frame_bytes: 64,
    adjacent_canary_changed: true, trapped: false,
  }));
}
const overflow = await unwind(128, 16);
assert(overflow.adjacent_canary_changed);
console.log(JSON.stringify({
  case: 'saved_budget_prewrite_containment', contract: 'FAIL', ...overflow,
}));
