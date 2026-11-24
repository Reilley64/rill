# OwnedValue

TLC reached 59 states and 96 steps. The chart draws 12 states and 14 arrows.
The view hides 38 steps, so the chart does not draw them.

## PanicDisposesNothing breaks
The shortest path has 3 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. a panic begins: Owned value Initialized, Routine Program ended by panic
3. exit cleanup begins on the value: Owned value Remaining-field cleanup, Routine Program ended by panic

## Rules that hold
ValueDisposedAtMostOnce, NormalExitDisposesEachOwnedValueOnce, PropagationExitDisposesEachOwnedValueOnce, ProgramExitDisposesEachStartupValueOnce, MovedValueNeverDisposed, MovedOrDisposedFieldNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, DependentCleanedUpBeforeSource, CleanupInReverseDeclarationOrderOtherwise, FailureExportedBeforeCleanup, OldValueGoneBeforeInstallation, NoDeadlock
