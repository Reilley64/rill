// Chart of one disposal of a record with a dispose overload: the overload,
// then remaining-field cleanup. Every other step of the routine stays inside
// "No disposal in progress".
type PerValue<T> = { [place: number]: { [version: number]: T } };
type Vars = {
  status: PerValue<string>;
  field: PerValue<{ [field: number]: string }>;
};

export const title = "Disposal of one value: dispose overload, then remaining-field cleanup";

const count = (n: number): string => ["no field", "one field", "two fields"][n];

export const state = (v: Vars): string => {
  for (const version of [1, 2]) {
    const status = v.status[1][version];
    if (status !== "overload" && status !== "fields") continue;
    const owned = Object.values(v.field[1][version]).filter((f) => f === "owned").length;
    return status === "overload"
      ? `Dispose overload running, ${count(owned)} still owned`
      : `Remaining-field cleanup, ${count(owned)} left`;
  }
  return "No disposal in progress";
};

const steps: Record<string, string> = {
  ExplicitDispose: "disposal of a value begins",
  OldValueDisposalBegins: "disposal of a value begins",
  ExitCleanupBegins: "disposal of a value begins",
  OverloadDisposesField: "the dispose overload moves or disposes a field",
  OverloadMovesField: "the dispose overload moves or disposes a field",
  OverloadReturns: "the dispose overload returns",
  FieldCleanup: "cleanup disposes the last-declared remaining field",
};

export const step = (action: string): string | null => steps[action] ?? null;
