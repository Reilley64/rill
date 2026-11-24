// Chart of the exit cleanup order between two owned places. "First" and
// "second" are declaration order. Steps before the exit stay inside "Running".
type PerPlace<T> = { [place: number]: T };
type Vars = {
  phase: string;
  exit: string;
  cur: PerPlace<number>;
  status: PerPlace<{ [version: number]: string }>;
};

export const title = "Exit cleanup order between two owned values";

const now = (v: Vars, place: number): string => v.status[place][v.cur[place]];
const owns = (v: Vars, place: number): boolean => ["owned", "partial"].includes(now(v, place));
const disposing = (v: Vars, place: number): boolean =>
  ["overload", "fields"].includes(now(v, place));

const owned = (v: Vars): string =>
  owns(v, 1) && owns(v, 2)
    ? "both values"
    : owns(v, 1)
      ? "only the first value"
      : owns(v, 2)
        ? "only the second value"
        : "no value";

export const state = (v: Vars): string => {
  if (v.phase === "running") return "Running";
  if (v.phase === "panicked") return "Program ended by panic";
  if (v.phase === "done") return "Left";
  if (disposing(v, 2))
    return owns(v, 1)
      ? "Second value under disposal, first still owned"
      : "Second value under disposal, first not owned";
  if (disposing(v, 1))
    return owns(v, 2)
      ? "First value under disposal, second still owned"
      : "First value under disposal, second not owned";
  return `Exit cleanup, the routine owns ${owned(v)}`;
};

// These steps stay inside "Running".
const before: Record<string, string> = {
  Initialize: "a value is initialized",
  MoveOut: "a whole value is moved out",
  ExplicitDispose: "the routine calls dispose on a value",
  ExportFailure: "the failure moves into the caller's result",
};

export const step = (action: string, from: Vars, to: Vars): string | null => {
  switch (action) {
    case "ReturnNormally":
    case "Propagate":
      return `the routine returns or propagates a failure while it owns ${owned(from)}`;
    case "ExitCleanupBegins":
      return disposing(to, 2)
        ? "cleanup begins on the second value"
        : "cleanup begins on the first value";
    case "OverloadReturns":
    case "FieldCleanup":
      return "the value is disposed";
    case "Leave":
      return "cleanup is complete";
    case "Panic":
      return "a panic begins";
    case "Done":
      return null;
    default:
      return before[action] ?? action;
  }
};

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
