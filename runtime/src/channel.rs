use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};

/// Result of one non-waiting send attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum SendAttempt<T> {
    Sent,
    Full(T),
    Closed(T),
}

/// Result of one non-waiting receive attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum ReceiveAttempt<T> {
    Item(T),
    Empty,
    Closed,
}

/// Result of one non-waiting coalesced-wake receive attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum WakeAttempt {
    Item,
    Empty,
    Closed,
}

/// Creates a bounded channel. Capacity zero is rendezvous mode.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(ChannelInner {
        state: Mutex::new(ChannelState {
            capacity,
            buffer: VecDeque::with_capacity(capacity),
            rendezvous: VecDeque::new(),
            pending_senders: VecDeque::new(),
            waiting_receivers: 0,
            sender_count: 1,
            receiver_alive: true,
            send_watches: Vec::new(),
            receive_watches: Vec::new(),
        }),
        changed: Condvar::new(),
    });
    (
        Sender {
            inner: Arc::clone(&inner),
        },
        Receiver { inner },
    )
}

pub struct Sender<T> {
    inner: Arc<ChannelInner<T>>,
}

impl<T> Sender<T> {
    /// Attempts one transfer without waiting for capacity or a receiver.
    pub fn try_send(&self, value: T) -> SendAttempt<T> {
        let mut state = lock(&self.inner.state);
        if !state.receiver_alive {
            return SendAttempt::Closed(value);
        }
        if state.capacity == 0 {
            if state.waiting_receivers > state.rendezvous.len() {
                state.rendezvous.push_back(value);
                fire_receive_watches(&mut state);
                self.inner.changed.notify_all();
                SendAttempt::Sent
            } else {
                SendAttempt::Full(value)
            }
        } else if state.buffer.len() < state.capacity {
            state.buffer.push_back(value);
            fire_receive_watches(&mut state);
            self.inner.changed.notify_all();
            SendAttempt::Sent
        } else {
            SendAttempt::Full(value)
        }
    }

    /// Waits until the value commits or receiving ownership disappears.
    pub fn send(&self, value: T) -> Result<(), T> {
        let mut state = lock(&self.inner.state);
        if state.capacity > 0 {
            let mut value = Some(value);
            loop {
                if !state.receiver_alive {
                    return Err(value.take().expect("send value remains owned"));
                }
                if state.buffer.len() < state.capacity {
                    state
                        .buffer
                        .push_back(value.take().expect("send value commits once"));
                    fire_receive_watches(&mut state);
                    self.inner.changed.notify_all();
                    return Ok(());
                }
                state = wait(&self.inner.changed, state);
            }
        }

        if !state.receiver_alive {
            return Err(value);
        }
        if state.waiting_receivers > state.rendezvous.len() {
            state.rendezvous.push_back(value);
            fire_receive_watches(&mut state);
            self.inner.changed.notify_all();
            return Ok(());
        }

        let pending = Arc::new(PendingSend::new(value));
        state.pending_senders.push_back(Arc::clone(&pending));
        fire_receive_watches(&mut state);
        self.inner.changed.notify_all();
        drop(state);
        pending.wait()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        let mut state = lock(&self.inner.state);
        state.sender_count = state
            .sender_count
            .checked_add(1)
            .expect("channel sender count overflow");
        drop(state);
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state = lock(&self.inner.state);
        state.sender_count -= 1;
        if state.sender_count == 0 {
            retire_watches(&mut state.send_watches);
            fire_receive_watches(&mut state);
            self.inner.changed.notify_all();
        }
    }
}

pub struct Receiver<T> {
    inner: Arc<ChannelInner<T>>,
}

