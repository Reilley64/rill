// Chart of one wrapper disposal: the custom disposer, then the remaining-field
// cleanup. The frame's own steps stay inside "No disposal in progress".
type Vars = { dstep: string };

export const title = "Disposal of a wrapper that owns a lock guard";

export const state = (v: Vars): string => {
  switch (v.dstep) {
    case "custom disposer":
      return "Custom disposer running, lock guard field still owned";
    case "field released":
      return "Custom disposer running, it released the lock guard field";
    case "protected release":
      return "Remaining-field cleanup owes the lock guard release";
    default:
      return "No disposal in progress";
  }
};

const steps: Record<string, string> = {
  WrittenRelease: "disposal of a wrapper begins",
  BeginReplacement: "disposal of a wrapper begins",
  CleanupNext: "disposal of a wrapper begins",
  DisposerReleasesField: "the custom disposer disposes the lock guard field",
  DisposerReturnsAfterRelease: "the custom disposer returns, no field is left to clean",
  EmptyDisposerReturns: "the custom disposer returns without touching the field",
  ProviderReleases: "the canonical provider releases the lock guard",
};

export const step = (action: string): string | null => steps[action] ?? null;
