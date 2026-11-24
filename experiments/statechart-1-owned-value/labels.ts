// Labels shared by view.ts and view-replacement.ts: the state of the owned
// place, and every step of the model.
type PerValue<T> = { [place: number]: { [version: number]: T } };
export type Vars = {
  phase: string;
  exit: string;
  exported: boolean;
  cur: { [place: number]: number };
  status: PerValue<string>;
};

export const value = (v: Vars): string => {
  const old = v.status[1][1];
  if (v.status[1][2] === "pending") {
    if (old === "owned") return "Replacement complete, old value still in place";
    if (old === "overload" || old === "fields") return "Old value under disposal";
    return "Old value gone, replacement not yet installed";
  }
  const installed = v.cur[1] === 2;
  switch (v.status[1][v.cur[1]]) {
    case "absent":
      return "Uninitialized";
    case "owned":
      if (installed && old === "owned") return "Replacement installed over a live old value";
      return installed ? "Initialized, replacement installed" : "Initialized";
    case "partial":
      return "Partially moved";
    case "moved":
      return "Moved out";
    case "overload":
      return "Disposing: dispose overload running";
    case "fields":
      return "Remaining-field cleanup";
    default:
      return "Disposed";
  }
};

const steps: Record<string, string> = {
  Initialize: "the value is initialized",
  MoveOut: "the whole value is moved out",
  ExplicitDispose: "the routine calls dispose on the value",
  OverloadDisposesField: "the dispose overload disposes a field",
  OverloadMovesField: "the dispose overload moves a field out",
  ReplacementComplete: "the replacement value is complete",
  OldValueDisposalBegins: "disposal of the old value begins",
  ReturnNormally: "the routine returns",
  Propagate: "the routine propagates a failure",
  StartupCompletes: "startup completes",
  ExportFailure: "the failure moves into the caller's result",
  ExitCleanupBegins: "exit cleanup begins on the value",
  ExitCleanupRepeatsEndedValue: "FAULT: exit cleanup disposes the ended value again",
  Panic: "a panic begins",
};

export const step = (action: string, from: Vars, to: Vars): string | null => {
  const after = to.status[1][from.status[1][2] === "pending" ? 1 : from.cur[1]];
  switch (action) {
    case "Done":
      return null;
    case "MoveFieldOut":
      return after === "partial" ? "one field is moved out" : "the last owned field is moved out";
    case "OverloadReturns":
      return after === "fields"
        ? "the dispose overload returns, fields remain"
        : "the dispose overload returns, no field is left to clean";
    case "FieldCleanup":
      if (after === "overload") return "FAULT: the dispose overload runs after field cleanup";
      return after === "fields"
        ? "a remaining field is cleaned up"
        : "the last remaining field is cleaned up";
    case "Install":
      return from.status[1][1] === "owned"
        ? "FAULT: the replacement is installed over the old value"
        : "the replacement is installed";
    case "Leave":
      if (to.exit === "normal") return "cleanup is complete, the routine returns to its caller";
      if (to.exit === "program exit") return "cleanup is complete, the program exits";
      return "cleanup is complete, the caller receives the failure";
    default:
      return steps[action] ?? action;
  }
};
