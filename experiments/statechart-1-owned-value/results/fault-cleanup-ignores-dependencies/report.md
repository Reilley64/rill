# OwnedValue

TLC reached 158 states and 247 steps. The chart draws 10 states and 19 arrows.
The chart does not draw 23 steps that stay in one chart state.
The view hides 90 steps, so the chart does not draw them.

## DependentCleanedUpBeforeSource breaks
The shortest path has 4 steps:
0. Running
1. a value is initialized: Running
2. a value is initialized: Running
3. the routine returns or propagates a failure while it owns both values: Exit cleanup, the routine owns both values
4. cleanup begins on the second value: Second value under disposal, first still owned

## Rules that hold
ValueDisposedAtMostOnce, NormalExitDisposesEachOwnedValueOnce, PropagationExitDisposesEachOwnedValueOnce, ProgramExitDisposesEachStartupValueOnce, MovedValueNeverDisposed, MovedOrDisposedFieldNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, CleanupInReverseDeclarationOrderOtherwise, PanicDisposesNothing, FailureExportedBeforeCleanup, OldValueGoneBeforeInstallation, NoDeadlock
