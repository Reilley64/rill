// Chart of one owned place and the routine that owns it, as two regions.
// What a dispose overload does to each field stays inside one state; see
// view-disposal.ts for that.
import { step as label, value, type Vars } from "./labels.ts";

export const title = "The life of one owned value in one routine";

const routine = (v: Vars): string => {
  if (v.phase === "running") return "Running";
  if (v.phase === "panicked") return "Program ended by panic";
  if (v.phase === "exiting") {
    if (v.exit === "normal") return "Normal exit: cleaning up";
    if (v.exit === "program exit") return "Startup complete: cleaning up the startup scope";
    return v.exported
      ? "Propagation exit: cleaning up"
      : "Propagation exit: failure not yet in the caller's result";
  }
  if (v.exit === "normal") return "Left by normal exit";
  if (v.exit === "program exit") return "Program exited";
  return "Left by propagation exit";
};

export const state = (v: Vars) => ({ "Owned value": value(v), Routine: routine(v) });

export const step = label;

export const group = (v: Vars): string =>
  v.phase === "panicked"
    ? "ended by panic, no cleanup"
    : v.phase === "done"
      ? "left"
      : v.phase === "exiting"
        ? "exit cleanup"
        : "running";

export const colors = {
  running: "blue",
  "exit cleanup": "orange",
  left: "teal",
  "ended by panic, no cleanup": "pink",
};
