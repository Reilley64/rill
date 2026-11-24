# OwnedValue

TLC reached 22 states and 34 steps. The chart draws 9 states and 8 arrows.
The view hides 13 steps, so the chart does not draw them.

## ProgramExitDisposesEachStartupValueOnce breaks
The shortest path has 3 steps:
0. Owned value Uninitialized, Routine Running
1. the value is initialized: Owned value Initialized, Routine Running
2. startup completes: Owned value Initialized, Routine Startup complete: cleaning up the startup scope
3. cleanup is complete, the program exits: Owned value Initialized, Routine Program exited

## Rules that hold
ValueDisposedAtMostOnce, NormalExitDisposesEachOwnedValueOnce, PropagationExitDisposesEachOwnedValueOnce, MovedValueNeverDisposed, MovedOrDisposedFieldNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, DependentCleanedUpBeforeSource, CleanupInReverseDeclarationOrderOtherwise, PanicDisposesNothing, FailureExportedBeforeCleanup, OldValueGoneBeforeInstallation, NoDeadlock
