// Chart of the frame: where it is, and whether a cancellation request is
// clear, pending or delivered. Disposal sub-steps stay inside one state.
type Vars = {
  pc: string;
  request: string;
  exit: string;
  content: { destination: string; replacement: string };
};

export const title = "Lock guard release across replacement and cancellation";

const exits: Record<string, string> = {
  normal: "normal exit",
  propagation: "propagation exit",
  delivered: "delivered cancellation exit",
};

const frame = (v: Vars): string => {
  switch (v.pc) {
    case "entry check":
      return "Entry budget check";
    case "loop":
      return v.content.destination === "vacant"
        ? "Loop, old lock guard already released"
        : "Loop, owns both lock guards";
    case "written release":
      return "Written dispose releases the old lock guard";
    case "old disposal":
      return "Old value under disposal";
    case "installation":
      return "Old value gone, replacement not yet installed";
    case "post-install check":
      return "Replacement installed, next budget check";
    case "suffix":
      return "Ordinary suffix";
    case "exit cleanup":
      return `Cleanup on ${exits[v.exit]}`;
    default:
      return `Left by ${exits[v.exit]}`;
  }
};

export const state = (v: Vars) => ({
  Frame: frame(v),
  "Cancellation request": v.request[0].toUpperCase() + v.request.slice(1),
});

const steps: Record<string, string> = {
  RequestArrives: "a cancellation request arrives",
  EntryCheckPasses: "the check passes without delivery",
  PostInstallCheckPasses: "the check passes without delivery",
  DeliveredAtEntry: "the check yields and delivers the cancellation",
  DeliveredAtBackedge: "the loop backedge yields and delivers the cancellation",
  DeliveredAtPostInstallCheck: "the check yields and delivers the cancellation",
  DeliveredBeforeInstallation: "FAULT: the cancellation is delivered before installation",
  WrittenRelease: "the body disposes the old value",
  BeginReplacement: "the replacement is complete, delivery is deferred",
  ProviderReleases: "the lock guard is released",
  EmptyDisposerReturns: "the custom disposer returns",
  DisposerReturnsAfterRelease: "the custom disposer returns",
  ReturnNormally: "the frame returns",
  Propagate: "the frame propagates a failure",
  CleanupNext: "cleanup starts on the next owned value",
  CleanupDone: "every owned lock guard is released",
};

export const step = (action: string, from: Vars, to: Vars): string | null => {
  if (action === "Install")
    return to.exit === "delivered"
      ? "the replacement is installed, unmasking delivers the cancellation"
      : "the replacement is installed";
  if (action === "Done") return null;
  return steps[action] ?? action;
};

export const group = (v: Vars): string =>
  v.pc === "exited"
    ? "left"
    : ["written release", "old disposal", "installation", "exit cleanup"].includes(v.pc)
      ? "delivery deferred"
      : "delivery possible";

export const colors = { "delivery possible": "blue", "delivery deferred": "orange", left: "teal" };
