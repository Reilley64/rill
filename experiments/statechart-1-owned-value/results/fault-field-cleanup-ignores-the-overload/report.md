# OwnedValue

TLC reached 334 states and 532 steps. The chart draws 13 states and 15 arrows.
The chart does not draw 63 steps that stay in one chart state.
The view hides 187 steps, so the chart does not draw them.

## MovedOrDisposedFieldNeverDisposed breaks
The shortest path has 5 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the routine calls dispose on the value: Owned value Disposing: dispose overload running, Routine Running
3. the dispose overload disposes a field: Owned value Disposing: dispose overload running, Routine Running
4. the dispose overload returns, fields remain: Owned value Remaining-field cleanup, Routine Running
5. a remaining field is cleaned up: Owned value Remaining-field cleanup, Routine Running

## NormalExitDisposesEachOwnedValueOnce breaks
The shortest path has 8 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the routine calls dispose on the value: Owned value Disposing: dispose overload running, Routine Running
3. the dispose overload disposes a field: Owned value Disposing: dispose overload running, Routine Running
4. the dispose overload returns, fields remain: Owned value Remaining-field cleanup, Routine Running
5. a remaining field is cleaned up: Owned value Remaining-field cleanup, Routine Running
6. the last remaining field is cleaned up: Owned value Disposed, Routine Running
7. the routine returns: Owned value Disposed, Routine Normal exit: cleaning up
8. cleanup is complete, the routine returns to its caller: Owned value Disposed, Routine Left by normal exit

## PropagationExitDisposesEachOwnedValueOnce breaks
The shortest path has 9 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the routine calls dispose on the value: Owned value Disposing: dispose overload running, Routine Running
3. the dispose overload disposes a field: Owned value Disposing: dispose overload running, Routine Running
4. the dispose overload returns, fields remain: Owned value Remaining-field cleanup, Routine Running
5. a remaining field is cleaned up: Owned value Remaining-field cleanup, Routine Running
6. the last remaining field is cleaned up: Owned value Disposed, Routine Running
7. the routine propagates a failure: Owned value Disposed, Routine Propagation exit: failure not yet in the caller's result
8. the failure moves into the caller's result: Owned value Disposed, Routine Propagation exit: cleaning up
9. cleanup is complete, the caller receives the failure: Owned value Disposed, Routine Left by propagation exit

## Rules that hold
ValueDisposedAtMostOnce, ProgramExitDisposesEachStartupValueOnce, MovedValueNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, DependentCleanedUpBeforeSource, CleanupInReverseDeclarationOrderOtherwise, PanicDisposesNothing, FailureExportedBeforeCleanup, OldValueGoneBeforeInstallation, NoDeadlock
