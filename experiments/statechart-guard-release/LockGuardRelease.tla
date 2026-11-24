---------------------------- MODULE LockGuardRelease ----------------------------
(* One frame that owns two lock guards and replaces one with the other.

     function frame(again: bool, old: Holder, incoming: Holder): Result<...> {
         // entry budget check
         let mut current = old;
         while (again) { continue; }      // backedge budget check
         // optional: dispose(current);   // written release
         current = incoming;              // dispose old value, install replacement
         leaf();                          // post-install budget check
         // return, or propagate a failure with `?`
     }

   Holder is LockGuard<i32> (Wrapper = FALSE) or a record with one lock guard
   field and a custom disposer (Wrapper = TRUE).

   Sources: RFC 0007 (replacement phases 1 to 6, cleanup scheduling, protected
   actions, case O14), ADR 0009 (request against delivery; deferral), and the
   as-built decision functions ReplacementCancellation::after_install and
   PostInstallCheck::advance on implementation/m5-ownership-cleanup. *)
EXTENDS Naturals

CONSTANTS
    Wrapper,          \* TRUE: each lock guard is a field of a record with a custom disposer
    DeliveryEnabled,  \* FALSE: the frame runs inside a caller's deferral
    Fault             \* "none", or a seeded fault that shows a rule can break

ASSUME /\ Wrapper \in BOOLEAN
       /\ DeliveryEnabled \in BOOLEAN
       /\ Fault \in {"none", "installation polls", "empty disposer suppresses"}

VARIABLES
    pc,           \* program point of the frame
    request,      \* cancellation request: "clear", "pending" or "delivered"
    content,      \* the value each place owns: "old", "new" or "vacant"
    releases,     \* protected releases of each lock guard, counted up to 2
    disposing,    \* the place whose value is under disposal, or "none"
    dstep,        \* progress of that disposal
    todo,         \* places the exit cleanup still has to clean
    exit,         \* "none", "normal", "propagation" or "delivered"
    deliveredAt,  \* the point that delivered the cancellation, or "none"
    disposed,     \* values whose disposal has completed
    duringOld     \* a request arrived between the start of old disposal and installation

vars == <<pc, request, content, releases, disposing, dstep, todo, exit,
          deliveredAt, disposed, duringOld>>

Guards == {"old", "new"}
Places == {"destination", "replacement"}

Init ==
    /\ pc = "entry check"
    /\ request = "clear"
    /\ content = [destination |-> "old", replacement |-> "new"]
    /\ releases = [g \in Guards |-> 0]
    /\ disposing = "none"
    /\ dstep = "idle"
    /\ todo = {}
    /\ exit = "none"
    /\ deliveredAt = "none"
    /\ disposed = {}
    /\ duringOld = FALSE

(* Delivery is deferred during disposal, installation and exit cleanup. *)
Deferred == pc \in {"written release", "old disposal", "installation", "exit cleanup"}

CanDeliver == DeliveryEnabled /\ request = "pending"

(* The places an exit from this point cleans. The stage is consumed by
   installation, so later exits clean the destination only. *)
CleanupList == IF pc \in {"entry check", "loop"} THEN Places ELSE {"destination"}

Release(g) == releases' = [releases EXCEPT ![g] = IF @ < 2 THEN @ + 1 ELSE 2]

----------------------------------------------------------------------------
(* The environment *)

RequestArrives ==
    /\ request = "clear"
    /\ pc # "exited"
    /\ request' = "pending"
    /\ duringOld' = (duringOld \/ pc \in {"old disposal", "installation"})
    /\ UNCHANGED <<pc, content, releases, disposing, dstep, todo, exit, deliveredAt, disposed>>

----------------------------------------------------------------------------
(* Disposal of the value in one place. A bare lock guard is released by its
   canonical provider. A wrapper runs its custom disposer first; the
   remaining-field cleanup then releases the lock guard unless the disposer
   already did. *)

BeginDisposal(p) ==
    /\ disposing' = p
    /\ dstep' = IF Wrapper THEN "custom disposer" ELSE "protected release"

