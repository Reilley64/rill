# OwnedValue

TLC reached 54 states and 85 steps. The chart draws 12 states and 14 arrows.
The view hides 31 steps, so the chart does not draw them.

## FailureExportedBeforeCleanup breaks
The shortest path has 3 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the routine propagates a failure: Owned value Initialized, Routine Propagation exit: failure not yet in the caller's result
3. exit cleanup begins on the value: Owned value Remaining-field cleanup, Routine Propagation exit: failure not yet in the caller's result

## Rules that hold
ValueDisposedAtMostOnce, NormalExitDisposesEachOwnedValueOnce, PropagationExitDisposesEachOwnedValueOnce, ProgramExitDisposesEachStartupValueOnce, MovedValueNeverDisposed, MovedOrDisposedFieldNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, DependentCleanedUpBeforeSource, CleanupInReverseDeclarationOrderOtherwise, PanicDisposesNothing, OldValueGoneBeforeInstallation, NoDeadlock