impl<T> Receiver<T> {
    /// Attempts one transfer without waiting for a sender or buffered value.
    pub fn try_receive(&self) -> ReceiveAttempt<T> {
        let mut state = lock(&self.inner.state);
        if !state.receiver_alive {
            return ReceiveAttempt::Closed;
        }
        if state.capacity == 0 {
            if let Some(value) = state.rendezvous.pop_front() {
                fire_send_watches(&mut state);
                self.inner.changed.notify_all();
                return ReceiveAttempt::Item(value);
            }
            if let Some(pending) = state.pending_senders.pop_front() {
                let value = pending.commit();
                fire_send_watches(&mut state);
                self.inner.changed.notify_all();
                return ReceiveAttempt::Item(value);
            }
        } else if let Some(value) = state.buffer.pop_front() {
            fire_send_watches(&mut state);
            self.inner.changed.notify_all();
            return ReceiveAttempt::Item(value);
        }

        if state.sender_count == 0 {
            ReceiveAttempt::Closed
        } else {
            ReceiveAttempt::Empty
        }
    }

    /// Waits for one value or closed-and-drained state.
    pub fn receive(&self) -> Option<T> {
        let mut state = lock(&self.inner.state);
        if state.capacity > 0 {
            loop {
                if let Some(value) = state.buffer.pop_front() {
                    fire_send_watches(&mut state);
                    self.inner.changed.notify_all();
                    return Some(value);
                }
                if state.sender_count == 0 || !state.receiver_alive {
                    return None;
                }
                state = wait(&self.inner.changed, state);
            }
        }

        let mut registered = false;
        loop {
            if let Some(value) = state.rendezvous.pop_front() {
                if registered {
                    state.waiting_receivers -= 1;
                }
                fire_send_watches(&mut state);
                self.inner.changed.notify_all();
                return Some(value);
            }
            if let Some(pending) = state.pending_senders.pop_front() {
                if registered {
                    state.waiting_receivers -= 1;
                }
                let value = pending.commit();
                fire_send_watches(&mut state);
                self.inner.changed.notify_all();
                return Some(value);
            }
            if state.sender_count == 0 || !state.receiver_alive {
                if registered {
                    state.waiting_receivers -= 1;
                    fire_send_watches(&mut state);
                }
                return None;
            }
            if !registered {
                state.waiting_receivers += 1;
                registered = true;
                fire_send_watches(&mut state);
            }
            state = wait(&self.inner.changed, state);
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = lock(&self.inner.state);
        if !state.receiver_alive {
            return;
        }
        state.receiver_alive = false;
        state.buffer.clear();
        state.rendezvous.clear();
        for pending in state.pending_senders.drain(..) {
            pending.close();
        }
        retire_watches(&mut state.receive_watches);
        fire_send_watches(&mut state);
        self.inner.changed.notify_all();
    }
}

struct ChannelInner<T> {
    state: Mutex<ChannelState<T>>,
    changed: Condvar,
}

struct ChannelState<T> {
    capacity: usize,
    buffer: VecDeque<T>,
    rendezvous: VecDeque<T>,
    pending_senders: VecDeque<Arc<PendingSend<T>>>,
    waiting_receivers: usize,
    sender_count: usize,
    receiver_alive: bool,
    send_watches: Vec<Weak<WatchCore>>,
    receive_watches: Vec<Weak<WatchCore>>,
}

struct PendingSend<T> {
    state: Mutex<PendingSendState<T>>,
    changed: Condvar,
}

enum PendingSendState<T> {
    Waiting(Option<T>),
    Sent,
    Closed(Option<T>),
}

impl<T> PendingSend<T> {
    fn new(value: T) -> Self {
        Self {
            state: Mutex::new(PendingSendState::Waiting(Some(value))),
            changed: Condvar::new(),
        }
    }

    fn commit(&self) -> T {
        let mut state = lock(&self.state);
        let PendingSendState::Waiting(value) = &mut *state else {
            unreachable!("queued rendezvous sender is waiting")
        };
        let value = value.take().expect("pending send commits once");
        *state = PendingSendState::Sent;
        self.changed.notify_all();
        value
    }