Finish ==
    /\ content' = [content EXCEPT ![disposing] = "vacant"]
    /\ disposed' = disposed \cup {content[disposing]}
    /\ disposing' = "none"
    /\ dstep' = "idle"
    /\ todo' = todo \ {disposing}
    /\ pc' = CASE pc = "old disposal" -> "installation"
               [] pc = "written release" -> "loop"
               [] OTHER -> pc

DisposerReleasesField ==
    /\ dstep = "custom disposer"
    /\ Release(content[disposing])
    /\ dstep' = "field released"
    /\ UNCHANGED <<pc, request, content, disposing, todo, exit, deliveredAt, disposed, duringOld>>

DisposerReturnsAfterRelease ==
    /\ dstep = "field released"
    /\ Finish
    /\ UNCHANGED <<request, releases, exit, deliveredAt, duringOld>>

EmptyDisposerReturns ==
    /\ dstep = "custom disposer"
    /\ IF Fault = "empty disposer suppresses"
       THEN Finish /\ UNCHANGED releases
       ELSE /\ dstep' = "protected release"
            /\ UNCHANGED <<pc, content, releases, disposing, todo, disposed>>
    /\ UNCHANGED <<request, exit, deliveredAt, duringOld>>

ProviderReleases ==
    /\ dstep = "protected release"
    /\ Release(content[disposing])
    /\ Finish
    /\ UNCHANGED <<request, exit, deliveredAt, duringOld>>

----------------------------------------------------------------------------
(* The frame *)

Deliver(at) ==
    /\ CanDeliver
    /\ request' = "delivered"
    /\ exit' = "delivered"
    /\ deliveredAt' = at
    /\ pc' = "exit cleanup"
    /\ todo' = CleanupList

EntryCheckPasses ==
    /\ pc = "entry check"
    /\ pc' = "loop"
    /\ UNCHANGED <<request, content, releases, disposing, dstep, todo, exit, deliveredAt, disposed, duringOld>>

DeliveredAtEntry ==
    /\ pc = "entry check"
    /\ Deliver("entry check")
    /\ UNCHANGED <<content, releases, disposing, dstep, disposed, duringOld>>

DeliveredAtBackedge ==
    /\ pc = "loop"
    /\ Deliver("loop backedge")
    /\ UNCHANGED <<content, releases, disposing, dstep, disposed, duringOld>>

WrittenRelease ==
    /\ pc = "loop"
    /\ content.destination # "vacant"
    /\ pc' = "written release"
    /\ BeginDisposal("destination")
    /\ UNCHANGED <<request, content, releases, todo, exit, deliveredAt, disposed, duringOld>>

(* Phases 2 to 4: the replacement is complete and retained; delivery is
   deferred from here; the old value is disposed if the destination owns it. *)
BeginReplacement ==
    /\ pc = "loop"
    /\ IF content.destination # "vacant"
       THEN pc' = "old disposal" /\ BeginDisposal("destination")
       ELSE pc' = "installation" /\ UNCHANGED <<disposing, dstep>>
    /\ UNCHANGED <<request, content, releases, todo, exit, deliveredAt, disposed, duringOld>>

(* Phases 5 and 6. Installation is not a poll. Unmasking delivers a request
   that is already pending, against the installed value. *)
Install ==
    /\ pc = "installation"
    /\ content' = [destination |-> content.replacement, replacement |-> "vacant"]
    /\ IF CanDeliver
       THEN Deliver("unmask after installation")
       ELSE pc' = "post-install check" /\ UNCHANGED <<request, exit, deliveredAt, todo>>
    /\ UNCHANGED <<releases, disposing, dstep, disposed, duringOld>>

(* Seeded fault: a delivery before installation that uses the cleanup of the
   installed state. *)
DeliveredBeforeInstallation ==
    /\ Fault = "installation polls"
    /\ pc = "installation"
    /\ Deliver("before installation")
    /\ UNCHANGED <<content, releases, disposing, dstep, disposed, duringOld>>

