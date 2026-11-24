# LockGuardRelease

TLC reached 79 states and 114 steps. The chart draws 17 states and 24 arrows.
The chart does not draw 36 steps that stay in one chart state.
The view hides 11 steps, so the chart does not draw them.
The step the check yields and delivers the cancellation changes Frame and Cancellation request at the same time.
The step the loop backedge yields and delivers the cancellation changes Frame and Cancellation request at the same time.
The step the replacement is installed, unmasking delivers the cancellation changes Frame and Cancellation request at the same time.
The step FAULT: the cancellation is delivered before installation changes Frame and Cancellation request at the same time.

## CancellationDuringOldDisposalStaysPendingThroughInstallation breaks
The shortest path has 5 steps:
0. Frame Entry budget check, Cancellation request Clear
1. the check passes without delivery: Frame Loop, owns both lock guards, Cancellation request Clear
2. the replacement is complete, delivery is deferred: Frame Old value under disposal, Cancellation request Clear
3. the lock guard is released: Frame Old value gone, replacement not yet installed, Cancellation request Clear
4. a cancellation request arrives: Frame Old value gone, replacement not yet installed, Cancellation request Pending
5. FAULT: the cancellation is delivered before installation: Frame Cleanup on delivered cancellation exit, Cancellation request Delivered

## DeliveredCancellationExitReleasesEachLockGuardOnce breaks
The shortest path has 6 steps:
0. Frame Entry budget check, Cancellation request Clear
1. the check passes without delivery: Frame Loop, owns both lock guards, Cancellation request Clear
2. a cancellation request arrives: Frame Loop, owns both lock guards, Cancellation request Pending
3. the replacement is complete, delivery is deferred: Frame Old value under disposal, Cancellation request Pending
4. the lock guard is released: Frame Old value gone, replacement not yet installed, Cancellation request Pending
5. FAULT: the cancellation is delivered before installation: Frame Cleanup on delivered cancellation exit, Cancellation request Delivered
6. every owned lock guard is released: Frame Left by delivered cancellation exit, Cancellation request Delivered

## Rules that hold
LockGuardReleasedAtMostOnce, NormalExitReleasesEachLockGuardOnce, PropagationExitReleasesEachLockGuardOnce, DeliveryAfterInstallationCleansInstalledValueOnce, NoReleaseSuppressedByWrapper, NoDeadlock