    fn close(&self) {
        let mut state = lock(&self.state);
        if let PendingSendState::Waiting(value) = &mut *state {
            let value = value.take();
            *state = PendingSendState::Closed(value);
            self.changed.notify_all();
        }
    }

    fn wait(&self) -> Result<(), T> {
        let mut state = lock(&self.state);
        loop {
            match &mut *state {
                PendingSendState::Waiting(_) => state = wait(&self.changed, state),
                PendingSendState::Sent => return Ok(()),
                PendingSendState::Closed(value) => {
                    return Err(value.take().expect("closed send returns input once"));
                }
            }
        }
    }
}

/// Creates a one-token coalescing wake channel.
pub fn wake_channel() -> (WakeNotifier, WakeReceiver) {
    let inner = Arc::new(WakeInner {
        state: Mutex::new(WakeState {
            pending: false,
            receiver_alive: true,
            notifier_count: 1,
            watches: Vec::new(),
        }),
        changed: Condvar::new(),
    });
    (
        WakeNotifier {
            inner: Arc::clone(&inner),
        },
        WakeReceiver { inner },
    )
}

pub struct WakeNotifier {
    inner: Arc<WakeInner>,
}

impl WakeNotifier {
    /// Posts or coalesces one wake. Returns false after receiver loss.
    pub fn wake(&self) -> bool {
        let mut state = lock(&self.inner.state);
        if !state.receiver_alive {
            return false;
        }
        state.pending = true;
        self.inner.changed.notify_all();
        true
    }
}

impl Clone for WakeNotifier {
    fn clone(&self) -> Self {
        let mut state = lock(&self.inner.state);
        state.notifier_count = state
            .notifier_count
            .checked_add(1)
            .expect("wake notifier count overflow");
        drop(state);
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Drop for WakeNotifier {
    fn drop(&mut self) {
        let mut state = lock(&self.inner.state);
        state.notifier_count -= 1;
        if state.notifier_count == 0 {
            self.inner.changed.notify_all();
        }
    }
}

pub struct WakeReceiver {
    inner: Arc<WakeInner>,
}

impl WakeReceiver {
    pub fn try_receive(&self) -> WakeAttempt {
        let mut state = lock(&self.inner.state);
        if state.pending {
            state.pending = false;
            WakeAttempt::Item
        } else if state.notifier_count == 0 {
            WakeAttempt::Closed
        } else {
            WakeAttempt::Empty
        }
    }

    pub fn receive(&self) -> Option<()> {
        let mut state = lock(&self.inner.state);
        loop {
            if state.pending {
                state.pending = false;
                return Some(());
            }
            if state.notifier_count == 0 {
                return None;
            }
            state = wait(&self.inner.changed, state);
        }
    }
}

impl Drop for WakeReceiver {
    fn drop(&mut self) {
        let mut state = lock(&self.inner.state);
        state.receiver_alive = false;
        state.pending = false;
        retire_watches(&mut state.watches);
        self.inner.changed.notify_all();
    }
}

struct WakeInner {
    state: Mutex<WakeState>,
    changed: Condvar,
}

struct WakeState {
    pending: bool,
    receiver_alive: bool,
    notifier_count: usize,
    watches: Vec<Weak<WatchCore>>,
}

#[derive(Clone, Copy)]
enum WatchDirection {
    Send,
    Receive,
}

struct WatchCore {
    armed: AtomicBool,
    notifier: WakeNotifier,
}

impl WatchCore {
    fn fire(&self) {
        if self.armed.swap(false, Ordering::SeqCst) {
            let _ = self.notifier.wake();
        }
    }

    fn retire(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }
}

/// One-shot readiness watch that retains channel state but no endpoint count.
pub struct ChannelWatch<T> {
    inner: Arc<ChannelInner<T>>,
    core: Arc<WatchCore>,
    direction: WatchDirection,
}

/// Watches send capacity or receiver closure without retaining sender count.
pub fn watch_send_ready<T>(source: &Sender<T>, notifier: WakeNotifier) -> ChannelWatch<T> {
    ChannelWatch::new(&source.inner, WatchDirection::Send, notifier)
}

/// Watches receive data or closed-and-drained state without retaining receiver ownership.
pub fn watch_receive_ready<T>(source: &Receiver<T>, notifier: WakeNotifier) -> ChannelWatch<T> {
    ChannelWatch::new(&source.inner, WatchDirection::Receive, notifier)
}

impl<T> ChannelWatch<T> {
    fn new(
        inner: &Arc<ChannelInner<T>>,
        direction: WatchDirection,
        notifier: WakeNotifier,
    ) -> Self {
        let core = Arc::new(WatchCore {
            armed: AtomicBool::new(false),
            notifier,
        });
        {
            let mut wake_state = lock(&core.notifier.inner.state);
            wake_state.watches.push(Arc::downgrade(&core));
        }
        let mut state = lock(&inner.state);
        match direction {
            WatchDirection::Send => state.send_watches.push(Arc::downgrade(&core)),
            WatchDirection::Receive => state.receive_watches.push(Arc::downgrade(&core)),
        }
        drop(state);
        Self {
            inner: Arc::clone(inner),
            core,
            direction,
        }
    }

    /// Arms once and synchronously rechecks current eligibility.
    pub fn arm(&mut self) {
        let state = lock(&self.inner.state);
        let source_alive = match self.direction {
            WatchDirection::Send => state.sender_count > 0,
            WatchDirection::Receive => state.receiver_alive,
        };
        if !source_alive {
            self.core.retire();
            return;
        }
        self.core.armed.store(true, Ordering::SeqCst);
        let eligible = match self.direction {
            WatchDirection::Send => send_ready(&state),
            WatchDirection::Receive => receive_ready(&state),
        };
        if eligible {
            self.core.fire();
        }
    }

    /// Withdraws and waits for any in-progress synchronous notification path.
    pub fn disarm(&mut self) {
        let _state = lock(&self.inner.state);
        self.core.retire();
    }
}

impl<T> Drop for ChannelWatch<T> {
    fn drop(&mut self) {
        let weak = Arc::downgrade(&self.core);
        {
            let mut state = lock(&self.inner.state);
            self.core.retire();
            let watches = match self.direction {
                WatchDirection::Send => &mut state.send_watches,
                WatchDirection::Receive => &mut state.receive_watches,
            };
            remove_watch(watches, &weak);
        }
        let mut wake_state = lock(&self.core.notifier.inner.state);
        remove_watch(&mut wake_state.watches, &weak);
    }
}

fn send_ready<T>(state: &ChannelState<T>) -> bool {
    if !state.receiver_alive {
        return true;
    }
    if state.capacity == 0 {
        state.waiting_receivers > state.rendezvous.len()
    } else {
        state.buffer.len() < state.capacity
    }
}

fn receive_ready<T>(state: &ChannelState<T>) -> bool {
    if state.capacity == 0 {
        !state.rendezvous.is_empty() || !state.pending_senders.is_empty() || state.sender_count == 0
    } else {
        !state.buffer.is_empty() || state.sender_count == 0
    }
}

fn fire_send_watches<T>(state: &mut ChannelState<T>) {
    if state.sender_count == 0 {
        retire_watches(&mut state.send_watches);
    } else if send_ready(state) {
        fire_watches(&mut state.send_watches);
    }
}

fn fire_receive_watches<T>(state: &mut ChannelState<T>) {
    if !state.receiver_alive {
        retire_watches(&mut state.receive_watches);
    } else if receive_ready(state) {
        fire_watches(&mut state.receive_watches);
    }
}

fn fire_watches(watches: &mut Vec<Weak<WatchCore>>) {
    watches.retain(|watch| {
        let Some(watch) = watch.upgrade() else {
            return false;
        };
        watch.fire();
        true
    });
}

fn retire_watches(watches: &mut Vec<Weak<WatchCore>>) {
    watches.retain(|watch| {
        let Some(watch) = watch.upgrade() else {
            return false;
        };
        watch.retire();
        true
    });
}

fn remove_watch(watches: &mut Vec<Weak<WatchCore>>, target: &Weak<WatchCore>) {
    watches.retain(|watch| watch.strong_count() > 0 && !Weak::ptr_eq(watch, target));
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(2);

    fn wait_for_wake(receiver: &WakeReceiver) -> WakeAttempt {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let attempt = receiver.try_receive();
            if attempt != WakeAttempt::Empty || Instant::now() >= deadline {
                return attempt;
            }
            thread::yield_now();
        }
    }

    #[test]
    fn buffered_try_operations_preserve_ownership_and_closure() {
        let (sender, receiver) = channel(1);
        assert_eq!(sender.try_send(1), SendAttempt::Sent);
        assert_eq!(sender.try_send(2), SendAttempt::Full(2));
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(1));
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Empty);
        drop(receiver);
        assert_eq!(sender.try_send(3), SendAttempt::Closed(3));
    }

