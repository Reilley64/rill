----------------------------- MODULE OwnedValue -----------------------------
(* The run-time life of an owned value in one routine: no concurrency and no
   cancellation.

     type Holder = { first: Part, second: Part };
     dispose(holder: Holder) { ... }     // only when HasDisposeOverload

     function routine(): Result<...> {   // Frame = "function"
         let a: Holder = ...;            // place 1, declared first
         let b: Holder = ...;            // place 2, declared second
         // between statements: move a value or a field out, dispose(a),
         // a = <replacement>, return, propagate a failure with `?`, or panic
     }

   With Frame = "startup" the same statements are the program's startup, and
   the only ordinary exit is the end of startup.

   A place is a declaration rank (1 is declared first). A place holds at most
   two values in its life: version 1, and version 2 when the routine assigns
   the place again. Every value is a record of the same type, with one or two
   fields.

   Sources: RFC 0007 (replacement phases, destructor-safe partial moves,
   deterministic cleanup scheduling), RFC 0008 (postfix propagation and
   normal return), ADR 0005 (a panic ends the program without unwinding),
   docs/research/m6-prerequisites.md sections 5.2 and 5.3, and owner
   decision Q12 on bead ri-qlh (startup-scope values are cleaned up before
   the program exits). *)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
    Places,              \* declaration ranks of the owned places: {1} or {1, 2}
    Fields,              \* declaration ranks of the record's fields: {1} or {1, 2}
    HasDisposeOverload,  \* TRUE: the record type has a dispose overload
    Replaceable,         \* the places that the routine can assign again
    Dependent,           \* the place whose value borrows from Source, or 0
    Source,              \* the place that Dependent borrows from, or 0
    Frame,               \* "function" or "startup"
    Fault                \* "none", or a seeded fault that shows a rule can break

Faults == {
    "exit cleanup repeats ended values",
    "field cleanup ignores the overload",
    "field cleanup before the overload",
    "cleanup ignores dependencies",
    "cleanup in declaration order",
    "panic runs cleanup",
    "cleanup before export",
    "installation skips old disposal",
    "program exit skips cleanup"}

ASSUME /\ Places \in {{1}, {1, 2}}
       /\ Fields \in {{1}, {1, 2}}
       /\ HasDisposeOverload \in BOOLEAN
       /\ Replaceable \subseteq Places
       /\ \/ Dependent = 0 /\ Source = 0
          \/ Dependent \in Places /\ Source \in Places /\ Dependent # Source
       /\ Frame \in {"function", "startup"}
       /\ Fault \in Faults \cup {"none"}

VARIABLES
    phase,         \* "running", "exiting", "done" or "panicked"
    exit,          \* "none", "normal", "propagation" or "program exit"
    exported,      \* the propagated failure is in the caller's result
    cur,           \* the version that each place holds now
    status,        \* per place and version, the state of that value
    field,         \* per place, version and field, the state of that field
    begun,         \* per value, the number of disposals that began
    overloadRuns,  \* per value, the number of dispose overload calls
    order,         \* the places that the exit cleanup began on, in order
    frozen         \* the values and fields as they were when the panic began

vars == <<phase, exit, exported, cur, status, field, begun, overloadRuns,
          order, frozen>>

(* Value states.
     absent    never initialized (the place is Uninitialized)
     pending   a completed replacement that is not installed yet
     owned     Initialized
     partial   PartiallyMoved: some field moved out, some field still owned
     moved     Moved: ownership left the place
     overload  Disposing: the dispose overload is running
     fields    remaining-field cleanup
     disposed  Disposed
   Field states.
     owned, moved
     disposed  ended by the dispose overload
     cleaned   ended by remaining-field cleanup
     movedThenDisposed, disposedTwice  only a seeded fault reaches these *)
Versions == {1, 2}

Max(S) == CHOOSE x \in S : \A y \in S : y <= x
Min(S) == CHOOSE x \in S : \A y \in S : x <= y

