//! Observable successors of the checked two-child compiler operation.
use super::*;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct JoinDelivery {
    pub enabled: bool,
    pub pending: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum JoinChildState {
    Entry,
    Running,
    Cleaning,
    AtNormalReturn,
    AtDeliveredExit,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JoinEvent {
    EnterChild { slot: usize, yielded: bool },
    StageResult(usize),
    EndChildView(usize),
    FinishChild(usize),
    RequestStop,
    YieldWhileWaiting,
}

#[derive(Clone, Copy, Debug)]
pub enum JoinNext<'g> {
    Waiting(usize),
    /// Follow the enclosing block's ordinary MIR `Goto`. Each transfer pairs
    /// the actual child export place with the caller's vacant tuple member.
    Publish {
        delivery: JoinDelivery,
        transfers: [(Place<'g>, Place<'g>); 2],
        /// Caller input keys carried into the corresponding tuple members.
        retained_results: [Option<Place<'g>>; 2],
    },
    /// End these initialized, unpublished stages (no disposer), then run the
    /// proof's parent exit. There is no ordinary successor on this edge.
    Cancel {
        retired: [Option<Place<'g>>; 2],
    },
    /// Retire the initialized parent-owned prefix, then run the parent exit.
    CancelBeforeCommit,
}

#[derive(Debug)]
pub struct JoinEntry<'g> {
    pub delivery: JoinDelivery,
    pub yielded: bool,
    pub next: JoinNext<'g>,
}

#[derive(Debug)]
pub struct JoinWait<'g> {
    pub delivery: JoinDelivery,
    pub deferred: bool,
    pub children: [JoinChildState; 2],
    pub stop_requested: [bool; 2],
    pub initialized_results: [bool; 2],
    /// Backing keys the parent preserves throughout the wait, independently
    /// of the live environments and initialized results.
    pub retained_backings: [Place<'g>; 2],
    /// Each live environment carries its input dependency until its storage ends.
    pub environment_backings: [Option<Place<'g>>; 2],
    /// Initialized reference stages carry their own dependencies independently.
    pub retained_results: [Option<Place<'g>>; 2],
    pub successors: Vec<(JoinEvent, JoinNext<'g>)>,
}

/// Body and bindings of a child at one selected scoped call.
#[derive(Debug)]
pub struct ScopedJoinFrame<'g> {
    pub origin: Site<'g>,
    pub invocation: Site<'g>,
    pub body_site: Site<'g>,
    pub environment: Place<'g>,
    pub capture: Place<'g>,
    /// The caller's List owner, or the incoming descriptor's caller-dependency
    /// key. A descriptor key does not denote its local storage or exact referent.
    pub backing: Place<'g>,
    /// The non-owning List proxy. Reference children instead use their capture.
    pub incoming_list_backing: Option<Place<'g>>,
    pub result: Place<'g>,
    pub body: Arc<Mir<'g>>,
}

/// Closed ownership transitions at one selected source call. The parent
/// inherits the caller's delivery context; entry rows cover that context and
/// whether the operation's entry budget check yields. Direct capture formation
/// adds no intermediate checkpoint. The child entries are separate checks.
#[derive(Debug)]
pub struct ScopedJoinProof<'g> {
    pub family: &'g DeclarationId,
    pub implementation: &'g DeclarationId,
    pub bridge: &'g DeclarationId,
    pub frames: [ScopedJoinFrame<'g>; 2],
    pub entries: Vec<JoinEntry<'g>>,
    pub waiting: Vec<JoinWait<'g>>,
    pub before_commit: Vec<Cleanup<'g>>,
    pub parent_exit: CancellationExit<'g>,
}
