use crate::WakeNotifier;
use mio::event::Source;
use mio::{Events, Interest, Poll, Registry, Token, Waker};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::{self, JoinHandle};

const WAKE_TOKEN: Token = Token(0);
const EVENT_CAPACITY: usize = 1024;

/// Owns the private OS readiness thread. Dropping it stops and joins the thread.
pub struct Reactor {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

/// Cloneable handle to one explicitly owned reactor.
#[derive(Clone)]
pub struct ReactorHandle {
    shared: Arc<Shared>,
}

/// One registered readiness generation. The source owner must withdraw it.
pub(crate) struct ReactorRegistration {
    token: Token,
    state: Arc<RegistrationState>,
}

#[derive(Clone, Debug)]
struct ReactorFailure {
    kind: io::ErrorKind,
    raw_os_error: Option<i32>,
    message: Arc<str>,
}

impl ReactorFailure {
    fn capture(error: io::Error) -> Self {
        Self {
            kind: error.kind(),
            raw_os_error: error.raw_os_error(),
            message: Arc::from(error.to_string()),
        }
    }

    fn to_io_error(&self) -> io::Error {
        if let Some(code) = self.raw_os_error {
            io::Error::from_raw_os_error(code)
        } else {
            io::Error::new(self.kind, self.message.to_string())
        }
    }
}

impl Reactor {
    pub fn new() -> io::Result<Self> {
        let mut poll = Poll::new()?;
        let registry = Arc::new(poll.registry().try_clone()?);
        let waker = Arc::new(Waker::new(poll.registry(), WAKE_TOKEN)?);
        let shared = Arc::new(Shared {
            registry,
            waker,
            registrations: Mutex::new(HashMap::new()),
            next_token: AtomicUsize::new(1),
            stopping: AtomicBool::new(false),
            failure: Mutex::new(None),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("rill-readiness".into())
            .spawn(move || run(&mut poll, &thread_shared))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    pub fn handle(&self) -> ReactorHandle {
        ReactorHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    #[cfg(test)]
    pub(crate) fn fail_for_test(&self, error: io::Error) {
        self.shared.fail(error);
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        let _ = self.shared.waker.wake();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let registrations: Vec<_> = lock(&self.shared.registrations)
            .drain()
            .filter_map(|(_, registration)| registration.upgrade())
            .collect();
        for registration in registrations {
            registration.deactivate();
        }
    }
}

impl ReactorHandle {
    pub(crate) fn register<S: Source + ?Sized>(
        &self,
        source: &mut S,
        interests: Interest,
        notifier: WakeNotifier,
    ) -> io::Result<ReactorRegistration> {
        self.check_available()?;
        let token = self.next_token()?;
        let state = Arc::new(RegistrationState {
            active: Mutex::new(RegistrationActivity {
                active: true,
                in_flight: 0,
            }),
            quiescent: Condvar::new(),
            notifier,
        });
        lock(&self.shared.registrations).insert(token, Arc::downgrade(&state));
        if let Err(error) = self.shared.registry.register(source, token, interests) {
            lock(&self.shared.registrations).remove(&token);
            state.deactivate();
            return Err(error);
        }
        Ok(ReactorRegistration { token, state })
    }

    pub(crate) fn withdraw<S: Source + ?Sized>(
        &self,
        source: &mut S,
        registration: ReactorRegistration,
    ) -> io::Result<()> {
        registration.state.deactivate();
        let result = self.shared.registry.deregister(source);
        lock(&self.shared.registrations).remove(&registration.token);
        registration.state.wait_quiescent();
        result
    }

    pub(crate) fn failure(&self) -> Option<io::Error> {
        lock(&self.shared.failure)
            .as_ref()
            .map(ReactorFailure::to_io_error)
    }

    fn check_available(&self) -> io::Result<()> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        if self.shared.stopping.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "readiness reactor is stopping",
            ));
        }
        Ok(())
    }

    fn next_token(&self) -> io::Result<Token> {
        self.shared
            .next_token
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |token| {
                token.checked_add(1)
            })
            .map(Token)
            .map_err(|_| io::Error::other("readiness token space exhausted"))
    }
}

struct Shared {
    registry: Arc<Registry>,
    waker: Arc<Waker>,
    registrations: Mutex<HashMap<Token, Weak<RegistrationState>>>,
    next_token: AtomicUsize,
    stopping: AtomicBool,
    failure: Mutex<Option<ReactorFailure>>,
}

impl Shared {
    fn fail(&self, error: io::Error) {
        let mut failure = lock(&self.failure);
        if failure.is_some() {
            return;
        }
        *failure = Some(ReactorFailure::capture(error));
        drop(failure);
        let registrations: Vec<_> = lock(&self.registrations)
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        for registration in registrations {
            registration.fire();
        }
    }
}

struct RegistrationState {
    active: Mutex<RegistrationActivity>,
    quiescent: Condvar,
    notifier: WakeNotifier,
}

struct RegistrationActivity {
    active: bool,
    in_flight: usize,
}

impl RegistrationState {
    fn fire(&self) {
        {
            let mut activity = lock(&self.active);
            if !activity.active {
                return;
            }
            activity.active = false;
            activity.in_flight += 1;
        }
        let _ = self.notifier.wake();
        let mut activity = lock(&self.active);
        activity.in_flight -= 1;
        if activity.in_flight == 0 {
            self.quiescent.notify_all();
        }
    }

    fn deactivate(&self) {
        let mut activity = lock(&self.active);
        activity.active = false;
        while activity.in_flight != 0 {
            activity = wait(&self.quiescent, activity);
        }
    }

    fn wait_quiescent(&self) {
        let mut activity = lock(&self.active);
        while activity.in_flight != 0 {
            activity = wait(&self.quiescent, activity);
        }
    }
}

fn run(poll: &mut Poll, shared: &Shared) {
    let mut events = Events::with_capacity(EVENT_CAPACITY);
    while !shared.stopping.load(Ordering::Acquire) {
        match poll.poll(&mut events, None) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                shared.fail(error);
                return;
            }
        }
        for event in &events {
            let token = event.token();
            if token == WAKE_TOKEN {
                continue;
            }
            let registration = lock(&shared.registrations)
                .remove(&token)
                .and_then(|registration| registration.upgrade());
            if let Some(registration) = registration {
                registration.fire();
            }
        }
    }
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
    use crate::wake_channel;
    use std::net::{TcpListener, TcpStream};

    fn stream() -> (mio::net::TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).unwrap();
        let (server, _) = listener.accept().unwrap();
        client.set_nonblocking(true).unwrap();
        (mio::net::TcpStream::from_std(client), server)
    }

    #[test]
    fn token_exhaustion_rejects_before_registration() {
        let reactor = Reactor::new().unwrap();
        let handle = reactor.handle();
        handle
            .shared
            .next_token
            .store(usize::MAX, Ordering::Relaxed);
        let (mut stream, _server) = stream();
        let (notifier, _receiver) = wake_channel();
        let error = match handle.register(&mut stream, Interest::READABLE, notifier) {
            Ok(_) => panic!("exhausted token space registered a source"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(lock(&handle.shared.registrations).is_empty());
    }

    #[test]
    fn reactor_drop_retires_active_callback_generation() {
        let reactor = Reactor::new().unwrap();
        let handle = reactor.handle();
        let (mut stream, _server) = stream();
        let (notifier, _receiver) = wake_channel();
        let registration = handle
            .register(&mut stream, Interest::READABLE, notifier)
            .unwrap();
        assert!(lock(&registration.state.active).active);
        drop(reactor);
        assert!(!lock(&registration.state.active).active);
    }
}
