# OwnedValue

TLC reached 240 states and 384 steps. The chart draws 10 states and 20 arrows.
The chart does not draw 46 steps that stay in one chart state.
The view hides 136 steps, so the chart does not draw them.

## CleanupInReverseDeclarationOrderOtherwise breaks
The shortest path has 6 steps:
0. Running
1. a value is initialized: Running
2. a value is initialized: Running
3. the routine returns or propagates a failure while it owns both values: Exit cleanup, the routine owns both values
4. cleanup begins on the first value: First value under disposal, second still owned
5. the value is disposed: Exit cleanup, the routine owns only the second value
6. cleanup begins on the second value: Second value under disposal, first not owned

## Rules that hold
ValueDisposedAtMostOnce, NormalExitDisposesEachOwnedValueOnce, PropagationExitDisposesEachOwnedValueOnce, ProgramExitDisposesEachStartupValueOnce, MovedValueNeverDisposed, MovedOrDisposedFieldNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, DependentCleanedUpBeforeSource, PanicDisposesNothing, FailureExportedBeforeCleanup, OldValueGoneBeforeInstallation, NoDeadlock
