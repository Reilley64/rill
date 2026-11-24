# LockGuardRelease

TLC reached 212 states and 320 steps. The chart draws 17 states and 24 arrows.
The chart does not draw 133 steps that stay in one chart state.
The view hides 36 steps, so the chart does not draw them.
The step the check yields and delivers the cancellation changes Frame and Cancellation request at the same time.
The step the loop backedge yields and delivers the cancellation changes Frame and Cancellation request at the same time.
The step the replacement is installed, unmasking delivers the cancellation changes Frame and Cancellation request at the same time.

## NoReleaseSuppressedByWrapper breaks
The shortest path has 3 steps:
0. Frame Entry budget check, Cancellation request Clear
1. the check passes without delivery: Frame Loop, owns both lock guards, Cancellation request Clear
2. the body disposes the old value: Frame Written dispose releases the old lock guard, Cancellation request Clear
3. the custom disposer returns: Frame Loop, old lock guard already released, Cancellation request Clear

## DeliveryAfterInstallationCleansInstalledValueOnce breaks
The shortest path has 5 steps:
0. Frame Entry budget check, Cancellation request Clear
1. the check passes without delivery: Frame Loop, owns both lock guards, Cancellation request Clear
2. a cancellation request arrives: Frame Loop, owns both lock guards, Cancellation request Pending
3. the replacement is complete, delivery is deferred: Frame Old value under disposal, Cancellation request Pending
4. the custom disposer returns: Frame Old value gone, replacement not yet installed, Cancellation request Pending
5. the replacement is installed, unmasking delivers the cancellation: Frame Cleanup on delivered cancellation exit, Cancellation request Delivered

## PropagationExitReleasesEachLockGuardOnce breaks
The shortest path has 7 steps:
0. Frame Entry budget check, Cancellation request Clear
1. the check passes without delivery: Frame Loop, owns both lock guards, Cancellation request Clear
2. the frame propagates a failure: Frame Cleanup on propagation exit, Cancellation request Clear
3. cleanup starts on the next owned value: Frame Cleanup on propagation exit, Cancellation request Clear
4. the custom disposer returns: Frame Cleanup on propagation exit, Cancellation request Clear
5. cleanup starts on the next owned value: Frame Cleanup on propagation exit, Cancellation request Clear
6. the custom disposer returns: Frame Cleanup on propagation exit, Cancellation request Clear
7. every owned lock guard is released: Frame Left by propagation exit, Cancellation request Clear

## DeliveredCancellationExitReleasesEachLockGuardOnce breaks
The shortest path has 7 steps:
0. Frame Entry budget check, Cancellation request Clear
1. a cancellation request arrives: Frame Entry budget check, Cancellation request Pending
2. the check yields and delivers the cancellation: Frame Cleanup on delivered cancellation exit, Cancellation request Delivered
3. cleanup starts on the next owned value: Frame Cleanup on delivered cancellation exit, Cancellation request Delivered
4. the custom disposer returns: Frame Cleanup on delivered cancellation exit, Cancellation request Delivered
5. cleanup starts on the next owned value: Frame Cleanup on delivered cancellation exit, Cancellation request Delivered
6. the custom disposer returns: Frame Cleanup on delivered cancellation exit, Cancellation request Delivered
7. every owned lock guard is released: Frame Left by delivered cancellation exit, Cancellation request Delivered

## NormalExitReleasesEachLockGuardOnce breaks
The shortest path has 9 steps:
0. Frame Entry budget check, Cancellation request Clear
1. the check passes without delivery: Frame Loop, owns both lock guards, Cancellation request Clear
2. the replacement is complete, delivery is deferred: Frame Old value under disposal, Cancellation request Clear
3. the custom disposer returns: Frame Old value gone, replacement not yet installed, Cancellation request Clear
4. the replacement is installed: Frame Replacement installed, next budget check, Cancellation request Clear
5. the check passes without delivery: Frame Ordinary suffix, Cancellation request Clear
6. the frame returns: Frame Cleanup on normal exit, Cancellation request Clear
7. cleanup starts on the next owned value: Frame Cleanup on normal exit, Cancellation request Clear
8. the custom disposer returns: Frame Cleanup on normal exit, Cancellation request Clear
9. every owned lock guard is released: Frame Left by normal exit, Cancellation request Clear

## Rules that hold
LockGuardReleasedAtMostOnce, CancellationDuringOldDisposalStaysPendingThroughInstallation, NoDeadlock