DependsOn(d, s) == d = Dependent /\ s = Source /\ d # 0

Live(p, k) == status[p][k] \in {"pending", "owned", "partial", "overload", "fields"}
InDisposal == \E p \in Places, k \in Versions : status[p][k] \in {"overload", "fields"}
ReplacementPending == \E p \in Places : status[p][2] = "pending"
HasLiveDependent(s) ==
    \E d \in Places : DependsOn(d, s) /\ \E k \in Versions : Live(d, k)
SourcesInitialized(p) ==
    \A s \in Places : DependsOn(p, s) => status[s][cur[s]] = "owned"

(* The routine is between two statements. *)
Quiet == phase = "running" /\ ~InDisposal /\ ~ReplacementPending

(* A panic ends the program, so nothing runs after it. *)
Active == \/ phase \in {"running", "exiting"}
          \/ phase = "panicked" /\ Fault = "panic runs cleanup"

Init ==
    /\ phase = "running"
    /\ exit = "none"
    /\ exported = FALSE
    /\ cur = [p \in Places |-> 1]
    /\ status = [p \in Places |-> [k \in Versions |-> "absent"]]
    /\ field = [p \in Places |-> [k \in Versions |-> [f \in Fields |-> "owned"]]]
    /\ begun = [p \in Places |-> [k \in Versions |-> 0]]
    /\ overloadRuns = [p \in Places |-> [k \in Versions |-> 0]]
    /\ order = <<>>
    /\ frozen = <<>>

(* Disposal of one value begins: the dispose overload first when the type has
   one, then remaining-field cleanup. *)
StartDisposal(p, k) ==
    /\ begun' = [begun EXCEPT ![p][k] = @ + 1]
    /\ IF HasDisposeOverload /\ Fault # "field cleanup before the overload"
       THEN /\ status' = [status EXCEPT ![p][k] = "overload"]
            /\ overloadRuns' = [overloadRuns EXCEPT ![p][k] = @ + 1]
       ELSE /\ status' = [status EXCEPT ![p][k] = "fields"]
            /\ UNCHANGED overloadRuns

-----------------------------------------------------------------------------
(* Statements of the routine *)

Initialize ==
    \E p \in Places :
        /\ Quiet
        /\ cur[p] = 1
        /\ status[p][1] = "absent"
        /\ SourcesInitialized(p)
        /\ status' = [status EXCEPT ![p][1] = "owned"]
        /\ UNCHANGED <<phase, exit, exported, cur, field, begun, overloadRuns,
                       order, frozen>>

(* The whole value goes to another owner: a callee, another place or the
   routine's result. The model does not follow it further. *)
MoveOut ==
    \E p \in Places :
        /\ Quiet
        /\ status[p][cur[p]] = "owned"
        /\ ~HasLiveDependent(p)
        /\ status' = [status EXCEPT ![p][cur[p]] = "moved"]
        /\ UNCHANGED <<phase, exit, exported, cur, field, begun, overloadRuns,
                       order, frozen>>

(* Only an ordinary record gives up one field. The checker rejects the same
   statement for a type with a dispose overload (RFC 0007). *)
MoveFieldOut ==
    \E p \in Places, f \in Fields :
        /\ Quiet
        /\ ~HasDisposeOverload
        /\ Cardinality(Fields) > 1
        /\ status[p][cur[p]] \in {"owned", "partial"}
        /\ field[p][cur[p]][f] = "owned"
        /\ ~HasLiveDependent(p)
        /\ field' = [field EXCEPT ![p][cur[p]][f] = "moved"]
        /\ status' = [status EXCEPT ![p][cur[p]] =
               IF \E g \in Fields \ {f} : field[p][cur[p]][g] = "owned"
               THEN "partial" ELSE "moved"]
        /\ UNCHANGED <<phase, exit, exported, cur, begun, overloadRuns, order,
                       frozen>>

