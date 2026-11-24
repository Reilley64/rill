# OwnedValue

TLC reached 68 states and 108 steps. The chart draws 12 states and 14 arrows.
The chart does not draw 6 steps that stay in one chart state.
The view hides 41 steps, so the chart does not draw them.

## MovedValueNeverDisposed breaks
The shortest path has 4 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the whole value is moved out: Owned value Moved out, Routine Running
3. the routine returns: Owned value Moved out, Routine Normal exit: cleaning up
4. FAULT: exit cleanup disposes the ended value again: Owned value Moved out, Routine Normal exit: cleaning up

## ValueDisposedAtMostOnce breaks
The shortest path has 5 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the routine calls dispose on the value: Owned value Remaining-field cleanup, Routine Running
3. the last remaining field is cleaned up: Owned value Disposed, Routine Running
4. the routine returns: Owned value Disposed, Routine Normal exit: cleaning up
5. FAULT: exit cleanup disposes the ended value again: Owned value Disposed, Routine Normal exit: cleaning up

## NormalExitDisposesEachOwnedValueOnce breaks
The shortest path has 5 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the whole value is moved out: Owned value Moved out, Routine Running
3. the routine returns: Owned value Moved out, Routine Normal exit: cleaning up
4. FAULT: exit cleanup disposes the ended value again: Owned value Moved out, Routine Normal exit: cleaning up
5. cleanup is complete, the routine returns to its caller: Owned value Moved out, Routine Left by normal exit

## PropagationExitDisposesEachOwnedValueOnce breaks
The shortest path has 6 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the whole value is moved out: Owned value Moved out, Routine Running
3. the routine propagates a failure: Owned value Moved out, Routine Propagation exit: failure not yet in the caller's result
4. the failure moves into the caller's result: Owned value Moved out, Routine Propagation exit: cleaning up
5. FAULT: exit cleanup disposes the ended value again: Owned value Moved out, Routine Propagation exit: cleaning up
6. cleanup is complete, the caller receives the failure: Owned value Moved out, Routine Left by propagation exit

## Rules that hold
ProgramExitDisposesEachStartupValueOnce, MovedOrDisposedFieldNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, DependentCleanedUpBeforeSource, CleanupInReverseDeclarationOrderOtherwise, PanicDisposesNothing, FailureExportedBeforeCleanup, OldValueGoneBeforeInstallation, NoDeadlock
