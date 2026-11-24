# OwnedValue

TLC reached 240 states and 384 steps. The chart draws 10 states and 19 arrows.
The chart does not draw 48 steps that stay in one chart state.
The view hides 136 steps, so the chart does not draw them.

## Rules that hold
ValueDisposedAtMostOnce, NormalExitDisposesEachOwnedValueOnce, PropagationExitDisposesEachOwnedValueOnce, ProgramExitDisposesEachStartupValueOnce, MovedValueNeverDisposed, MovedOrDisposedFieldNeverDisposed, DisposeOverloadRunsBeforeFieldCleanup, DependentCleanedUpBeforeSource, CleanupInReverseDeclarationOrderOtherwise, PanicDisposesNothing, FailureExportedBeforeCleanup, OldValueGoneBeforeInstallation, NoDeadlock