ExplicitDispose ==
    \E p \in Places :
        /\ Quiet
        /\ status[p][cur[p]] = "owned"
        /\ ~HasLiveDependent(p)
        /\ StartDisposal(p, cur[p])
        /\ UNCHANGED <<phase, exit, exported, cur, field, order, frozen>>

-----------------------------------------------------------------------------
(* Disposal of one value *)

OverloadDisposesField ==
    \E p \in Places, k \in Versions, f \in Fields :
        /\ Active
        /\ status[p][k] = "overload"
        /\ field[p][k][f] = "owned"
        /\ field' = [field EXCEPT ![p][k][f] = "disposed"]
        /\ UNCHANGED <<phase, exit, exported, cur, status, begun, overloadRuns,
                       order, frozen>>

OverloadMovesField ==
    \E p \in Places, k \in Versions, f \in Fields :
        /\ Active
        /\ status[p][k] = "overload"
        /\ field[p][k][f] = "owned"
        /\ field' = [field EXCEPT ![p][k][f] = "moved"]
        /\ UNCHANGED <<phase, exit, exported, cur, status, begun, overloadRuns,
                       order, frozen>>

(* The fields that remaining-field cleanup ends. *)
CleanupTargets(p, k) ==
    {f \in Fields : field[p][k][f] \in
        IF Fault = "field cleanup ignores the overload"
        THEN {"owned", "moved", "disposed"} ELSE {"owned"}}

OverloadReturns ==
    \E p \in Places, k \in Versions :
        /\ Active
        /\ status[p][k] = "overload"
        /\ status' = [status EXCEPT ![p][k] =
               IF /\ Fault # "field cleanup before the overload"
                  /\ CleanupTargets(p, k) # {}
               THEN "fields" ELSE "disposed"]
        /\ UNCHANGED <<phase, exit, exported, cur, field, begun, overloadRuns,
                       order, frozen>>

(* Remaining-field cleanup ends one field, in reverse declaration order. *)
FieldCleanup ==
    \E p \in Places, k \in Versions :
        /\ Active
        /\ status[p][k] = "fields"
        /\ LET targets == CleanupTargets(p, k)
               f == Max(targets)
               runOverloadNow == /\ HasDisposeOverload
                                 /\ Fault = "field cleanup before the overload"
           IN /\ field' = [field EXCEPT ![p][k][f] =
                     CASE @ = "owned" -> "cleaned"
                       [] @ = "moved" -> "movedThenDisposed"
                       [] OTHER -> "disposedTwice"]
              /\ status' = [status EXCEPT ![p][k] =
                     IF targets # {f} THEN "fields"
                     ELSE IF runOverloadNow THEN "overload" ELSE "disposed"]
              /\ overloadRuns' = [overloadRuns EXCEPT ![p][k] =
                     IF targets = {f} /\ runOverloadNow THEN @ + 1 ELSE @]
        /\ UNCHANGED <<phase, exit, exported, cur, begun, order, frozen>>

-----------------------------------------------------------------------------
(* Replacement: place = <new value> (RFC 0007 phases 2, 4 and 5) *)

(* The replacement value is complete and the routine owns it. The old value
   is Initialized, or it left the place earlier by a move or a dispose. *)
ReplacementComplete ==
    \E p \in Replaceable :
        /\ Quiet
        /\ cur[p] = 1
        /\ status[p][1] \in {"owned", "moved", "disposed"}
        /\ status[p][2] = "absent"
        /\ ~HasLiveDependent(p)
        /\ SourcesInitialized(p)
        /\ status' = [status EXCEPT ![p][2] = "pending"]
        /\ UNCHANGED <<phase, exit, exported, cur, field, begun, overloadRuns,
                       order, frozen>>

OldValueDisposalBegins ==
    \E p \in Places :
        /\ phase = "running"
        /\ ~InDisposal
        /\ status[p][2] = "pending"
        /\ status[p][1] = "owned"
        /\ StartDisposal(p, 1)
        /\ UNCHANGED <<phase, exit, exported, cur, field, order, frozen>>