    #[test]
    fn buffered_receive_drains_before_closed() {
        let (sender, receiver) = channel(2);
        assert_eq!(sender.try_send(1), SendAttempt::Sent);
        assert_eq!(sender.try_send(2), SendAttempt::Sent);
        drop(sender);
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(1));
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(2));
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Closed);
    }

    #[test]
    fn rendezvous_try_operations_do_not_reserve_absent_counterpart() {
        let (sender, receiver) = channel(0);
        assert_eq!(sender.try_send(1), SendAttempt::Full(1));
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Empty);
    }

    #[test]
    fn rendezvous_try_send_commits_to_waiting_receiver() {
        let (sender, receiver) = channel(0);
        let (notifier, wake_receiver) = wake_channel();
        let mut watch = watch_send_ready(&sender, notifier);
        watch.arm();

        let (result_sender, result_receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            result_sender.send(receiver.receive()).unwrap();
        });

        assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
        assert_eq!(sender.try_send(7), SendAttempt::Sent);
        assert_eq!(result_receiver.recv_timeout(TIMEOUT).unwrap(), Some(7));
        worker.join().unwrap();
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Empty);
        watch.arm();
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Item);
        assert_eq!(sender.try_send(8), SendAttempt::Closed(8));
    }

    #[test]
    fn rendezvous_try_receive_commits_pending_sender() {
        let (sender, receiver) = channel(0);
        let (notifier, wake_receiver) = wake_channel();
        let mut watch = watch_receive_ready(&receiver, notifier);
        watch.arm();

        let worker = thread::spawn(move || sender.send(9));
        assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(9));
        assert_eq!(worker.join().unwrap(), Ok(()));
    }

    #[test]
    fn wake_notifications_coalesce_and_close_after_pending_drains() {
        let (notifier, receiver) = wake_channel();
        assert!(notifier.wake());
        assert!(notifier.wake());
        drop(notifier);
        assert_eq!(receiver.try_receive(), WakeAttempt::Item);
        assert_eq!(receiver.try_receive(), WakeAttempt::Closed);
    }

    #[test]
    fn buffered_send_watch_fires_when_capacity_opens() {
        let (sender, receiver) = channel(1);
        assert_eq!(sender.try_send(1), SendAttempt::Sent);
        let (notifier, wake_receiver) = wake_channel();
        let mut watch = watch_send_ready(&sender, notifier);
        watch.arm();
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Empty);
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(1));
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Item);
        assert_eq!(sender.try_send(2), SendAttempt::Sent);
    }

    #[test]
    fn watch_is_one_shot_and_rearm_rechecks() {
        let (sender, receiver) = channel(1);
        let (root, wake_receiver) = wake_channel();
        let mut watch = watch_receive_ready(&receiver, root.clone());
        watch.arm();
        assert_eq!(sender.try_send(1), SendAttempt::Sent);
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Item);
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(1));

        assert_eq!(sender.try_send(2), SendAttempt::Sent);
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Empty);
        watch.arm();
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Item);
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(2));
    }

    #[test]
    fn disarm_quiesces_future_notification() {
        let (sender, receiver) = channel(1);
        let (notifier, wake_receiver) = wake_channel();
        let mut watch = watch_receive_ready(&receiver, notifier);
        watch.arm();
        watch.disarm();
        assert_eq!(sender.try_send(1), SendAttempt::Sent);
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Empty);
    }

    #[test]
    fn watches_do_not_keep_application_endpoints_alive() {
        let (sender, _receiver) = channel::<i32>(1);
        assert_eq!(sender.try_send(1), SendAttempt::Sent);
        let (notifier, wake_receiver) = wake_channel();
        let mut send_watch = watch_send_ready(&sender, notifier);
        send_watch.arm();
        drop(sender);
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Empty);

        let (sender, receiver) = channel::<i32>(1);
        let (notifier, wake_receiver) = wake_channel();
        let mut receive_watch = watch_receive_ready(&receiver, notifier);
        receive_watch.arm();
        drop(receiver);
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Empty);
        assert_eq!(sender.try_send(2), SendAttempt::Closed(2));
    }

    #[test]
    fn receiver_loss_returns_pending_rendezvous_input() {
        let (sender, receiver) = channel(0);
        let (notifier, wake_receiver) = wake_channel();
        let mut watch = watch_receive_ready(&receiver, notifier);
        watch.arm();
        let worker = thread::spawn(move || sender.send(11));
        assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
        drop(receiver);
        assert_eq!(worker.join().unwrap(), Err(11));
    }

    #[test]
    fn opposite_endpoint_closure_wakes_authoritative_try_operation() {
        let (sender, receiver) = channel::<i32>(1);
        let (notifier, wake_receiver) = wake_channel();
        let mut watch = watch_receive_ready(&receiver, notifier);
        watch.arm();
        drop(sender);
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Item);
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Closed);
    }

    #[test]
    fn target_loss_retires_an_armed_watch() {
        let (sender, receiver) = channel::<i32>(1);
        let (notifier, wake_receiver) = wake_channel();
        let mut watch = watch_receive_ready(&receiver, notifier);
        watch.arm();
        assert!(watch.core.armed.load(Ordering::SeqCst));
        drop(wake_receiver);
        assert!(!watch.core.armed.load(Ordering::SeqCst));
        watch.arm();
        assert_eq!(sender.try_send(1), SendAttempt::Sent);
        watch.disarm();
        assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(1));
    }

    #[test]
    fn dropped_watches_remove_channel_and_wake_registrations() {
        let (sender, receiver) = channel::<i32>(1);
        let (notifier, wake_receiver) = wake_channel();
        for _ in 0..128 {
            let watch = watch_receive_ready(&receiver, notifier.clone());
            drop(watch);
        }
        assert!(lock(&receiver.inner.state).receive_watches.is_empty());
        assert!(lock(&notifier.inner.state).watches.is_empty());
        drop(sender);
        drop(wake_receiver);
    }

    #[test]
    fn arm_and_transition_race_has_no_lost_wake() {
        for value in 0..256 {
            let (sender, receiver) = channel(1);
            let (notifier, wake_receiver) = wake_channel();
            let mut watch = watch_receive_ready(&receiver, notifier);
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let sender_barrier = Arc::clone(&barrier);
            let worker = thread::spawn(move || {
                sender_barrier.wait();
                sender.try_send(value)
            });

            barrier.wait();
            watch.arm();
            assert_eq!(worker.join().unwrap(), SendAttempt::Sent);
            assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
            assert_eq!(receiver.try_receive(), ReceiveAttempt::Item(value));
        }
    }
}