PostInstallCheckPasses ==
    /\ pc = "post-install check"
    /\ pc' = "suffix"
    /\ UNCHANGED <<request, content, releases, disposing, dstep, todo, exit, deliveredAt, disposed, duringOld>>

DeliveredAtPostInstallCheck ==
    /\ pc = "post-install check"
    /\ Deliver("post-install check")
    /\ UNCHANGED <<content, releases, disposing, dstep, disposed, duringOld>>

ReturnNormally ==
    /\ pc = "suffix"
    /\ exit' = "normal"
    /\ pc' = "exit cleanup"
    /\ todo' = CleanupList
    /\ UNCHANGED <<request, content, releases, disposing, dstep, deliveredAt, disposed, duringOld>>

Propagate ==
    /\ pc \in {"loop", "suffix"}
    /\ exit' = "propagation"
    /\ pc' = "exit cleanup"
    /\ todo' = CleanupList
    /\ UNCHANGED <<request, content, releases, disposing, dstep, deliveredAt, disposed, duringOld>>

(* Exit cleanup: the destination first (declared last), then the replacement. *)
CleanupNext ==
    /\ pc = "exit cleanup"
    /\ disposing = "none"
    /\ \E p \in todo :
        /\ content[p] # "vacant"
        /\ (p = "replacement" => ("destination" \notin todo \/ content.destination = "vacant"))
        /\ BeginDisposal(p)
    /\ UNCHANGED <<pc, request, content, releases, todo, exit, deliveredAt, disposed, duringOld>>

CleanupDone ==
    /\ pc = "exit cleanup"
    /\ disposing = "none"
    /\ \A p \in todo : content[p] = "vacant"
    /\ pc' = "exited"
    /\ todo' = {}
    /\ UNCHANGED <<request, content, releases, disposing, dstep, exit, deliveredAt, disposed, duringOld>>

Done == pc = "exited" /\ UNCHANGED vars

Next ==
    \/ RequestArrives
    \/ DisposerReleasesField \/ DisposerReturnsAfterRelease
    \/ EmptyDisposerReturns \/ ProviderReleases
    \/ EntryCheckPasses \/ DeliveredAtEntry \/ DeliveredAtBackedge
    \/ WrittenRelease \/ BeginReplacement \/ Install
    \/ DeliveredBeforeInstallation
    \/ PostInstallCheckPasses \/ DeliveredAtPostInstallCheck
    \/ ReturnNormally \/ Propagate
    \/ CleanupNext \/ CleanupDone
    \/ Done

Spec == Init /\ [][Next]_vars

----------------------------------------------------------------------------
(* Rules *)

Exited(kind) == pc = "exited" /\ exit = kind
EachReleasedOnce == \A g \in Guards : releases[g] = 1

LockGuardReleasedAtMostOnce == \A g \in Guards : releases[g] <= 1

NormalExitReleasesEachLockGuardOnce == Exited("normal") => EachReleasedOnce

PropagationExitReleasesEachLockGuardOnce == Exited("propagation") => EachReleasedOnce

DeliveredCancellationExitReleasesEachLockGuardOnce == Exited("delivered") => EachReleasedOnce

(* K7, O14: until the replacement is installed, such a request is neither
   delivered nor lost. *)
CancellationDuringOldDisposalStaysPendingThroughInstallation ==
    (duringOld /\ content.replacement # "vacant") => (request = "pending" /\ exit = "none")

(* O14: the old value was disposed once before delivery; the delivered exit
   cleans the installed value once. *)
DeliveryAfterInstallationCleansInstalledValueOnce ==
    deliveredAt \in {"unmask after installation", "post-install check"} =>
        /\ releases.old = 1 /\ "old" \in disposed
        /\ releases.new <= 1
        /\ pc = "exited" => (releases.new = 1 /\ "new" \in disposed)

(* K3: a completed disposal has released the lock guard, wrapper or not. *)
NoReleaseSuppressedByWrapper == \A g \in disposed : releases[g] = 1

=============================================================================
