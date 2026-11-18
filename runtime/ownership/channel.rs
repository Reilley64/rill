use std::sync::{Mutex, mpsc};

pub struct Sender<T>(mpsc::SyncSender<T>);
pub struct Receiver<T>(Mutex<mpsc::Receiver<T>>);

pub fn new<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    (Sender(sender), Receiver(Mutex::new(receiver)))
}

pub fn retain_sender<T>(sender: &Sender<T>) -> Sender<T> {
    Sender(sender.0.clone())
}

pub fn release_sender<T>(sender: Sender<T>) {
    drop(sender);
}

pub fn release_receiver<T>(receiver: Receiver<T>) {
    drop(receiver);
}

// A blocking native call borrows its endpoint until return. It owns the
// message until delivery or returns it on failure; it cannot detach a wait.
pub fn send<T>(sender: &Sender<T>, value: T) -> Result<(), T> {
    sender.0.send(value).map_err(|error| error.0)
}

pub fn receive<T>(receiver: &Receiver<T>) -> Option<T> {
    receiver.0.lock().unwrap().recv().ok()
}
