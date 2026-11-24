# LockGuardRelease

TLC reached 121 states and 191 steps. The chart draws 4 states and 5 arrows.
The chart does not draw 2 steps that stay in one chart state.
The view hides 75 steps, so the chart does not draw them.

## Rules that hold
LockGuardReleasedAtMostOnce, NormalExitReleasesEachLockGuardOnce, PropagationExitReleasesEachLockGuardOnce, DeliveredCancellationExitReleasesEachLockGuardOnce, CancellationDuringOldDisposalStaysPendingThroughInstallation, DeliveryAfterInstallationCleansInstalledValueOnce, NoReleaseSuppressedByWrapper, NoDeadlock
