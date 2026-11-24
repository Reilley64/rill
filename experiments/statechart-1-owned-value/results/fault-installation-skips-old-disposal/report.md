# OwnedValue

TLC reached 178 states and 280 steps. The chart draws 17 states and 27 arrows.
The view hides 102 steps, so the chart does not draw them.

## OldValueGoneBeforeInstallation breaks
The shortest path has 3 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the replacement value is complete: Owned value Replacement complete, old value still in place, Routine Running
3. FAULT: the replacement is installed over the old value: Owned value Replacement installed over a live old value, Routine Running

## NormalExitDisposesEachOwnedValueOnce breaks
The shortest path has 6 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the replacement value is complete: Owned value Replacement complete, old value still in place, Routine Running
3. FAULT: the replacement is installed over the old value: Owned value Replacement installed over a live old value, Routine Running
4. the whole value is moved out: Owned value Moved out, Routine Running
5. the routine returns: Owned value Moved out, Routine Normal exit: cleaning up
6. cleanup is complete, the routine returns to its caller: Owned value Moved out, Routine Left by normal exit

## PropagationExitDisposesEachOwnedValueOnce breaks
The shortest path has 7 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the replacement value is complete: Owned value Replacement complete, old value still in place, Routine Running
3. FAULT: the replacement is installed over the old value: Owned value Replacement installed over a live old value, Routine Running
4. the whole value is moved out: Owned value Moved out, Routine Running
5. the routine propagates a failure: Owned value Moved out, Routine Propagation exit: failure not yet in the caller's result
6. the failure moves into the caller's result: Owned value Moved out, Routine Propagation exit: cleaning up
7. cleanup is complete, the caller receives the failure: Owned value Moved out, Routine Left by propagation exit

## Rules that hold
ValueDisposedAtMostOnce, ProgramExitDisposesEachStartupValueOnce, MovedValueNeverDisposed, MovedOrDisposedFieldNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, DependentCleanedUpBeforeSource, CleanupInReverseDeclarationOrderOtherwise, PanicDisposesNothing, FailureExportedBeforeCleanup, NoDeadlock
