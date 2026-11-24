# LockGuardRelease

TLC reached 75 states and 108 steps. The chart draws 17 states and 24 arrows.
The chart does not draw 36 steps that stay in one chart state.
The view hides 9 steps, so the chart does not draw them.
The step the check yields and delivers the cancellation changes Frame and Cancellation request at the same time.
The step the loop backedge yields and delivers the cancellation changes Frame and Cancellation request at the same time.
The step the replacement is installed, unmasking delivers the cancellation changes Frame and Cancellation request at the same time.

## Rules that hold
LockGuardReleasedAtMostOnce, NormalExitReleasesEachLockGuardOnce, PropagationExitReleasesEachLockGuardOnce, DeliveredCancellationExitReleasesEachLockGuardOnce, CancellationDuringOldDisposalStaysPendingThroughInstallation, DeliveryAfterInstallationCleansInstalledValueOnce, NoReleaseSuppressedByWrapper, NoDeadlock
