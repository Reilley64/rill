//! Target-runtime mechanisms shared by generated Rill programs.
//!
//! This crate is implementation-private. Its Rust API is not Rill's public
//! standard-library ABI. The first implemented slice covers the channel wake
//! and readiness semantics required by `docs/progress-io-contracts.md`.

#![forbid(unsafe_code)]

mod channel;
mod reactor;
mod tcp;

pub use channel::{
    ChannelWatch, ReceiveAttempt, Receiver, SendAttempt, Sender, WakeAttempt, WakeNotifier,
    WakeReceiver, channel, wake_channel, watch_receive_ready, watch_send_ready,
};
pub use reactor::{Reactor, ReactorHandle};
pub use tcp::{
    Admission, AdmissionReason, DriverFailure, DriverFailurePhase, DriverFailureSet, Interruption,
    MAX_PROGRESS_BYTES, OperationOutcome, ProgressSchedule, ProgressStep, ReadFailure, ReadOutcome,
    TcpProgress, TcpProgressWait, WriteAdmission, WriteFailure, WriteOutcome, WriteRejection,
    tcp_progress,
};
