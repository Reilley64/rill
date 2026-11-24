# OwnedValue

TLC reached 56 states and 87 steps. The chart draws 13 states and 15 arrows.
The view hides 32 steps, so the chart does not draw them.

## DisposeOverloadRunsBeforeFieldCleanup breaks
The shortest path has 2 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. the routine calls dispose on the value: Owned value Remaining-field cleanup, Routine Running

## Rules that hold
ValueDisposedAtMostOnce, NormalExitDisposesEachOwnedValueOnce, PropagationExitDisposesEachOwnedValueOnce, ProgramExitDisposesEachStartupValueOnce, MovedValueNeverDisposed, MovedOrDisposedFieldNeverDisposed, DependentCleanedUpBeforeSource, CleanupInReverseDeclarationOrderOtherwise, PanicDisposesNothing, FailureExportedBeforeCleanup, OldValueGoneBeforeInstallation, NoDeadlock
