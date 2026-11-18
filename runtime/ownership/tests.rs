use rill_ownership::{channel, mutex, shared};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Payload<'a>(&'a AtomicUsize);
impl Drop for Payload<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Relaxed);
    }
}

#[test]
fn shared_retains_the_same_payload_until_final_release() {
    let drops = AtomicUsize::new(0);
    let original = shared::new(Payload(&drops));
    let second = shared::retain(&original);
    assert!(std::ptr::eq(shared::view(&original), shared::view(&second)));
    shared::release(original);
    assert_eq!(drops.load(Relaxed), 0);
    shared::release(second);
    assert_eq!(drops.load(Relaxed), 1);
}

#[test]
fn guard_release_on_another_worker_republishes_the_payload() {
    let drops = AtomicUsize::new(0);
    let owner = mutex::new((Payload(&drops), 0));
    let mut guard = mutex::acquire(&owner);
    guard.1 = 42;
    std::thread::scope(|scope| {
        scope.spawn(move || mutex::release(guard)).join().unwrap();
    });
    assert_eq!(drops.load(Relaxed), 0);
    let next = mutex::acquire(&owner);
    assert_eq!(next.1, 42);
    mutex::release(next);
    drop(owner);
    assert_eq!(drops.load(Relaxed), 1);
}

#[test]
fn copied_sender_keeps_the_channel_open_and_close_drains() {
    let (sender, receiver) = channel::new(1);
    let second = channel::retain_sender(&sender);
    channel::release_sender(sender);
    assert_eq!(channel::send(&second, 42), Ok(()));
    channel::release_sender(second);
    assert_eq!(channel::receive(&receiver), Some(42));
    assert_eq!(channel::receive(&receiver), None);
    channel::release_receiver(receiver);
}

#[test]
fn receiver_release_disposes_buffered_messages_and_rejects_new_sends() {
    let drops = AtomicUsize::new(0);
    let (sender, receiver) = channel::new(1);
    assert!(channel::send(&sender, Payload(&drops)).is_ok());
    channel::release_receiver(receiver);
    assert_eq!(drops.load(Relaxed), 1);
    let unsent = channel::send(&sender, Payload(&drops)).err().unwrap();
    assert_eq!(drops.load(Relaxed), 1);
    drop(unsent);
    assert_eq!(drops.load(Relaxed), 2);
    channel::release_sender(sender);
}

#[test]
fn receiver_release_returns_the_uncommitted_rendezvous_message() {
    let drops = AtomicUsize::new(0);
    let (sender, receiver) = channel::new(0);
    std::thread::scope(|scope| {
        let (started, proceed) = std::sync::mpsc::channel();
        let (sender, drops) = (&sender, &drops);
        let send = scope.spawn(move || {
            started.send(()).unwrap();
            channel::send(sender, Payload(drops))
        });
        proceed.recv().unwrap();
        channel::release_receiver(receiver);
        let unsent = send.join().unwrap().err().unwrap();
        assert_eq!(drops.load(Relaxed), 0);
        drop(unsent);
    });
    assert_eq!(drops.load(Relaxed), 1);
}