Install ==
    \E p \in Places :
        /\ phase = "running"
        /\ ~InDisposal
        /\ status[p][2] = "pending"
        /\ \/ status[p][1] \in {"moved", "disposed"}
           \/ /\ Fault = "installation skips old disposal"
              /\ status[p][1] = "owned"
        /\ cur' = [cur EXCEPT ![p] = 2]
        /\ status' = [status EXCEPT ![p][2] = "owned"]
        /\ UNCHANGED <<phase, exit, exported, field, begun, overloadRuns, order,
                       frozen>>

-----------------------------------------------------------------------------
(* Exits *)

BeginExit(kind) ==
    /\ Quiet
    /\ phase' = "exiting"
    /\ exit' = kind
    /\ UNCHANGED <<exported, cur, status, field, begun, overloadRuns, order,
                   frozen>>

ReturnNormally == Frame = "function" /\ BeginExit("normal")

Propagate == Frame = "function" /\ BeginExit("propagation")

StartupCompletes == Frame = "startup" /\ BeginExit("program exit")

(* The failure moves into the caller's result. *)
ExportFailure ==
    /\ phase = "exiting"
    /\ exit = "propagation"
    /\ ~exported
    /\ ~InDisposal
    /\ exported' = TRUE
    /\ UNCHANGED <<phase, exit, cur, status, field, begun, overloadRuns, order,
                   frozen>>

SkipsCleanup == Fault = "program exit skips cleanup" /\ exit = "program exit"

Eligible(p) == status[p][cur[p]] \in {"owned", "partial"}
Unblocked(p) == Eligible(p) /\ ~\E d \in Places : Eligible(d) /\ DependsOn(d, p)

MayClean ==
    /\ \/ phase = "exiting"
       \/ phase = "panicked" /\ Fault = "panic runs cleanup"
    /\ ~InDisposal
    /\ ~SkipsCleanup
    /\ exit = "propagation" => exported \/ Fault = "cleanup before export"

(* Dependents before their sources, otherwise reverse declaration order. *)
NextToClean ==
    CASE Fault = "cleanup ignores dependencies" ->
             Max({p \in Places : Eligible(p)})
      [] Fault = "cleanup in declaration order" ->
             Min({p \in Places : Unblocked(p)})
      [] OTHER -> Max({p \in Places : Unblocked(p)})

ExitCleanupBegins ==
    /\ MayClean
    /\ \E p \in Places : Eligible(p)
    /\ order' = Append(order, NextToClean)
    /\ StartDisposal(NextToClean, cur[NextToClean])
    /\ UNCHANGED <<phase, exit, exported, cur, field, frozen>>

(* Seeded fault: the exit cleanup disposes a value that was moved out or
   disposed earlier. *)
ExitCleanupRepeatsEndedValue ==
    /\ Fault = "exit cleanup repeats ended values"
    /\ MayClean
    /\ \E p \in Places :
        /\ \/ status[p][cur[p]] = "moved" /\ begun[p][cur[p]] = 0
           \/ status[p][cur[p]] = "disposed" /\ begun[p][cur[p]] = 1
        /\ begun' = [begun EXCEPT ![p][cur[p]] = @ + 1]
    /\ UNCHANGED <<phase, exit, exported, cur, status, field, overloadRuns,
                   order, frozen>>

Leave ==
    /\ phase = "exiting"
    /\ ~InDisposal
    /\ SkipsCleanup \/ ~\E p \in Places : Eligible(p)
    /\ exit = "propagation" => exported
    /\ phase' = "done"
    /\ UNCHANGED <<exit, exported, cur, status, field, begun, overloadRuns,
                   order, frozen>>

(* A panic can begin in any statement, in a dispose overload and in exit
   cleanup. *)
Panic ==
    /\ phase \in {"running", "exiting"}
    /\ phase' = "panicked"
    /\ frozen' = <<status, field>>
    /\ UNCHANGED <<exit, exported, cur, status, field, begun, overloadRuns,
                   order>>

Done == phase \in {"done", "panicked"} /\ UNCHANGED vars

Next ==
    \/ Initialize \/ MoveOut \/ MoveFieldOut \/ ExplicitDispose
    \/ OverloadDisposesField \/ OverloadMovesField \/ OverloadReturns
    \/ FieldCleanup
    \/ ReplacementComplete \/ OldValueDisposalBegins \/ Install
    \/ ReturnNormally \/ Propagate \/ StartupCompletes
    \/ ExportFailure \/ ExitCleanupBegins \/ ExitCleanupRepeatsEndedValue
    \/ Leave \/ Panic
    \/ Done

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Rules *)

