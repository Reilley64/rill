use crate::reactor::ReactorRegistration;
use crate::{ReactorHandle, WakeNotifier};
use mio::Interest;
use mio::net::TcpStream as MioTcpStream;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

pub const MAX_PROGRESS_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressSchedule {
    Runnable,
    Waiting,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionReason {
    Busy,
    Closing,
    Stopping,
    Terminal,
    InvalidInput,
    LimitExceeded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Admitted,
    Rejected(AdmissionReason),
}

#[derive(Debug, PartialEq, Eq)]
pub struct WriteRejection {
    pub reason: AdmissionReason,
    pub input: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WriteAdmission {
    Admitted,
    Rejected(WriteRejection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interruption {
    ReadFailed,
    WriteFailed,
    FlushFailed,
    CloseFailed,
    DriverFailed,
    StopRequested,
}

#[derive(Debug)]
pub struct ReadFailure {
    pub data: Vec<u8>,
    pub error: io::Error,
}

#[derive(Debug)]
pub enum ReadOutcome {
    Data(Vec<u8>),
    End,
    Failed(ReadFailure),
    Interrupted(Interruption),
}

#[derive(Debug)]
pub struct WriteFailure {
    pub transferred: u64,
    pub error: io::Error,
}

#[derive(Debug)]
pub enum WriteOutcome {
    Completed,
    Failed(WriteFailure),
    Interrupted(Interruption),
}

#[derive(Debug)]
pub enum OperationOutcome {
    Completed,
    Failed(io::Error),
    Interrupted(Interruption),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverFailurePhase {
    TransportProgress,
    WaitRegistration,
    Release,
}

#[derive(Debug)]
pub struct DriverFailure {
    pub phase: DriverFailurePhase,
    pub error: io::Error,
}

#[derive(Debug)]
pub struct DriverFailureSet {
    pub primary: DriverFailure,
    pub cleanup: Option<DriverFailure>,
}

#[derive(Debug)]
pub struct ProgressStep {
    pub schedule: ProgressSchedule,
    pub stop_finished: bool,
    pub read: Option<ReadOutcome>,
    pub write: Option<WriteOutcome>,
    pub flush: Option<OperationOutcome>,
    pub close: Option<OperationOutcome>,
    pub driver_failures: Option<DriverFailureSet>,
}

impl ProgressStep {
    fn new() -> Self {
        Self {
            schedule: ProgressSchedule::Idle,
            stop_finished: false,
            read: None,
            write: None,
            flush: None,
            close: None,
            driver_failures: None,
        }
    }
}

/// One-owner nonblocking TCP progress state.
pub struct TcpProgress {
    reactor: ReactorHandle,
    socket: Mutex<SocketState>,
    read: ReadLane,
    write: WriteLane,
    flush: FlushLane,
    close: CloseLane,
    read_ended: bool,
    stopping: bool,
    terminal: bool,
    stop_requested: AtomicBool,
    wait_active: AtomicBool,
    wait_failure: Mutex<Option<io::Error>>,
}

/// Creates progress state without performing native socket setup.
pub fn tcp_progress(stream: TcpStream, reactor: ReactorHandle) -> TcpProgress {
    TcpProgress {
        reactor,
        socket: Mutex::new(SocketState::Unprepared(Some(stream))),
        read: ReadLane::Idle,
        write: WriteLane::Idle,
        flush: FlushLane::Idle,
        close: CloseLane::Idle,
        read_ended: false,
        stopping: false,
        terminal: false,
        stop_requested: AtomicBool::new(false),
        wait_active: AtomicBool::new(false),
        wait_failure: Mutex::new(None),
    }
}

impl TcpProgress {
    pub fn admit_read(&mut self, maximum: u64) -> Admission {
        if maximum == 0 {
            return Admission::Rejected(AdmissionReason::InvalidInput);
        }
        if let Some(reason) = self.admission_rejection() {
            return Admission::Rejected(reason);
        }
        if !matches!(self.read, ReadLane::Idle) {
            return Admission::Rejected(AdmissionReason::Busy);
        }
        self.read = ReadLane::Pending(maximum.min(MAX_PROGRESS_BYTES as u64) as usize);
        Admission::Admitted
    }

    pub fn admit_write(&mut self, input: Vec<u8>) -> WriteAdmission {
        if input.len() > MAX_PROGRESS_BYTES {
            return WriteAdmission::Rejected(WriteRejection {
                reason: AdmissionReason::LimitExceeded,
                input,
            });
        }
        if let Some(reason) = self.admission_rejection() {
            return WriteAdmission::Rejected(WriteRejection { reason, input });
        }
        if !matches!(self.write, WriteLane::Idle) || !matches!(self.flush, FlushLane::Idle) {
            return WriteAdmission::Rejected(WriteRejection {
                reason: AdmissionReason::Busy,
                input,
            });
        }
        self.write = WriteLane::Pending { input, cursor: 0 };
        WriteAdmission::Admitted
    }

    pub fn admit_flush(&mut self) -> Admission {
        if let Some(reason) = self.admission_rejection() {
            return Admission::Rejected(reason);
        }
        if !matches!(self.flush, FlushLane::Idle) {
            return Admission::Rejected(AdmissionReason::Busy);
        }
        self.flush = FlushLane::Pending;
        Admission::Admitted
    }

    pub fn admit_close(&mut self) -> Admission {
        if let Some(reason) = self.admission_rejection() {
            return Admission::Rejected(reason);
        }
        if !matches!(self.read, ReadLane::Idle)
            || !matches!(self.write, WriteLane::Idle)
            || !matches!(self.flush, FlushLane::Idle)
            || !matches!(self.close, CloseLane::Idle)
        {
            return Admission::Rejected(AdmissionReason::Busy);
        }
        self.close = CloseLane::Pending;
        Admission::Admitted
    }

    pub fn request_stop(&mut self) {
        if !self.terminal {
            self.stopping = true;
        }
    }

    pub fn advance(&mut self) -> ProgressStep {
        let mut step = ProgressStep::new();
        if self.terminal {
            return step;
        }

        let mut driver_failed = false;
        if let Some(error) = lock(&self.wait_failure).take() {
            step.driver_failures = Some(DriverFailureSet {
                primary: DriverFailure {
                    phase: DriverFailurePhase::WaitRegistration,
                    error,
                },
                cleanup: None,
            });
            self.stopping = true;
            driver_failed = true;
        }
        if self.stop_requested.swap(false, Ordering::AcqRel) {
            self.stopping = true;
            driver_failed = true;
        }
        if self.stopping {
            self.finish_stop(
                &mut step,
                if driver_failed {
                    Interruption::DriverFailed
                } else {
                    Interruption::StopRequested
                },
            );
            return step;
        }
        if let Some(error) = self.reactor.failure() {
            step.driver_failures = Some(DriverFailureSet {
                primary: DriverFailure {
                    phase: DriverFailurePhase::WaitRegistration,
                    error,
                },
                cleanup: None,
            });
            self.finish_stop(&mut step, Interruption::DriverFailed);
            return step;
        }
        if let Err(error) = self.prepare_socket() {
            step.driver_failures = Some(DriverFailureSet {
                primary: DriverFailure {
                    phase: DriverFailurePhase::TransportProgress,
                    error,
                },
                cleanup: None,
            });
            self.finish_stop(&mut step, Interruption::DriverFailed);
            return step;
        }

        if matches!(self.close, CloseLane::Pending) {
            let release = self.release_socket();
            self.close = CloseLane::Idle;
            self.terminal = true;
            step.close = Some(match release {
                Some(error) => OperationOutcome::Failed(error),
                None => OperationOutcome::Completed,
            });
            return step;
        }

        let read_failed = self.advance_read(&mut step);
        if read_failed {
            self.interrupt_after_read_failure(&mut step);
            return step;
        }
        let write_before = self.write_cursor();
        let write_failed = self.advance_write(&mut step);
        let write_progressed = match (write_before, self.write_cursor()) {
            (Some(before), Some(after)) => after > before,
            _ => false,
        };
        if write_failed {
            self.interrupt_after_write_failure(&mut step);
            return step;
        }
        let flush_failed = self.advance_flush(&mut step);
        if flush_failed {
            self.interrupt_after_flush_failure(&mut step);
            return step;
        }

        step.schedule = if self.has_pending() {
            if write_progressed {
                ProgressSchedule::Runnable
            } else {
                ProgressSchedule::Waiting
            }
        } else {
            ProgressSchedule::Idle
        };
        step
    }

    pub fn register_wait(&self, notifier: WakeNotifier) -> io::Result<TcpProgressWait<'_>> {
        if self
            .wait_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "TCP progress wait already registered",
            ));
        }
        if self.terminal || self.stopping || lock(&self.wait_failure).is_some() {
            let _ = notifier.wake();
            return Ok(TcpProgressWait {
                driver: self,
                registration: None,
            });
        }
        let Some(interests) = self.wait_interests() else {
            let _ = notifier.wake();
            return Ok(TcpProgressWait {
                driver: self,
                registration: None,
            });
        };
        let mut socket = lock(&self.socket);
        let SocketState::Ready(socket) = &mut *socket else {
            let _ = notifier.wake();
            return Ok(TcpProgressWait {
                driver: self,
                registration: None,
            });
        };
        match self.reactor.register(socket, interests, notifier) {
            Ok(registration) => Ok(TcpProgressWait {
                driver: self,
                registration: Some(registration),
            }),
            Err(error) => {
                self.wait_active.store(false, Ordering::Release);
                self.stop_requested.store(true, Ordering::Release);
                Err(error)
            }
        }
    }

    fn admission_rejection(&self) -> Option<AdmissionReason> {
        if self.terminal {
            Some(AdmissionReason::Terminal)
        } else if self.stopping || self.stop_requested.load(Ordering::Acquire) {
            Some(AdmissionReason::Stopping)
        } else if matches!(self.close, CloseLane::Pending) {
            Some(AdmissionReason::Closing)
        } else {
            None
        }
    }

    fn prepare_socket(&mut self) -> io::Result<()> {
        let mut socket = lock(&self.socket);
        let SocketState::Unprepared(stream) = &mut *socket else {
            return Ok(());
        };
        let stream = stream.take().expect("unprepared TCP owner exists");
        stream.set_nonblocking(true)?;
        *socket = SocketState::Ready(MioTcpStream::from_std(stream));
        Ok(())
    }

    fn advance_read(&mut self, step: &mut ProgressStep) -> bool {
        let ReadLane::Pending(maximum) = self.read else {
            return false;
        };
        if self.read_ended {
            self.read = ReadLane::Idle;
            step.read = Some(ReadOutcome::End);
            return false;
        }
        let mut data = vec![0; maximum];
        let result = {
            let mut socket = lock(&self.socket);
            let SocketState::Ready(socket) = &mut *socket else {
                unreachable!("prepared socket required")
            };
            socket.read(&mut data)
        };
        match result {
            Ok(0) => {
                self.read = ReadLane::Idle;
                self.read_ended = true;
                step.read = Some(ReadOutcome::End);
                false
            }
            Ok(count) => {
                data.truncate(count);
                self.read = ReadLane::Idle;
                step.read = Some(ReadOutcome::Data(data));
                false
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => false,
            Err(error) => {
                self.read = ReadLane::Idle;
                step.read = Some(ReadOutcome::Failed(ReadFailure {
                    data: Vec::new(),
                    error,
                }));
                true
            }
        }
    }

    fn advance_write(&mut self, step: &mut ProgressStep) -> bool {
        let WriteLane::Pending { input, cursor } = &mut self.write else {
            return false;
        };
        if *cursor == input.len() {
            self.write = WriteLane::Idle;
            step.write = Some(WriteOutcome::Completed);
            return false;
        }
        let result = {
            let mut socket = lock(&self.socket);
            let SocketState::Ready(socket) = &mut *socket else {
                unreachable!("prepared socket required")
            };
            socket.write(&input[*cursor..])
        };
        match result {
            Ok(0) => {
                let transferred = *cursor as u64;
                self.write = WriteLane::Idle;
                step.write = Some(WriteOutcome::Failed(WriteFailure {
                    transferred,
                    error: io::Error::from(io::ErrorKind::WriteZero),
                }));
                true
            }
            Ok(count) => {
                *cursor += count;
                if *cursor == input.len() {
                    self.write = WriteLane::Idle;
                    step.write = Some(WriteOutcome::Completed);
                }
                false
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => false,
            Err(error) => {
                let transferred = *cursor as u64;
                self.write = WriteLane::Idle;
                step.write = Some(WriteOutcome::Failed(WriteFailure { transferred, error }));
                true
            }
        }
    }

    fn advance_flush(&mut self, step: &mut ProgressStep) -> bool {
        if !matches!(self.flush, FlushLane::Pending) || !matches!(self.write, WriteLane::Idle) {
            return false;
        }
        let result = {
            let mut socket = lock(&self.socket);
            let SocketState::Ready(socket) = &mut *socket else {
                unreachable!("prepared socket required")
            };
            socket.flush()
        };
        match result {
            Ok(()) => {
                self.flush = FlushLane::Idle;
                step.flush = Some(OperationOutcome::Completed);
                false
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => false,
            Err(error) => {
                self.flush = FlushLane::Idle;
                step.flush = Some(OperationOutcome::Failed(error));
                true
            }
        }
    }

    fn interrupt_after_read_failure(&mut self, step: &mut ProgressStep) {
        if !matches!(self.write, WriteLane::Idle) {
            self.write = WriteLane::Idle;
            step.write = Some(WriteOutcome::Interrupted(Interruption::ReadFailed));
        }
        if !matches!(self.flush, FlushLane::Idle) {
            self.flush = FlushLane::Idle;
            step.flush = Some(OperationOutcome::Interrupted(Interruption::ReadFailed));
        }
        self.record_release_failure(step);
        self.terminal = true;
        step.stop_finished = true;
    }

    fn interrupt_after_write_failure(&mut self, step: &mut ProgressStep) {
        if !matches!(self.read, ReadLane::Idle) {
            self.read = ReadLane::Idle;
            step.read = Some(ReadOutcome::Interrupted(Interruption::WriteFailed));
        }
        if !matches!(self.flush, FlushLane::Idle) {
            self.flush = FlushLane::Idle;
            step.flush = Some(OperationOutcome::Interrupted(Interruption::WriteFailed));
        }
        self.record_release_failure(step);
        self.terminal = true;
        step.stop_finished = true;
    }

    fn interrupt_after_flush_failure(&mut self, step: &mut ProgressStep) {
        if !matches!(self.read, ReadLane::Idle) {
            self.read = ReadLane::Idle;
            step.read = Some(ReadOutcome::Interrupted(Interruption::FlushFailed));
        }
        self.record_release_failure(step);
        self.terminal = true;
        step.stop_finished = true;
    }

    fn finish_stop(&mut self, step: &mut ProgressStep, reason: Interruption) {
        if !matches!(self.read, ReadLane::Idle) {
            self.read = ReadLane::Idle;
            step.read = Some(ReadOutcome::Interrupted(reason));
        }
        if !matches!(self.write, WriteLane::Idle) {
            self.write = WriteLane::Idle;
            step.write = Some(WriteOutcome::Interrupted(reason));
        }
        if !matches!(self.flush, FlushLane::Idle) {
            self.flush = FlushLane::Idle;
            step.flush = Some(OperationOutcome::Interrupted(reason));
        }
        if !matches!(self.close, CloseLane::Idle) {
            self.close = CloseLane::Idle;
            step.close = Some(OperationOutcome::Interrupted(reason));
        }
        self.record_release_failure(step);
        self.stopping = false;
        self.terminal = true;
        step.stop_finished = true;
    }

    fn release_socket(&self) -> Option<io::Error> {
        let mut socket = lock(&self.socket);
        let previous = std::mem::replace(&mut *socket, SocketState::Gone);
        match previous {
            SocketState::Unprepared(Some(stream)) => stream.shutdown(Shutdown::Both).err(),
            SocketState::Ready(stream) => stream.shutdown(Shutdown::Both).err(),
            SocketState::Unprepared(None) | SocketState::Gone => None,
        }
    }

    fn record_release_failure(&self, step: &mut ProgressStep) {
        let Some(error) = self.release_socket() else {
            return;
        };
        let failure = DriverFailure {
            phase: DriverFailurePhase::Release,
            error,
        };
        if let Some(failures) = &mut step.driver_failures {
            failures.cleanup = Some(failure);
        } else {
            step.driver_failures = Some(DriverFailureSet {
                primary: failure,
                cleanup: None,
            });
        }
    }

    fn has_pending(&self) -> bool {
        !matches!(self.read, ReadLane::Idle)
            || !matches!(self.write, WriteLane::Idle)
            || !matches!(self.flush, FlushLane::Idle)
    }

    fn write_cursor(&self) -> Option<usize> {
        match self.write {
            WriteLane::Pending { cursor, .. } => Some(cursor),
            WriteLane::Idle => None,
        }
    }

    fn wait_interests(&self) -> Option<Interest> {
        let read = matches!(self.read, ReadLane::Pending(_));
        let write =
            !matches!(self.write, WriteLane::Idle) || !matches!(self.flush, FlushLane::Idle);
        match (read, write) {
            (true, true) => Some(Interest::READABLE | Interest::WRITABLE),
            (true, false) => Some(Interest::READABLE),
            (false, true) => Some(Interest::WRITABLE),
            (false, false) => None,
        }
    }
}

impl Drop for TcpProgress {
    fn drop(&mut self) {
        if !self.terminal {
            let _ = self.release_socket();
            self.terminal = true;
        }
    }
}

pub struct TcpProgressWait<'a> {
    driver: &'a TcpProgress,
    registration: Option<ReactorRegistration>,
}

impl TcpProgressWait<'_> {
    pub fn withdraw(mut self) -> io::Result<()> {
        let result = self.withdraw_inner();
        if result.is_err() {
            self.driver.stop_requested.store(true, Ordering::Release);
        }
        result
    }

    fn withdraw_inner(&mut self) -> io::Result<()> {
        let result = if let Some(registration) = self.registration.take() {
            let mut socket = lock(&self.driver.socket);
            if let SocketState::Ready(socket) = &mut *socket {
                self.driver.reactor.withdraw(socket, registration)
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };
        self.driver.wait_active.store(false, Ordering::Release);
        result
    }
}

impl Drop for TcpProgressWait<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.withdraw_inner() {
            self.driver.stop_requested.store(true, Ordering::Release);
            let mut failure = lock(&self.driver.wait_failure);
            if failure.is_none() {
                *failure = Some(error);
            }
        }
    }
}

enum SocketState {
    Unprepared(Option<TcpStream>),
    Ready(MioTcpStream),
    Gone,
}

enum ReadLane {
    Idle,
    Pending(usize),
}

enum WriteLane {
    Idle,
    Pending { input: Vec<u8>, cursor: usize },
}

enum FlushLane {
    Idle,
    Pending,
}

enum CloseLane {
    Idle,
    Pending,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Reactor, WakeAttempt, WakeReceiver, wake_channel};
    use socket2::SockRef;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream as StdTcpStream};
    use std::thread;
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(2);

    fn pair() -> (StdTcpStream, StdTcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let client = StdTcpStream::connect(address).unwrap();
        let (server, _) = listener.accept().unwrap();
        server.set_read_timeout(Some(TIMEOUT)).unwrap();
        (client, server)
    }

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

    fn complete_read(progress: &mut TcpProgress) -> ReadOutcome {
        loop {
            let step = progress.advance();
            if let Some(outcome) = step.read {
                return outcome;
            }
            assert_eq!(step.schedule, ProgressSchedule::Waiting);
            let (notifier, wake_receiver) = wake_channel();
            let wait = progress.register_wait(notifier).unwrap();
            assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
            wait.withdraw().unwrap();
        }
    }

    #[test]
    fn read_wait_registration_wakes_and_returns_owned_data() {
        let reactor = Reactor::new().unwrap();
        let (client, mut server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        assert_eq!(progress.admit_read(64), Admission::Admitted);
        assert_eq!(progress.advance().schedule, ProgressSchedule::Waiting);

        let (notifier, wake_receiver) = wake_channel();
        let wait = progress.register_wait(notifier).unwrap();
        server.write_all(b"hello").unwrap();
        assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
        wait.withdraw().unwrap();
        let step = progress.advance();
        match step.read {
            Some(ReadOutcome::Data(data)) => assert_eq!(data, b"hello"),
            other => panic!("unexpected read outcome: {other:?}"),
        }
        assert_eq!(step.schedule, ProgressSchedule::Idle);
    }

    #[test]
    fn registration_rechecks_already_readable_socket() {
        let reactor = Reactor::new().unwrap();
        let (client, mut server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        server.write_all(b"ready").unwrap();
        assert_eq!(progress.admit_read(64), Admission::Admitted);
        let first = progress.advance();
        if matches!(first.read, Some(ReadOutcome::Data(_))) {
            return;
        }
        assert_eq!(first.schedule, ProgressSchedule::Waiting);
        let (notifier, wake_receiver) = wake_channel();
        let wait = progress.register_wait(notifier).unwrap();
        assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
        wait.withdraw().unwrap();
        assert!(matches!(
            progress.advance().read,
            Some(ReadOutcome::Data(_))
        ));
    }

    #[test]
    fn write_and_flush_complete_in_order() {
        let reactor = Reactor::new().unwrap();
        let (client, mut server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        assert!(matches!(
            progress.admit_write(b"payload".to_vec()),
            WriteAdmission::Admitted
        ));
        assert_eq!(progress.admit_flush(), Admission::Admitted);
        let step = progress.advance();
        assert!(matches!(step.write, Some(WriteOutcome::Completed)));
        assert!(matches!(step.flush, Some(OperationOutcome::Completed)));
        let mut received = [0; 7];
        server.read_exact(&mut received).unwrap();
        assert_eq!(&received, b"payload");
    }

    #[test]
    fn read_and_write_lanes_progress_under_one_owner() {
        let reactor = Reactor::new().unwrap();
        let (client, mut server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        assert_eq!(progress.admit_read(16), Admission::Admitted);
        assert!(matches!(
            progress.admit_write(b"outbound".to_vec()),
            WriteAdmission::Admitted
        ));
        assert_eq!(progress.admit_flush(), Admission::Admitted);
        assert!(matches!(
            progress.admit_write(b"later".to_vec()),
            WriteAdmission::Rejected(WriteRejection {
                reason: AdmissionReason::Busy,
                ..
            })
        ));

        let first = progress.advance();
        assert!(matches!(first.write, Some(WriteOutcome::Completed)));
        assert!(matches!(first.flush, Some(OperationOutcome::Completed)));
        assert_eq!(first.schedule, ProgressSchedule::Waiting);
        let mut outbound = [0; 8];
        server.read_exact(&mut outbound).unwrap();
        assert_eq!(&outbound, b"outbound");

        let (notifier, wake_receiver) = wake_channel();
        let wait = progress.register_wait(notifier).unwrap();
        server.write_all(b"inbound").unwrap();
        assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
        wait.withdraw().unwrap();
        match progress.advance().read {
            Some(ReadOutcome::Data(data)) => assert_eq!(data, b"inbound"),
            other => panic!("unexpected read outcome: {other:?}"),
        }
    }

    #[test]
    fn clean_fin_is_end_and_future_reads_end_without_io() {
        let reactor = Reactor::new().unwrap();
        let (client, server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        drop(server);
        assert_eq!(progress.admit_read(8), Admission::Admitted);
        let step = progress.advance();
        if !matches!(step.read, Some(ReadOutcome::End)) {
            let (notifier, wake_receiver) = wake_channel();
            let wait = progress.register_wait(notifier).unwrap();
            assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
            wait.withdraw().unwrap();
            assert!(matches!(progress.advance().read, Some(ReadOutcome::End)));
        }
        assert_eq!(progress.admit_read(8), Admission::Admitted);
        assert!(matches!(progress.advance().read, Some(ReadOutcome::End)));
    }

    #[test]
    fn duplicate_wait_registration_is_rejected_without_replacing_first() {
        let reactor = Reactor::new().unwrap();
        let (client, _server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        assert_eq!(progress.admit_read(8), Admission::Admitted);
        assert_eq!(progress.advance().schedule, ProgressSchedule::Waiting);
        let (root, _wake_receiver) = wake_channel();
        let wait = progress.register_wait(root.clone()).unwrap();
        let error = match progress.register_wait(root) {
            Ok(_) => panic!("duplicate wait registration succeeded"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        wait.withdraw().unwrap();
    }

    #[test]
    fn withdrawn_registration_does_not_deliver_late_wake() {
        let reactor = Reactor::new().unwrap();
        let (client, mut server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        assert_eq!(progress.admit_read(8), Admission::Admitted);
        assert_eq!(progress.advance().schedule, ProgressSchedule::Waiting);
        let (notifier, wake_receiver) = wake_channel();
        progress
            .register_wait(notifier.clone())
            .unwrap()
            .withdraw()
            .unwrap();
        server.write_all(b"x").unwrap();
        thread::sleep(Duration::from_millis(20));
        assert_eq!(wake_receiver.try_receive(), WakeAttempt::Empty);
        assert!(matches!(
            progress.advance().read,
            Some(ReadOutcome::Data(_))
        ));
    }

    #[test]
    fn stop_interrupts_pending_read_and_releases_once() {
        let reactor = Reactor::new().unwrap();
        let (client, _server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        assert_eq!(progress.admit_read(8), Admission::Admitted);
        assert_eq!(progress.advance().schedule, ProgressSchedule::Waiting);
        progress.request_stop();
        let step = progress.advance();
        assert!(step.stop_finished);
        assert!(matches!(
            step.read,
            Some(ReadOutcome::Interrupted(Interruption::StopRequested))
        ));
        assert_eq!(progress.advance().schedule, ProgressSchedule::Idle);
        assert_eq!(
            progress.admit_read(8),
            Admission::Rejected(AdmissionReason::Terminal)
        );
    }

    #[test]
    fn write_limit_and_zero_read_reject_without_socket_io() {
        let reactor = Reactor::new().unwrap();
        let (client, _server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        assert_eq!(
            progress.admit_read(0),
            Admission::Rejected(AdmissionReason::InvalidInput)
        );
        let input = vec![0; MAX_PROGRESS_BYTES + 1];
        match progress.admit_write(input) {
            WriteAdmission::Rejected(rejection) => {
                assert_eq!(rejection.reason, AdmissionReason::LimitExceeded);
                assert_eq!(rejection.input.len(), MAX_PROGRESS_BYTES + 1);
            }
            WriteAdmission::Admitted => panic!("oversized write admitted"),
        }
    }

    #[test]
    fn buffered_prefix_precedes_peer_reset_failure() {
        let reactor = Reactor::new().unwrap();
        let (client, mut server) = pair();
        server.write_all(b"prefix").unwrap();
        SockRef::from(&server)
            .set_linger(Some(Duration::ZERO))
            .unwrap();
        drop(server);
        let mut progress = tcp_progress(client, reactor.handle());

        assert_eq!(progress.admit_read(16), Admission::Admitted);
        match complete_read(&mut progress) {
            ReadOutcome::Data(data) => assert_eq!(data, b"prefix"),
            other => panic!("buffered prefix was not preserved: {other:?}"),
        }
        assert_eq!(progress.admit_read(16), Admission::Admitted);
        match complete_read(&mut progress) {
            ReadOutcome::Failed(failure) => assert!(matches!(
                failure.error.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
            )),
            ReadOutcome::End => panic!("peer reset was normalized to EOF"),
            other => panic!("unexpected reset outcome: {other:?}"),
        }
    }

    #[test]
    fn sustained_backpressure_waits_then_rearms_to_completion() {
        let reactor = Reactor::new().unwrap();
        let (client, mut server) = pair();
        SockRef::from(&client).set_send_buffer_size(4096).unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut progress = tcp_progress(client, reactor.handle());
        let payload = vec![0x5a; MAX_PROGRESS_BYTES];
        let mut completed_bytes = 0usize;
        let mut reached_backpressure = false;

        'fill: for _ in 0..4096 {
            assert!(matches!(
                progress.admit_write(payload.clone()),
                WriteAdmission::Admitted
            ));
            loop {
                let step = progress.advance();
                if matches!(step.write, Some(WriteOutcome::Completed)) {
                    completed_bytes += payload.len();
                    continue 'fill;
                }
                match step.schedule {
                    ProgressSchedule::Runnable => continue,
                    ProgressSchedule::Waiting => {
                        reached_backpressure = true;
                        break 'fill;
                    }
                    ProgressSchedule::Idle => panic!("pending write became idle"),
                }
            }
        }
        assert!(
            reached_backpressure,
            "loopback send buffer never applied pressure"
        );

        let expected_bytes = completed_bytes + payload.len();
        let reader = thread::spawn(move || {
            let mut received = vec![0; expected_bytes];
            server.read_exact(&mut received).unwrap();
            received
        });
        let (root, wake_receiver) = wake_channel();
        'complete: loop {
            let wait = progress.register_wait(root.clone()).unwrap();
            assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
            wait.withdraw().unwrap();
            loop {
                let step = progress.advance();
                if matches!(step.write, Some(WriteOutcome::Completed)) {
                    break 'complete;
                }
                match step.schedule {
                    ProgressSchedule::Runnable => continue,
                    ProgressSchedule::Waiting => break,
                    ProgressSchedule::Idle => panic!("pending write became idle"),
                }
            }
        }
        let received = reader.join().unwrap();
        assert_eq!(received.len(), expected_bytes);
        assert!(received.iter().all(|byte| *byte == 0x5a));
    }

    #[test]
    fn one_reactor_coalesces_many_connection_wakes_without_losing_results() {
        const CONNECTIONS: usize = 32;
        let reactor = Reactor::new().unwrap();
        let mut progresses = Vec::with_capacity(CONNECTIONS);
        let mut servers = Vec::with_capacity(CONNECTIONS);
        for _ in 0..CONNECTIONS {
            let (client, server) = pair();
            let mut progress = tcp_progress(client, reactor.handle());
            assert_eq!(progress.admit_read(8), Admission::Admitted);
            assert_eq!(progress.advance().schedule, ProgressSchedule::Waiting);
            progresses.push(progress);
            servers.push(server);
        }

        let (root, wake_receiver) = wake_channel();
        let mut remaining: Vec<_> = (0..CONNECTIONS).collect();
        let mut waits: Vec<_> = remaining
            .iter()
            .map(|index| progresses[*index].register_wait(root.clone()).unwrap())
            .collect();
        for (index, server) in servers.iter_mut().enumerate() {
            server.write_all(&[index as u8]).unwrap();
        }

        for _ in 0..CONNECTIONS {
            assert_eq!(wait_for_wake(&wake_receiver), WakeAttempt::Item);
            for wait in waits {
                wait.withdraw().unwrap();
            }
            let mut next = Vec::new();
            for index in remaining {
                let step = progresses[index].advance();
                match step.read {
                    Some(ReadOutcome::Data(data)) => assert_eq!(data, [index as u8]),
                    None => {
                        assert_eq!(step.schedule, ProgressSchedule::Waiting);
                        next.push(index);
                    }
                    other => panic!("connection {index} failed: {other:?}"),
                }
            }
            if next.is_empty() {
                remaining = next;
                break;
            }
            waits = next
                .iter()
                .map(|index| progresses[*index].register_wait(root.clone()).unwrap())
                .collect();
            remaining = next;
        }
        assert!(
            remaining.is_empty(),
            "some connections never became readable"
        );
    }

    #[test]
    fn graceful_close_is_terminal() {
        let reactor = Reactor::new().unwrap();
        let (client, _server) = pair();
        let mut progress = tcp_progress(client, reactor.handle());
        assert_eq!(progress.admit_close(), Admission::Admitted);
        let step = progress.advance();
        assert!(matches!(step.close, Some(OperationOutcome::Completed)));
        assert_eq!(
            progress.admit_close(),
            Admission::Rejected(AdmissionReason::Terminal)
        );
    }
}
