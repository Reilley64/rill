# LockGuardRelease

TLC reached 92 states and 153 steps. The chart draws 14 states and 15 arrows.
The chart does not draw 70 steps that stay in one chart state.
The view hides 6 steps, so the chart does not draw them.

## Rules that hold
LockGuardReleasedAtMostOnce, NormalExitReleasesEachLockGuardOnce, PropagationExitReleasesEachLockGuardOnce, DeliveredCancellationExitReleasesEachLockGuardOnce, CancellationDuringOldDisposalStaysPendingThroughInstallation, DeliveryAfterInstallationCleansInstalledValueOnce, NoReleaseSuppressedByWrapper, NoDeadlock