AllValues(P(_, _)) == \A p \in Places, k \in Versions : P(p, k)

(* R1. A value is disposed at most once, and its dispose overload runs at
   most once. *)
ValueDisposedAtMostOnce ==
    AllValues(LAMBDA p, k : begun[p][k] <= 1 /\ overloadRuns[p][k] <= 1)

(* R2. On every exit that is not a panic, each value the routine still owns
   is disposed exactly once. A value that was never initialized or that was
   moved out is not disposed here. *)
Settled(p, k) ==
    \/ status[p][k] \in {"absent", "moved"} /\ begun[p][k] = 0
    \/ /\ status[p][k] = "disposed"
       /\ begun[p][k] = 1
       /\ \A f \in Fields : field[p][k][f] \in {"moved", "disposed", "cleaned"}

Left(kind) == phase = "done" /\ exit = kind

NormalExitDisposesEachOwnedValueOnce == Left("normal") => AllValues(Settled)

PropagationExitDisposesEachOwnedValueOnce ==
    Left("propagation") => AllValues(Settled)

(* Owner decision Q12: startup-scope values are cleaned up before exit. *)
ProgramExitDisposesEachStartupValueOnce ==
    Left("program exit") => AllValues(Settled)

(* R3. A moved value is never disposed by the routine it left. *)
MovedValueNeverDisposed ==
    AllValues(LAMBDA p, k : status[p][k] = "moved" => begun[p][k] = 0)

(* R4. A field that was moved out or already disposed is never disposed. *)
MovedOrDisposedFieldNeverDisposed ==
    AllValues(LAMBDA p, k : \A f \in Fields :
        field[p][k][f] \notin {"movedThenDisposed", "disposedTwice"})

(* R5. The dispose overload runs, and returns, before remaining-field
   cleanup. *)
DisposeOverloadRunsBeforeFieldCleanup ==
    HasDisposeOverload =>
        AllValues(LAMBDA p, k :
            (status[p][k] = "fields" \/ \E f \in Fields : field[p][k][f] = "cleaned")
                => overloadRuns[p][k] >= 1 /\ status[p][k] # "overload")

(* R6. A source's disposal does not begin while a value that borrows from it
   is still live or under disposal. *)
DependentCleanedUpBeforeSource ==
    \A d \in Places, s \in Places :
        (DependsOn(d, s) /\ status[s][cur[s]] \in {"overload", "fields", "disposed"})
            => ~\E k \in Versions : Live(d, k)

(* R7. Without a dependency, exit cleanup runs in reverse declaration order. *)
CleanupInReverseDeclarationOrderOtherwise ==
    \A i \in 1..Len(order), j \in 1..Len(order) :
        (i < j /\ order[i] < order[j]) => DependsOn(order[i], order[j])

(* R8. A panic disposes nothing: after it no value or field changes. *)
PanicDisposesNothing == phase = "panicked" => frozen = <<status, field>>

(* R9. On a propagation exit the failure is in the caller's result before
   any cleanup begins. *)
FailureExportedBeforeCleanup ==
    (exit = "propagation" /\ ~exported) => Len(order) = 0

(* R10. A replacement is installed only after the old value left the place:
   disposed there, or moved out earlier. *)
OldValueGoneBeforeInstallation ==
    \A p \in Places : cur[p] = 2 => status[p][1] \in {"moved", "disposed"}

=============================================================================
