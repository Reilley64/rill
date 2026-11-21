//! Scoped ownership checking and cleanup MIR over a typed capture.
use super::DeclarationFacts;
use super::declarations::Budget;
use super::scalars::{BodyOwner, NodeFactKind, NodeFacts, Site};
use super::typed::{CollectionProvider, CollectionRepresentation, TypedReceipt, TypedReport};
use super::{
    AnalysisLimits, DeclarationShape, InvocationMode, Module, NodeId, QueryError, Statistics, Type,
    TypeTerm, retained_bool_contents_type,
};
use crate::loader::{CollectionArtifact, Declaration, DeclarationId, DeclarationKind};
use crate::source::Span;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Range;
use std::sync::Arc;

mod build;
mod flow;
mod graph;
mod join;
mod scalar;

pub use graph::{
    Delivery, Dependency, DependencyContract, GraphCompletion, GraphObligation, GraphReport,
    GraphRetainingCall, RetainingCallSite, RootReport, analyze_ownership,
};
pub use join::{
    JoinChildState, JoinDelivery, JoinEntry, JoinEvent, JoinNext, JoinWait, ScopedJoinFrame,
    ScopedJoinProof,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    Checked,
    SourceRejected,
    Incomplete,
    ResourceLimit,
}

#[derive(Debug)]
pub struct Finding<'g> {
    pub site: Site<'g>,
    pub related: Site<'g>,
    /// The backedge or suspended call at which an intrinsic exit exposes the store.
    pub cancellation_delivery: Option<Site<'g>>,
    kind: Conflict,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Conflict {
    Moved,
    Access,
    ForbiddenMove,
    Escape,
    CancellationEscape { delivery: NodeId },
    Release,
}

impl Finding<'_> {
    pub fn code(&self) -> &'static str {
        match self.kind {
            Conflict::Moved => "R0200",
            Conflict::Access => "R0201",
            Conflict::ForbiddenMove => "R0501",
            Conflict::Escape | Conflict::CancellationEscape { .. } => "R0202",
            Conflict::Release => "R0203",
        }
    }
}

#[derive(Debug)]
pub struct Report<'g> {
    completion: Completion,
    findings: Vec<Finding<'g>>,
    blocked_at: Option<Site<'g>>,
    mir: Option<Mir<'g>>,
    statistics: Statistics,
}

impl<'g> Report<'g> {
    pub fn completion(&self) -> Completion {
        self.completion
    }
    pub fn findings(&self) -> &[Finding<'g>] {
        &self.findings
    }
    pub fn blocked_at(&self) -> Option<Site<'g>> {
        self.blocked_at
    }
    pub fn mir(&self) -> Option<&Mir<'g>> {
        self.mir.as_ref()
    }
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }
}

#[derive(Debug)]
pub struct Mir<'g> {
    pub entry: usize,
    pub blocks: Vec<Block<'g>>,
    disposer_bodies: BTreeMap<&'g DeclarationId, Mir<'g>>,
    cancellation: Option<Cancellation<'g>>,
    replacement: Option<Box<ReplacementCancellation<'g>>>,
    /// Sole direct call in a checked active frame with no reached budget backedge.
    active_leaf: Option<&'g DeclarationId>,
}

impl<'g> Mir<'g> {
    /// On targeted root MIR, look up a reached active disposer. Graph reports
    /// store these frames separately in [`GraphReport::active_disposers`].
    pub fn disposer(&self, implementation: &DeclarationId) -> Option<&Mir<'g>> {
        self.disposer_bodies.get(implementation)
    }

    /// A checked disposal-through-install interval, not a whole-frame proof.
    pub fn replacement_cancellation(&self) -> Option<&ReplacementCancellation<'g>> {
        self.replacement.as_deref()
    }

    /// Intrinsic cancellation proof for a supported flat frame, including
    /// its suspended state at an admitted retaining call into a second frame.
    /// `None` means unknown, not that cancellation is impossible. `Some` does
    /// not certify enclosing callers, deferred regions, or graph-wide roots.
    pub fn cancellation(&self) -> Option<&Cancellation<'g>> {
        self.cancellation.as_ref()
    }
}

/// A reached initialized replacement under ADR 0009. The two uses of the
/// destination denote different owners: displacement consumes the old value;
/// installation consumes the separate stage and initializes the destination.
#[derive(Debug)]
pub struct ReplacementCancellation<'g> {
    pub displacement: usize,
    pub installation: usize,
    pub staged: Place<'g>,
    pub installed: Place<'g>,
    pub implementation: &'g DeclarationId,
    pub deferred: DeferredDisposerCheckpoint<'g>,
    pub callee: &'g DeclarationId,
    pub entry: Site<'g>,
    pub post_install_call: usize,
    ordinary_suffix: usize,
    installed_cleanup: Vec<Cleanup<'g>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplacementRequest {
    Clear,
    Pending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplacementTrigger {
    UnmaskAfterInstall,
    PostInstallYield,
}

/// Continue in the active disposer; the request survives its suffix and epilogue.
#[derive(Clone, Copy, Debug)]
pub struct DeferredResume {
    pub block: usize,
    pub request: ReplacementRequest,
}

#[derive(Debug)]
pub enum ReplacementSuccessor<'a, 'g> {
    Check(PostInstallCheck<'a, 'g>),
    Continue {
        block: usize,
        request: ReplacementRequest,
    },
    /// Terminal: run installed-owner cleanup under disposal deferral. There
    /// is no ordinary successor or second delivery from this cleanup.
    DisposeInstalledAndExit {
        trigger: ReplacementTrigger,
        cleanup: &'a [Cleanup<'g>],
        deferred: &'a DeferredDisposerCheckpoint<'g>,
    },
}

impl<'g> ReplacementCancellation<'g> {
    fn terminal(&self, trigger: ReplacementTrigger) -> ReplacementSuccessor<'_, 'g> {
        ReplacementSuccessor::DisposeInstalledAndExit {
            trigger,
            cleanup: &self.installed_cleanup,
            deferred: &self.deferred,
        }
    }

    /// After the active disposer returns and installation consumes the stage,
    /// unmasking can deliver a request already pending. Install is not a poll.
    pub fn after_install(
        &self,
        request: ReplacementRequest,
        delivery_enabled: bool,
    ) -> ReplacementSuccessor<'_, 'g> {
        if delivery_enabled && request == ReplacementRequest::Pending {
            self.terminal(ReplacementTrigger::UnmaskAfterInstall)
        } else {
            ReplacementSuccessor::Check(PostInstallCheck {
                interval: self,
                request,
                delivery_enabled,
            })
        }
    }
}

/// Only an installation that did not deliver reaches this check. No intervening
/// operation can change the enclosing mask or clear a pending request.
#[derive(Debug)]
pub struct PostInstallCheck<'a, 'g> {
    interval: &'a ReplacementCancellation<'g>,
    request: ReplacementRequest,
    delivery_enabled: bool,
}

impl<'a, 'g> PostInstallCheck<'a, 'g> {
    pub fn block(&self) -> usize {
        self.interval.post_install_call
    }
    pub fn request(&self) -> ReplacementRequest {
        self.request
    }

    /// The inputs are events at this check, not a replacement mask or a new
    /// value for carried state. A fresh request delivers only at an enabled yield.
    pub fn advance(self, fresh_request: bool, yielded: bool) -> ReplacementSuccessor<'a, 'g> {
        if self.delivery_enabled && fresh_request && yielded {
            self.interval.terminal(ReplacementTrigger::PostInstallYield)
        } else {
            ReplacementSuccessor::Continue {
                block: self.interval.ordinary_suffix,
                request: if fresh_request {
                    ReplacementRequest::Pending
                } else {
                    self.request
                },
            }
        }
    }
}

/// A yield with a request at this callee entry leaves the request pending.
/// There is no lexical exit: the caller keeps its stage and the active frame
/// keeps its live locals and remaining initialized fields as in ordinary MIR.
/// Resume the active body, including its field transfer and epilogue, before
/// continuing at the caller's installation block.
#[derive(Debug)]
pub struct DeferredDisposerCheckpoint<'g> {
    /// Indices in the checked active disposer MIR, not in the caller.
    pub call: usize,
    pub resume: usize,
    pub defer_fields: usize,
    pub operand: Place<'g>,
    pub remainder: Place<'g>,
}

impl DeferredDisposerCheckpoint<'_> {
    pub fn after_masked_yield(&self, request: ReplacementRequest) -> DeferredResume {
        DeferredResume {
            block: self.resume,
            request,
        }
    }
}

#[derive(Debug)]
pub struct Cancellation<'g> {
    /// The budget check before the first ordinary instruction of this frame.
    pub entry_site: Site<'g>,
    pub on_entry_delivery: CancellationExit<'g>,
    successors: BTreeMap<usize, CancellationSuccessor<'g>>,
}

impl<'g> Cancellation<'g> {
    /// The delivery proof associated with a reached ordinary MIR block.
    /// Within a known proof, absence means this block contributes no delivery edge.
    /// A retaining call links checkpoints inside the callee; it is not itself a checkpoint.
    pub fn successor(&self, block: usize) -> Option<&CancellationSuccessor<'g>> {
        self.successors.get(&block)
    }
}

#[derive(Debug)]
pub enum CancellationSuccessor<'g> {
    /// Reached after the iteration's lexical cleanup, before the next test.
    LoopBackedge { on_delivery: CancellationExit<'g> },
    /// Delivery at the identified callee's entry exits the suspended caller,
    /// before any normal callee result is produced. The admitted callee has no
    /// arguments or owned state to clean at entry. This is not a caller poll.
    CalleeEntry {
        callee: &'g DeclarationId,
        entry: Site<'g>,
        on_delivery: CancellationExit<'g>,
    },
    /// Delivery in a nonempty, directly called retaining frame, not at the call instruction.
    RetainingCall(Box<RetainingCallCancellation<'g>>),
}

#[derive(Debug)]
pub struct RetainingCallCancellation<'g> {
    pub callee: &'g DeclarationId,
    /// Formal order. The callee now owns these values; caller cleanup does not end the stages.
    pub arguments: [CancellationArgument<'g>; 3],
    /// Its entry and reached backedges carry the callee's terminal cleanup.
    pub body: Mir<'g>,
    pub destination: Place<'g>,
    /// Conservative backing for caller cleanup at either delivery site, not
    /// the exact contents at a single checkpoint. Excludes the access stage.
    pub possible_backing: Vec<Place<'g>>,
    /// Runs only after the callee checks the slot and cleans its frame, for either delivery site.
    pub caller_cleanup: Vec<Cleanup<'g>>,
}

/// Caller state for bounded delivery inside the selected callee. The callee
/// checks its outward slot and cleans its own frame before this cleanup runs.
#[derive(Debug)]
pub struct SuspendedRetainingFrame<'g> {
    /// Formal order; transferred stages are not caller cleanup operands.
    pub arguments: [CancellationArgument<'g>; 3],
    pub destination: Place<'g>,
    /// Conservative old and compatible input backing, excluding the access stage.
    pub possible_backing: Vec<Place<'g>>,
    pub caller_cleanup: Vec<Cleanup<'g>>,
}

#[derive(Debug)]
pub struct CancellationArgument<'g> {
    pub staged: Place<'g>,
    pub parameter: Place<'g>,
}

/// The terminal successor of an actual budget yield with a pending intrinsic
/// request and delivery enabled. A check without a yield, a yield without a
/// request, or disabled delivery does not take this edge. The ordinary MIR
/// path is separate; this successor neither exports a result nor returns.
#[derive(Debug)]
pub struct CancellationExit<'g> {
    /// Checked before any cleanup. The caller's slot is not a cleanup operand.
    pub retained: Option<RetainedAtDelivery<'g>>,
    pub cleanup: Vec<Cleanup<'g>>,
}

#[derive(Debug)]
pub struct RetainedAtDelivery<'g> {
    pub parameter: Place<'g>,
    /// The last checked outward write; `None` preserves incoming contents.
    pub last_store: Option<Site<'g>>,
}

#[derive(Debug)]
pub struct Block<'g> {
    pub site: Site<'g>,
    pub operation: Operation<'g>,
    pub terminator: Terminator,
}

/// A named or temporary storage root, optionally projected to member storage.
#[derive(Clone, Copy, Debug)]
pub struct Place<'g> {
    pub root: Site<'g>,
    pub projection: Option<Projection<'g>>,
}

#[derive(Clone, Copy, Debug)]
pub enum Projection<'g> {
    Field(&'g Declaration),
    /// A leaf of a two-level owned record, relative to the owned root.
    NestedField {
        outer: &'g Declaration,
        inner: &'g Declaration,
    },
    TupleElement(usize),
    /// Two tuple positions relative to an owned root or a descriptor's referent.
    NestedTupleElement {
        outer: usize,
        inner: usize,
    },
    /// An owned position of the exact statically known variant, not a tag test.
    VariantPayload {
        variant: &'g DeclarationId,
        index: usize,
    },
}

impl<'g> Place<'g> {
    pub fn field(self) -> Option<&'g Declaration> {
        match self.projection {
            Some(Projection::Field(field)) => Some(field),
            _ => None,
        }
    }

    pub fn tuple_element(self) -> Option<usize> {
        match self.projection {
            Some(Projection::TupleElement(position)) => Some(position),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum Operation<'g> {
    None,
    Initialize(Place<'g>),
    Copy {
        from: Place<'g>,
        into: Option<Place<'g>>,
    },
    Move {
        from: Place<'g>,
        into: Option<Place<'g>>,
    },
    /// Move the initialized string stages in written order into a new backing
    /// using the linked `List::from_elements`. No partial List is published.
    CommitListText {
        elements: Vec<Place<'g>>,
        into: Place<'g>,
        backing: CollectionRepresentation<'g>,
        provider: CollectionArtifact<'g>,
    },
    /// Consume the List and initialize the sole owner of its unyielded slots.
    OpenOwnedListI32 {
        source: Place<'g>,
        cursor: Place<'g>,
        backing: CollectionRepresentation<'g>,
        provider: CollectionProvider<'g>,
    },
    /// Call the selected `next` once. The yes edge takes `Some`'s i32 into
    /// the binder (or discards it); the no edge sees `None` and an empty cursor.
    AdvanceOwnedListI32 {
        cursor: Place<'g>,
        into: Option<Place<'g>>,
        some: &'g DeclarationId,
        none: &'g DeclarationId,
        provider: CollectionProvider<'g>,
    },
    /// Transfer the installed List<string> backing into the sole suffix owner.
    OpenOwnedListText {
        source: Place<'g>,
        cursor: Place<'g>,
        source_backing: CollectionRepresentation<'g>,
        cursor_backing: CollectionRepresentation<'g>,
        provider: CollectionProvider<'g>,
    },
    /// Remove one string from the cursor on Some and initialize `into` alone.
    /// On None the cursor is empty and `into` stays vacant.
    AdvanceOwnedListText {
        cursor: Place<'g>,
        into: Place<'g>,
        some: &'g DeclarationId,
        none: &'g DeclarationId,
        provider: CollectionProvider<'g>,
    },
    Borrow {
        source: Place<'g>,
        holder: Place<'g>,
        exclusive: bool,
    },
    Reborrow {
        parent: Place<'g>,
        holder: Place<'g>,
        exclusive: bool,
    },
    Dereference(Place<'g>),
    /// Read an i32 through the checked view using the linked `List::index` action.
    /// An exclusive view here belongs to a transferred child environment;
    /// reading does not copy the descriptor or create another loan.
    /// The full u64 is checked against the backing at evaluation; out of
    /// bounds is fatal. Only success initializes `into` and reaches the
    /// normal successor.
    ReadListI32 {
        view: Place<'g>,
        backing: Place<'g>,
        exclusive: bool,
        index: u64,
        into: Option<Place<'g>>,
        owner: &'g DeclarationId,
        representation: CollectionArtifact<'g>,
        provider: CollectionArtifact<'g>,
    },
    /// Compare the initialized i32 scratch value with the written literal,
    /// ending the scratch and initializing the bool result.
    EqualI32Literal {
        from: Place<'g>,
        literal: i32,
        into: Place<'g>,
    },
    /// Copy a scalar through a checked descriptor into its destination.
    CopyReferent {
        from: Place<'g>,
        into: Place<'g>,
    },
    /// Bind a fixed projection relative to a borrow's referent.
    /// No mode moves a member out of the aggregate.
    BindBorrowedMember {
        descriptor: Place<'g>,
        projection: Projection<'g>,
        into: Place<'g>,
        binding: BorrowedMemberBinding,
    },
    FormClosure {
        closure: Site<'g>,
        capture: Site<'g>,
        mode: InvocationMode,
    },
    /// Form a pair environment from the two already-staged captures, in order.
    FormClosurePair {
        closure: Site<'g>,
        captures: [Site<'g>; 2],
        mode: InvocationMode,
    },
    /// Copy the initialized fields in selection order into distinct owned
    /// storage. Scalar fields use the checked scalar copy; text fields execute
    /// CopyText and acquire their own DropCapture obligation. No invocation.
    CopyClosure {
        from: Place<'g>,
        into: Place<'g>,
        origin: Site<'g>,
        captures: Vec<CaptureCopyEffect<'g>>,
    },
    Call(&'g DeclarationId),
    /// Move a staged pair owner through a closed ordinary specialization, not
    /// an erased invocation or a proof for the callee's open source root.
    CallCallableTransfer {
        callee: &'g DeclarationId,
        argument: Place<'g>,
        into: Place<'g>,
        layout: CallableLayout,
        body: Arc<Mir<'g>>,
    },
    /// Invoke the selected ordinary application body, not a joined child
    /// operation. Both transferred environments end on the normal edge.
    CallListArrows {
        implementation: &'g DeclarationId,
        body: Site<'g>,
        slots: Box<[ListArrowFrame<'g>; 2]>,
    },
    /// Checked commit, wait, and cleanup successors, not an ordinary call.
    ScopedJoin(Box<ScopedJoinProof<'g>>),
    /// Call the selected scalar copy body, ending only the argument stage.
    CallScalarCopy {
        implementation: &'g DeclarationId,
        argument: Place<'g>,
        into: Option<Place<'g>>,
    },
    /// Consume a named string through the selected, linked element disposer.
    DisposeText {
        operand: Place<'g>,
        element: TextElementDisposal<'g>,
    },
    /// Transfer the original named operand to the selected scalar disposer.
    /// The callee ends its parameter storage; there is no argument copy.
    CallScalarDispose {
        implementation: &'g DeclarationId,
        operand: Place<'g>,
    },
    /// Call with a staged shared record descriptor. The result may retain the
    /// whole record access and the sources carried by its contents.
    CallBorrowingRecord {
        callee: &'g DeclarationId,
        argument: Place<'g>,
        into: Place<'g>,
    },
    /// Evaluate a direct producer into the statement's owned backing storage.
    CallIntoTemporary {
        callee: &'g DeclarationId,
        arguments: Vec<Place<'g>>,
        into: Place<'g>,
    },
    /// Consume a known once environment and transfer its two captured values
    /// into the corresponding result tuple positions.
    InvokeClosurePair {
        receiver: Place<'g>,
        origin: Site<'g>,
        into: Place<'g>,
    },
    /// Copy both values from a known pair environment into a fresh result,
    /// in the order established by FormClosurePair. Descriptors retain their
    /// carried sources without borrowing the receiver's storage. The
    /// environment remains initialized.
    InvokeReadOnlyClosurePair {
        receiver: Place<'g>,
        origin: Site<'g>,
        into: Place<'g>,
    },
    /// Exclusively access the receiver for this operation. Update its sole
    /// owned capture with checked i32 addition, then copy the updated cell
    /// into the result. Overflow panics before the store. `field` identifies
    /// the capture stage of the forming arrow, not the outer scalar's storage.
    InvokeMutableI32Cell {
        receiver: Place<'g>,
        origin: Site<'g>,
        field: Site<'g>,
        addend: i32,
        into: Place<'g>,
    },
    /// A direct call may retain an input's backing in the destination's
    /// contents, including the exclusive restriction on a shared view.
    CallRetaining {
        callee: &'g DeclarationId,
        destination: RetainedPlace<'g>,
        input: Place<'g>,
    },
    /// A direct call can retain either of two compatible Bool inputs.
    /// Input stages are listed in parameter order, omitting the destination.
    CallRetainingPair {
        callee: &'g DeclarationId,
        destination: RetainedPlace<'g>,
        inputs: [Place<'g>; 2],
    },
    /// `arguments` lists already-staged operands in formal parameter order.
    /// Only compatible reference inputs contribute backing; the Bool stage carries none.
    CallRetainingWithBool {
        callee: &'g DeclarationId,
        destination: RetainedPlace<'g>,
        arguments: Vec<Place<'g>>,
        input: Place<'g>,
        second_input: Option<Place<'g>>,
    },
    /// A direct call with staged `(destination, seed, gate)` in formal order.
    /// Only `seed` can add backing to the destination. Neither the canonical
    /// `Option<bool>` gate nor the result carries loans.
    CallRetainingWithOptionBool {
        callee: &'g DeclarationId,
        destination: RetainedPlace<'g>,
        arguments: [Place<'g>; 3],
        into: Place<'g>,
    },
    /// Select the success variant of a staged propagation or wrapping operand.
    TestVariant {
        operand: Place<'g>,
        success: &'g DeclarationId,
    },
    CopyOptionPayload {
        operand: Place<'g>,
        into: Place<'g>,
    },
    /// Consume a proved Some carrier and transfer its payload into a slot.
    TakeOptionPayload {
        operand: Place<'g>,
        into: Place<'g>,
    },
    /// Consume a proved None carrier without dropping a payload.
    EndEmptyOption(Place<'g>),
    ConstructVariant {
        variant: &'g DeclarationId,
        payload: Option<Place<'g>>,
        into: Place<'g>,
    },
    /// Consume the payload into a fresh, cause-free `Result.Err` in `into`.
    /// Its origin is this block's source snapshot and `site.span.start()`.
    ConstructResultError {
        constructor: &'g DeclarationId,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        payload: Place<'g>,
        into: Place<'g>,
    },
    /// Consume a proved Ok operand after transferring or discarding its payload.
    TakeResultSuccess {
        operand: Place<'g>,
        into: Option<Place<'g>>,
    },
    /// Move the existing Err wrapper, including its origin and retained cause.
    ForwardResultFailure {
        operand: Place<'g>,
        into: Place<'g>,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
    },
    /// Retire a proved failed stage on an interrupted wrapping RHS. Consume
    /// its installed `OwnedErr<i32>` (including origin and retained chain)
    /// using the pinned storage's drop, with no wrapper-level dispatch or rendering.
    DropResultFailure {
        operand: Place<'g>,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        storage_source: &'g std::path::Path,
    },
    /// Consume the staged failed Result and outer payload into a new
    /// `Result.Err` in `into`. The cause keeps the inner origin and chain;
    /// only the outer wrapper gets `origin`. `cause_display` names the
    /// retained renderer, not a call during wrapping. The inner i32 uses
    /// scalar cleanup.
    WrapResultFailure {
        operand: Place<'g>,
        payload: Place<'g>,
        into: Place<'g>,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        origin: &'g Span,
        cause_display: &'g DeclarationId,
    },
    Dispose {
        callee: &'g DeclarationId,
        operand: Place<'g>,
    },
    Install {
        target: Place<'g>,
        incoming: Place<'g>,
    },
    /// Copy the shared reference stored through the exclusive descriptor.
    CopyThrough {
        descriptor: Place<'g>,
        into: Option<Place<'g>>,
    },
    /// Replace the contents of an exact local or caller destination through
    /// its initialized exclusive access descriptor.
    InstallThrough {
        descriptor: Place<'g>,
        incoming: Place<'g>,
    },
    /// Install at `**parameter`, leaving the exclusive descriptor at
    /// `*parameter` in place.
    InstallThroughTwoHop {
        parameter: Place<'g>,
        incoming: Place<'g>,
    },
    Export(Place<'g>),
    Drop(Place<'g>),
    DropCapture {
        closure: Site<'g>,
        field: Site<'g>,
    },
    Cleanup(Vec<Cleanup<'g>>),
}

/// Entry and return bindings for one normal arrow frame. The transferred
/// environment supplies `capture`; its `EndStorage` ends that loan. `backing`
/// stays in the caller; `incoming_backing` is its non-owning name in `body`.
#[derive(Debug)]
pub struct ListArrowFrame<'g> {
    pub origin: Site<'g>,
    pub invocation: Site<'g>,
    pub body_site: Site<'g>,
    pub environment: Place<'g>,
    pub capture: Place<'g>,
    pub backing: Place<'g>,
    pub incoming_backing: Place<'g>,
    pub result: Place<'g>,
    pub body: Arc<Mir<'g>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorrowedMemberBinding {
    /// Copy a primitive value with no carried backing.
    CopyScalar,
    /// Copy a stored shared reference with carried dependencies, not the
    /// loan used to access the enclosing aggregate.
    CopyReference,
    /// Borrow the stored reference's caller-owned slot without copying it.
    SharedSlot,
    /// Retain the original backing for a shared view of the member.
    SharedView,
    /// Consume the fresh staged descriptor and transfer its original
    /// exclusive restriction to the sole writable member view.
    ExclusiveView,
}

#[derive(Clone, Copy, Debug)]
pub enum RetainedPlace<'g> {
    Local(Place<'g>),
    ThroughParameter(Place<'g>),
    /// The final contents at `**parameter`, passed via the staged exclusive
    /// reborrow of `parameter`. The descriptor at `*parameter` stays in place.
    ThroughTwoHopParameter {
        parameter: Place<'g>,
        argument: Place<'g>,
    },
    ThroughAlias {
        descriptor: Place<'g>,
        target: Place<'g>,
    },
    /// A local binding holds the exclusive access moved from this parameter.
    ThroughParameterAlias {
        descriptor: Place<'g>,
        parameter: Place<'g>,
    },
}

/// Capture storage in a checked pair, in formation order. This describes
/// ordinary ownership and cleanup, not a runtime layout or invocation ABI.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CallableLayout(pub [CallableField; 2]);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CallableField {
    OwnedPrimitive(&'static str),
    SharedPrimitive(&'static str),
    ExclusiveBool,
    OwnedString,
}

/// Effect on one initialized field of the source environment, not a call
/// into the closure body or a bytewise copy of its representation.
#[derive(Clone, Copy, Debug)]
pub enum CaptureCopyEffect<'g> {
    Scalar {
        field: Site<'g>,
        implementation: &'g DeclarationId,
        primitive: super::typed::Primitive,
    },
    Text {
        field: Site<'g>,
        copy: super::typed::PrimitiveOperation,
        dispose: super::typed::PrimitiveOperation,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Cleanup<'g> {
    EndStorage(Place<'g>),
    Drop(Place<'g>),
    /// Consume an owned `Option<string>`, dropping its string only in `some`.
    DropOptionPayload {
        operand: Place<'g>,
        some: &'g DeclarationId,
    },
    DropCapture {
        closure: Site<'g>,
        field: Site<'g>,
    },
    /// Consume the environment in reverse capture order, dropping each
    /// owned string and ending primitive and descriptor storage without access
    /// to the descriptor's referent.
    ConsumeCallableTransfer {
        operand: Place<'g>,
        layout: CallableLayout,
    },
    InvokeDisposer {
        operand: Place<'g>,
        implementation: &'g DeclarationId,
    },
    /// Consume the checked LockGuard<i32> through its protected release.
    ReleaseGuard {
        operand: Place<'g>,
        implementation: &'g DeclarationId,
        entry: &'g DeclarationId,
    },
    /// Consume the canonical List<i32> backing. Its scalar elements have no drop action.
    DisposeListI32 {
        operand: Place<'g>,
        implementation: &'g DeclarationId,
        bridge: &'g DeclarationId,
    },
    /// Retire only the remaining backing. The checked i32 element disposer
    /// has no effect, including when `next` has already observed exhaustion.
    DisposeOwnedListIterI32 {
        cursor: Place<'g>,
        implementation: &'g DeclarationId,
        bridge: &'g DeclarationId,
    },
    /// Consume a whole canonical List<string>, releasing elements in reverse order.
    DisposeListText {
        operand: Place<'g>,
        implementation: &'g DeclarationId,
        bridge: &'g DeclarationId,
        element: TextElementDisposal<'g>,
    },
    /// Consume only the unyielded suffix, invoking `element` in reverse position
    /// order. After None the bridge retires the empty backing with no element calls.
    DisposeOwnedListIterText {
        cursor: Place<'g>,
        implementation: &'g DeclarationId,
        bridge: &'g DeclarationId,
        element: TextElementDisposal<'g>,
    },
    /// Transfer the initialized remainder to the active disposer's epilogue.
    DeferDisposerFields {
        from: Place<'g>,
        into: Place<'g>,
    },
}

/// The protected primitive string dispose winner and its DisposeText binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextElementDisposal<'g> {
    pub implementation: &'g DeclarationId,
    pub bridge: &'g DeclarationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Terminator {
    Goto(usize),
    Branch {
        yes: usize,
        no: usize,
    },
    Return,
    /// End the module's lexical startup frame; there is no callable result.
    FinishStartup,
    /// Export the staged Bool after expression cleanup, without a callable return.
    FinishExpectation,
}

/// Lower a single root and validate its reached dependencies in this capture.
/// `Checked` certifies only this scoped judgment, not whole-capture ownership
/// or execution. Non-function roots do not carry a cancellation proof.
pub fn lower_root<'g>(
    receipt: &TypedReceipt<'_, 'g>,
    module: &'g Module,
    owner: BodyOwner<'_>,
    limits: AnalysisLimits,
) -> Result<Report<'g>, QueryError> {
    let typed = receipt.report;
    validate_request(typed, module, owner)?;
    let mut budget = Budget {
        limits,
        statistics: Statistics::default(),
    };
    // Capture identity is constant-time and precedes resource errors. Charge
    // valid root resolution, including an upper bound on binary-search comparisons.
    let lookup_work = match owner {
        BodyOwner::Startup => 1,
        BodyOwner::Declaration(id) => match id.kind {
            DeclarationKind::Function => 2,
            DeclarationKind::Implementation | DeclarationKind::Expectation => {
                3 + module.declarations.len().max(1).ilog2() as usize
            }
            _ => return Err(QueryError::InvalidRequest),
        },
    };
    if budget.step(lookup_work).is_err() {
        return Ok(report(Err((module, Stop::Limit)), budget.statistics));
    }
    let plan = match owner {
        BodyOwner::Startup => RootPlan::Startup,
        BodyOwner::Declaration(id) if id.kind == DeclarationKind::Function => {
            let header = typed
                .inventory()
                .type_facts()
                .declarations
                .get(id)
                .ok_or(QueryError::InvalidRequest)?;
            RootPlan::Function(header)
        }
        BodyOwner::Declaration(id)
            if matches!(
                id.kind,
                DeclarationKind::Implementation | DeclarationKind::Expectation
            ) =>
        {
            let index = module
                .declarations
                .binary_search_by(|candidate| candidate.id.cmp(id))
                .map_err(|_| QueryError::InvalidRequest)?;
            let declaration = &module.declarations[index];
            if id.kind == DeclarationKind::Implementation {
                RootPlan::Implementation(declaration)
            } else {
                RootPlan::Expectation(declaration)
            }
        }
        BodyOwner::Declaration(_) => return Err(QueryError::InvalidRequest),
    };
    Ok(check_root(typed, module, plan, budget))
}

// Reject foreign capture handles even at zero budget, without searching either table.
fn validate_request(
    typed: &TypedReport<'_>,
    module: &Module,
    owner: BodyOwner<'_>,
) -> Result<(), QueryError> {
    let graph = typed.inventory().graph();
    if let BodyOwner::Declaration(id) = owner
        && (id.context != graph.id() || id.module != module.key)
    {
        return Err(QueryError::InvalidRequest);
    }
    if !graph.owns_module(module) {
        return Err(QueryError::InvalidRequest);
    }
    Ok(())
}

/// Lower the selected body after validating reached direct calls and disposers.
/// `Checked` certifies ordinary MIR; see `Mir::cancellation` for the separate,
/// bounded intrinsic-exit proof. Neither is a whole-capture receipt or execution.
pub fn lower_body<'g>(
    receipt: &TypedReceipt<'_, 'g>,
    module: &'g Module,
    declaration: &DeclarationId,
    limits: AnalysisLimits,
) -> Result<Report<'g>, QueryError> {
    if declaration.kind != DeclarationKind::Function {
        return Err(QueryError::InvalidRequest);
    }
    let typed = receipt.report;
    validate_request(typed, module, BodyOwner::Declaration(declaration))?;
    // Preserve the original function query's budget boundary: its typed header
    // is a request premise, not a root-discovery operation.
    let header = typed
        .inventory()
        .type_facts()
        .declarations
        .get(declaration)
        .ok_or(QueryError::InvalidRequest)?;
    Ok(check_root(
        typed,
        module,
        RootPlan::Function(header),
        Budget {
            limits,
            statistics: Statistics::default(),
        },
    ))
}

fn check_root<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    plan: RootPlan<'_, 'g>,
    mut budget: Budget,
) -> Report<'g> {
    let mut result = checked_graph(typed, module, plan, &mut budget);
    if let Err((at, Stop::Rejected { .. })) = result
        && budget.reserve(1).is_err()
    {
        result = Err((at, Stop::Limit));
    }
    report(result, budget.statistics)
}

fn report<'g>(result: Result<Mir<'g>, (&'g Module, Stop)>, statistics: Statistics) -> Report<'g> {
    let (completion, findings, blocked_at, mir) = match result {
        Ok(mir) => (Completion::Checked, Vec::new(), None, Some(mir)),
        Err((
            module,
            Stop::Rejected {
                site,
                related,
                kind,
            },
        )) => (
            Completion::SourceRejected,
            vec![finding(module, site, related, kind)],
            None,
            None,
        ),
        Err((module, Stop::Pending(node))) => (
            Completion::Incomplete,
            Vec::new(),
            Some(site_at(module, node)),
            None,
        ),
        Err((_, Stop::Limit)) => (Completion::ResourceLimit, Vec::new(), None, None),
    };
    Report {
        completion,
        findings,
        blocked_at,
        mir,
        statistics,
    }
}

fn finding<'g>(module: &'g Module, site: NodeId, related: NodeId, kind: Conflict) -> Finding<'g> {
    Finding {
        site: site_at(module, site),
        related: site_at(module, related),
        cancellation_delivery: match kind {
            Conflict::CancellationEscape { delivery } => Some(site_at(module, delivery)),
            _ => None,
        },
        kind,
    }
}

enum RootPlan<'a, 'g> {
    Function(&'a DeclarationFacts<'g>),
    Implementation(&'g Declaration),
    Expectation(&'g Declaration),
    Startup,
}

// Prove the declared dependency contracts of every direct callee reached by
// the selected body. Recursion uses those declarations provisionally; no MIR
// escapes until the worklist has validated each reached body.
fn checked_graph<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    plan: RootPlan<'_, 'g>,
    budget: &mut Budget,
) -> Result<Mir<'g>, (&'g Module, Stop)> {
    let mut scalar_module = None;
    let mut reached = |mir: &Mir<'g>,
                       pending: &mut VecDeque<(&'g Module, &'g DeclarationId, bool)>,
                       seen: &mut BTreeMap<&'g DeclarationId, Option<Site<'g>>>,
                       budget: &mut Budget|
     -> ResultOf<()> {
        graph::visit_dependencies(mir, budget, |_, edge, budget| {
            let is_disposer = match edge.contract {
                DependencyContract::OrdinaryCall => false,
                DependencyContract::ActiveDisposer => true,
                // Preserve the targeted query's narrower dependency contract.
                _ => return Ok(()),
            };
            let callee = edge.declaration;
            if seen.contains_key(callee) {
                return Ok(());
            }
            let scalar = !is_disposer && callee.kind == DeclarationKind::Implementation;
            let modules = match scalar_module.filter(|_| scalar) {
                // Every admitted ordinary scalar implementation is in the
                // same captured module, regardless of family or width.
                Some(module) => std::slice::from_ref(module),
                None => typed.inventory().graph().modules(),
            };
            let mut target = None;
            for candidate in modules {
                budget.step(1)?;
                if candidate.key == callee.module {
                    target = Some(candidate);
                    break;
                }
            }
            let target = target.ok_or(Stop::Pending(edge.site.node))?;
            if scalar {
                scalar_module = Some(target);
            }
            budget.reserve(2)?;
            seen.insert(callee, None);
            pending.push_back((target, callee, is_disposer));
            Ok(())
        })
    };

    let mut seen = BTreeMap::new();
    let mut pending = VecDeque::new();
    let mut cache = QueryCache::default();
    let lowering = Lowering::new(budget, &mut cache);
    let (root_site, lowered) = match plan {
        RootPlan::Function(header) => {
            lowering
                .budget
                .reserve(1)
                .map_err(|()| (module, Stop::Limit))?;
            seen.insert(&header.declaration.id, None);
            (
                header.declaration.node,
                lower(
                    typed,
                    module,
                    header,
                    Frame::OrdinaryFunction,
                    true,
                    None,
                    lowering,
                ),
            )
        }
        RootPlan::Implementation(declaration) => (
            declaration.node,
            lower_implementation(typed, module, declaration, lowering),
        ),
        RootPlan::Expectation(declaration) => (
            declaration.node,
            lower_expectation(typed, module, declaration, lowering),
        ),
        RootPlan::Startup => (module.syntax.root, lower_startup(typed, module, lowering)),
    };
    let Lowered {
        mir: mut root,
        trace,
        replacement,
        ..
    } = lowered.map_err(|stop| (module, stop))?;
    reached(&root, &mut pending, &mut seen, budget).map_err(|stop| (module, stop))?;
    let mut retaining = match &trace {
        Some(trace) => trace
            .prepare_retaining_call(&root, budget)
            .map_err(|stop| (module, stop))?,
        None => None,
    };
    // All direct dependencies of the root are queued before transitive ones.
    let mut direct = pending.len();
    let mut scalar_targets = None;
    let mut replacement_entry = None;
    let mut matching_active = false;
    let mut known_entry = false;
    let mut unknown_entry = false;
    while let Some((target, id, is_disposer)) = pending.pop_front() {
        budget.step(1).map_err(|()| (target, Stop::Limit))?;
        let missing = (module, Stop::Pending(root_site));
        let enabled = !is_disposer && retaining.as_ref().is_some_and(|call| call.callee == id);
        let lowered = if !is_disposer && id.kind == DeclarationKind::Implementation {
            // Only selected scalar edges target ordinary implementations here.
            // All widths share the same core module and capture index.
            let index = match &mut scalar_targets {
                Some(index) => index,
                None => scalar_targets.insert(
                    scalar::TargetIndex::new(typed, target, target.syntax.root, budget)
                        .map_err(|stop| (target, stop))?,
                ),
            };
            index
                .lower_selected(typed, target, id, target.syntax.root, budget)
                .map(|mir| Lowered {
                    mir,
                    trace: None,
                    replacement: None,
                    empty_entry: None,
                })
        } else {
            let (selected, frame) = if is_disposer {
                let family = typed
                    .dispose_role()
                    .and_then(|role| typed.inventory().captured_family(&role.id))
                    .ok_or((module, Stop::Pending(root_site)))?;
                budget
                    .step(family.implementations.len())
                    .map_err(|()| (target, Stop::Limit))?;
                let selected = family
                    .implementations
                    .iter()
                    .find(|body| body.source.declaration == *id)
                    .map(|body| &body.header);
                (selected, Frame::ActiveDisposer)
            } else {
                (
                    typed.inventory().type_facts().declarations.get(id),
                    Frame::OrdinaryFunction,
                )
            };
            lower(
                typed,
                target,
                selected.ok_or(missing)?,
                frame,
                enabled,
                None,
                Lowering::new(budget, &mut cache),
            )
        };
        let Lowered {
            mir,
            empty_entry,
            trace: callee_trace,
            ..
        } = lowered.map_err(|stop| (target, stop))?;
        if let Some(candidate) = &replacement {
            if std::ptr::eq(id, candidate.site.callee) {
                replacement_entry = empty_entry;
            }
            if is_disposer && std::ptr::eq(id, candidate.site.implementation) {
                matching_active = mir
                    .active_leaf
                    .is_some_and(|leaf| std::ptr::eq(leaf, candidate.site.callee));
            }
        }
        if direct != 0 {
            direct -= 1;
            known_entry |= empty_entry.is_some();
            unknown_entry |= empty_entry.is_none();
            if let Some(entry) = empty_entry {
                *seen.get_mut(id).unwrap() = Some(entry);
            }
        }
        reached(&mir, &mut pending, &mut seen, budget).map_err(|stop| (target, stop))?;
        if let Some(call) = retaining.take_if(|_| enabled) {
            if let Some(callee_trace) = callee_trace {
                root.cancellation = call.finish(callee_trace, mir, budget)?;
            }
        } else if is_disposer {
            budget.reserve(1).map_err(|()| (target, Stop::Limit))?;
            root.disposer_bodies.insert(id, mir);
        }
    }
    if let Some(trace) = trace.filter(|_| root.cancellation.is_none()) {
        root.cancellation = trace
            .finish(
                &root,
                |_, callee, _| Ok(seen.get(callee).copied().flatten()),
                known_entry,
                unknown_entry,
                budget,
            )
            .map_err(|stop| (module, stop))?;
    }
    if matching_active && let (Some(candidate), Some(entry)) = (replacement, replacement_entry) {
        // The reached direct frames established both entry identities before
        // any replacement-only scan, lookup or cleanup allocation.
        graph::charge_key(
            &candidate.site.implementation.module,
            root.disposer_bodies.len(),
            budget,
        )
        .map_err(|()| (module, Stop::Limit))?;
        if let Some(body) = root.disposer_bodies.get(candidate.site.implementation) {
            root.replacement = candidate
                .finish(&root, body, entry, budget)
                .map_err(|stop| (module, stop))?;
        }
    }
    Ok(root)
}

struct Lowered<'a, 'g> {
    mir: Mir<'g>,
    trace: Option<flow::CancellationTrace<'a, 'g>>,
    replacement: Option<flow::ReplacementCandidate<'g>>,
    empty_entry: Option<Site<'g>>,
}

#[derive(Default)]
struct QueryCache<'a, 'g> {
    target_nodes: BTreeMap<*const Module, Arc<NodeIndex<'a, 'g>>>,
    // Canonical references belong to this receipt. Address keys avoid path
    // comparisons, and identical source offsets in other modules cannot alias.
    callable_proofs: BTreeMap<(*const DeclarationId, CallableLayout), ResultOf<Arc<Mir<'g>>>>,
    #[cfg(all(test, target_arch = "aarch64", target_os = "macos"))]
    join_probe: Option<flow::scoped_join::tests::Probe<'g>>,
}

struct Lowering<'q, 'a, 'g> {
    budget: &'q mut Budget,
    cache: Option<&'q mut QueryCache<'a, 'g>>,
}

impl<'q, 'a, 'g> Lowering<'q, 'a, 'g> {
    fn new(budget: &'q mut Budget, cache: &'q mut QueryCache<'a, 'g>) -> Self {
        Self {
            budget,
            cache: Some(cache),
        }
    }
}

impl<'a, 'g> QueryCache<'a, 'g> {
    fn target_nodes(
        &self,
        module: &Module,
        budget: &mut Budget,
    ) -> ResultOf<Option<Arc<NodeIndex<'a, 'g>>>> {
        if self.target_nodes.is_empty() {
            return Ok(None);
        }
        budget.step(1)?;
        Ok(self.target_nodes.get(&std::ptr::from_ref(module)).cloned())
    }

    fn share_target_nodes(
        &mut self,
        module: &Module,
        facts: &mut BuildFacts<'a, 'g>,
        budget: &mut Budget,
    ) -> ResultOf<()> {
        if let BuildFacts::Local(nodes) = facts {
            // Keep the already-metered scan only once a transfer is reached.
            // Indexed graph/scalar builds and ordinary-only requests do no extra indexing.
            budget.step(1)?;
            budget.reserve(2)?;
            let nodes = Arc::new(std::mem::take(nodes));
            self.target_nodes
                .insert(std::ptr::from_ref(module), Arc::clone(&nodes));
            *facts = BuildFacts::Shared(nodes);
        }
        Ok(())
    }

    fn callable_transfer(
        &mut self,
        typed: &TypedReport<'g>,
        module: &'g Module,
        header: &DeclarationFacts<'g>,
        layout: CallableLayout,
        nodes: &NodeIndex<'_, 'g>,
        budget: &mut Budget,
    ) -> ResultOf<Arc<Mir<'g>>> {
        let key = (std::ptr::from_ref(&header.declaration.id), layout);
        budget.step(1)?;
        if let Some(proof) = self.callable_proofs.get(&key) {
            return proof.clone();
        }
        // Transfer frames cannot lower another callable transfer. Only a
        // finished check (including closure of external edges) is reusable.
        let proof = lower(
            typed,
            module,
            header,
            Frame::CallableTransfer(layout),
            false,
            Some(nodes),
            Lowering {
                budget,
                cache: None,
            },
        )
        .and_then(|lowered| {
            graph::visit_dependencies(&lowered.mir, budget, |_, edge, _| {
                Err(Stop::Pending(edge.site.node))
            })?;
            Ok(lowered.mir)
        });
        if matches!(proof, Err(Stop::Limit)) {
            return Err(Stop::Limit);
        }
        budget.step(1)?;
        budget.reserve(1 + usize::from(proof.is_ok()))?;
        let proof = proof.map(Arc::new);
        self.callable_proofs.insert(key, proof.clone());
        proof
    }
}

enum Frame {
    OrdinaryFunction,
    CallableTransfer(CallableLayout),
    PlainImplementation { body: NodeId },
    ScalarCopy { body: NodeId },
    ActiveDisposer,
}

fn lower<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    header: &'a DeclarationFacts<'g>,
    frame: Frame,
    collect_trace: bool,
    facts: Option<&'a NodeIndex<'a, 'g>>,
    Lowering { budget, cache }: Lowering<'_, 'a, 'g>,
) -> ResultOf<Lowered<'a, 'g>> {
    let entry_site = header.declaration.node;
    let DeclarationShape::Function { parameters, result } = &header.shape else {
        return Err(Stop::Pending(entry_site));
    };
    let body = match frame {
        Frame::PlainImplementation { body } | Frame::ScalarCopy { body } => body,
        _ => *module.syntax.nodes[entry_site]
            .children
            .get(1)
            .ok_or(Stop::Pending(entry_site))?,
    };
    let mut retains_exclusive_bool = false;
    if matches!(parameters.len(), 2..=4) {
        budget.step(parameters.len())?;
        if let Some(destination) = parameters
            .iter()
            .position(|parameter| retained_bool_contents_type(&parameter.ty).is_some())
        {
            retains_exclusive_bool =
                retained_bool_contract(parameters, result, destination, budget)?
                    .is_some_and(|contract| contract.exclusive_source);
        }
    }
    let two_hop = retained_two_hop_bool_contract(header);
    let mut build = Build::new(
        typed,
        module,
        body,
        Some((&header.declaration.id, result)),
        facts,
        cache.as_deref(),
        budget,
    )?;
    if matches!(frame, Frame::ScalarCopy { .. }) {
        let [formal] = parameters.as_slice() else {
            return Err(Stop::Pending(entry_site));
        };
        build.check_scalar_copy_body(formal, result)?;
    }
    build.retains_exclusive_bool = retains_exclusive_bool;
    build.budget.reserve(parameters.len())?;
    let mut params = Vec::with_capacity(parameters.len());
    let mut owns_guard = false;
    for parameter in parameters {
        if matches!(parameter.ty.as_ref(), TypeTerm::Callable { .. })
            && !matches!(frame, Frame::CallableTransfer(_))
        {
            return Err(Stop::Pending(parameter.declaration.node));
        }
        let slot = build.parameter(parameter)?;
        params.push(slot);
        owns_guard |= build.slots[slot].contains_guard();
        if let Some(ty) = retained_bool_contents_type(&parameter.ty) {
            let contents = build.slot(parameter.declaration.node, ty)?;
            build.budget.reserve(1)?;
            build
                .retained
                .insert(slot, RetainedContents::OneHop(contents));
        } else if let Some((destination, ty)) = two_hop
            && destination.declaration.id == parameter.declaration.id
        {
            let contents = build.slot(parameter.declaration.node, ty)?;
            build.budget.reserve(1)?;
            build
                .retained
                .insert(slot, RetainedContents::TwoHop(contents));
        } else if matches!(parameter.ty.as_ref(),
            TypeTerm::Reference { mutable: true, referent }
                if matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"))
        {
            build.budget.reserve(1)?;
            build.incoming_exclusive_bools.push(slot);
        }
    }
    if owns_guard {
        // The scoped release contract does not establish mixed borrowed
        // guard routes. Leave the existing reference-only bodies unchanged.
        build.budget.step(parameters.len())?;
        for parameter in parameters {
            if let TypeTerm::Reference { referent, .. } = parameter.ty.as_ref()
                && build.guard_bearing_type(referent)?
            {
                return Err(Stop::Pending(parameter.declaration.node));
            }
        }
    }
    let active = if matches!(frame, Frame::ActiveDisposer) {
        let ([slot], [parameter]) = (params.as_slice(), parameters.as_slice()) else {
            return Err(Stop::Pending(entry_site));
        };
        if build.slots[*slot].custom_disposer != Some(&header.declaration.id) {
            return Err(Stop::Pending(entry_site));
        }
        let remainder = build.slot(body, &parameter.ty)?;
        Some(DisposerFrame {
            operand: *slot,
            remainder,
        })
    } else {
        None
    };
    let end = build.emit(body, Action::None, Edges::End)?;
    let end = if let Frame::CallableTransfer(layout) = frame {
        let output = build.slot(body, result)?;
        build.callable_frame = Some((layout, output));
        build.emit(body, Action::Export(output), Edges::Next(end))?
    } else {
        end
    };
    let end = match active {
        Some(frame) => build.cleanup(body, &[frame.remainder], end)?,
        None => end,
    };
    let finish = build.cleanup_scope(body, &params, end)?;
    let entry = build.statement(
        body,
        finish,
        Exits {
            returning: Some(finish),
            looping: None,
        },
        1,
    )?;
    // Only an empty application frame can contribute its entry and no other
    // delivery point. Ordinary lowering already has its body and signature.
    let empty_entry = (matches!(frame, Frame::OrdinaryFunction)
        && module.key.origin == crate::loader::Origin::Application
        && parameters.is_empty()
        && matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void")
        && module.syntax.nodes[body].kind == crate::parser::NodeKind::Block
        && module.syntax.nodes[body].children.is_empty())
    .then(|| site_at(module, body));
    // Capturing pre-states is separate from the ordinary lowering frame and
    // does not assume delivery is enabled on any ordinary edge.
    let mut lowered = flow::run(build, entry, params, active, collect_trace, cache)?;
    lowered.empty_entry = empty_entry;
    Ok(lowered)
}

fn lower_implementation<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    declaration: &'g Declaration,
    Lowering { budget, cache }: Lowering<'_, 'a, 'g>,
) -> ResultOf<Lowered<'a, 'g>> {
    use crate::loader::Origin;

    let site = declaration.node;
    budget.step(1)?;
    let standard = module.key.origin == Origin::Standard && module.key.path == "core.rill";
    if module.key.origin != Origin::Application && !standard {
        return Err(Stop::Pending(site));
    }
    let inventory = typed.inventory();
    let mut attachment = None;
    for candidate in inventory.graph().implementations() {
        budget.step(1)?;
        if candidate.declaration == declaration.id {
            attachment = Some(candidate);
            break;
        }
    }
    let source = attachment.ok_or(Stop::Pending(site))?;
    // In the installed typed receipt the closed protection catalogue is keyed
    // by canonical standard family, including its implementations on app types.
    // Application registrations of those families are not plain bodies here.
    // Standard scalar bodies require the separate proof below.
    if (!standard && source.family.module.origin != Origin::Application)
        || source.family.kind != DeclarationKind::Family
    {
        return Err(Stop::Pending(site));
    }
    budget.step(1)?;
    let family = inventory
        .captured_family(&source.family)
        .ok_or(Stop::Pending(site))?;
    if !standard && !family.header.binders.is_empty() {
        return Err(Stop::Pending(site));
    }
    budget.step(family.implementations.len())?;
    let implementation = family
        .implementations
        .iter()
        .find(|body| std::ptr::eq(body.source, source))
        .ok_or(Stop::Pending(site))?;
    let body = concrete_implementation_body(implementation)?;
    let facts = typed
        .bodies()
        .checked_body(module, BodyOwner::Declaration(&declaration.id), budget)?
        .ok_or(Stop::Pending(site))?;
    let frame = if standard {
        scalar::implementation_frame(typed, module, declaration, implementation, facts, budget)?
    } else {
        Frame::PlainImplementation { body }
    };
    lower(
        typed,
        module,
        &implementation.header,
        frame,
        false,
        None,
        Lowering { budget, cache },
    )
}

fn concrete_implementation_body(
    implementation: &super::ordinary::ImplementationFacts<'_>,
) -> ResultOf<NodeId> {
    if implementation.compatibility != super::ordinary::Compatibility::Compatible
        || !implementation.header.binders.is_empty()
    {
        return Err(Stop::Pending(implementation.header.declaration.node));
    }
    Ok(implementation.source.body_node)
}

fn lower_expectation<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    declaration: &'g Declaration,
    lowering: Lowering<'_, 'a, 'g>,
) -> ResultOf<Lowered<'a, 'g>> {
    let root = declaration.node;
    let result = typed
        .bodies()
        .checked_body(
            module,
            BodyOwner::Declaration(&declaration.id),
            lowering.budget,
        )?
        .and_then(|body| body.result.as_ref())
        .ok_or(Stop::Pending(root))?;
    lower_expectation_body(typed, module, root, result, None, lowering)
}

fn lower_expectation_body<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    root: NodeId,
    result: &'a Type<'g>,
    facts: Option<&'a NodeIndex<'a, 'g>>,
    Lowering { budget, cache }: Lowering<'_, 'a, 'g>,
) -> ResultOf<Lowered<'a, 'g>> {
    let mut build = Build::new(typed, module, root, None, facts, cache.as_deref(), budget)?;
    let entry = build.expectation(result)?;
    flow::run(build, entry, Vec::new(), None, false, cache)
}

fn lower_startup<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    lowering: Lowering<'_, 'a, 'g>,
) -> ResultOf<Lowered<'a, 'g>> {
    let root = module.syntax.root;
    typed
        .bodies()
        .checked_body(module, BodyOwner::Startup, lowering.budget)?
        .ok_or(Stop::Pending(root))?;
    lower_startup_body(typed, module, None, lowering)
}

fn lower_startup_body<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    facts: Option<&'a NodeIndex<'a, 'g>>,
    Lowering { budget, cache }: Lowering<'_, 'a, 'g>,
) -> ResultOf<Lowered<'a, 'g>> {
    let mut build = Build::new(
        typed,
        module,
        module.syntax.root,
        None,
        facts,
        cache.as_deref(),
        budget,
    )?;
    let entry = build.startup()?;
    flow::run(build, entry, Vec::new(), None, false, cache)
}

fn retained_two_hop_bool_type<'a, 'g>(ty: &'a TypeTerm<'g>) -> Option<&'a Type<'g>> {
    let TypeTerm::Reference {
        mutable: true,
        referent,
    } = ty
    else {
        return None;
    };
    retained_bool_contents_type(referent)
}

fn retained_two_hop_bool_contract<'a, 'g>(
    header: &'a DeclarationFacts<'g>,
) -> Option<(&'a super::MemberType<'g>, &'a Type<'g>)> {
    if header.declaration.id.kind != DeclarationKind::Function || !header.binders.is_empty() {
        return None;
    }
    let DeclarationShape::Function { parameters, result } = &header.shape else {
        return None;
    };
    let [destination, source] = parameters.as_slice() else {
        return None;
    };
    let contents = retained_two_hop_bool_type(&destination.ty)?;
    (source.ty == *contents
        && matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void"))
    .then_some((destination, contents))
}

struct RetainedBoolContract<'a, 'g> {
    contents: &'a Type<'g>,
    extra: Option<RetainedCallExtra>,
    exclusive_source: bool,
}

#[derive(Clone, Copy)]
enum RetainedCallExtra {
    Bool { position: usize },
    OptionBool,
}

// Void calls: one destination, one or two compatible sources and at most one
// independent Bool. Check the same declared contract at calls and exits.
fn retained_bool_contract<'a, 'g>(
    parameters: &'a [super::MemberType<'g>],
    result: &Type<'g>,
    destination: usize,
    budget: &mut Budget,
) -> ResultOf<Option<RetainedBoolContract<'a, 'g>>> {
    if !matches!(parameters.len(), 2..=4)
        || !matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void")
    {
        return Ok(None);
    }
    budget.step(1)?;
    let Some(contents) = parameters
        .get(destination)
        .and_then(|formal| retained_bool_contents_type(&formal.ty))
    else {
        return Ok(None);
    };
    let mut scalar = None;
    let mut sources = 0;
    let mut exclusive_source = false;
    for (index, formal) in parameters.iter().enumerate() {
        budget.step(1)?;
        if index == destination {
            continue;
        }
        if formal.ty == *contents {
            sources += 1;
        } else if matches!((contents.as_ref(), formal.ty.as_ref()), (
            TypeTerm::Reference { referent: stored, .. },
            TypeTerm::Reference { mutable: true, referent: source },
        ) if stored == source)
        {
            sources += 1;
            exclusive_source = true;
        } else if matches!(formal.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
            && scalar.is_none()
        {
            scalar = Some(index);
        } else {
            return Ok(None);
        }
    }
    Ok(matches!(sources, 1 | 2).then_some(RetainedBoolContract {
        contents,
        extra: scalar.map(|position| RetainedCallExtra::Bool { position }),
        exclusive_source,
    }))
}

fn site_at(module: &Module, node: NodeId) -> Site<'_> {
    Site {
        module: &module.key,
        node,
        span: &module.syntax.nodes[node].span,
    }
}

#[derive(Clone)]
enum Stop {
    Rejected {
        site: NodeId,
        related: NodeId,
        kind: Conflict,
    },
    Pending(NodeId),
    Limit,
}
impl From<()> for Stop {
    fn from((): ()) -> Self {
        Self::Limit
    }
}
type ResultOf<T> = Result<T, Stop>;
type Slot = usize;
type Pc = usize;

#[derive(Clone, Copy)]
struct DisposerFrame {
    operand: Slot,
    remainder: Slot,
}

#[derive(Clone, Copy)]
struct Local<'g> {
    node: NodeId,
    /// Lexical cleanup root, including for leaves inside a nested child.
    owner: Slot,
    projection: Option<Projection<'g>>,
    /// Number of trailing slots in this region, not just direct fields.
    members: usize,
    reference_member: bool,
    has_guard_field: bool,
    custom_disposer: Option<&'g DeclarationId>,
    /// Materialized backing or descriptor of a computed scalar borrow.
    scalar_borrow_temporary: bool,
    reference_option: Option<ReferenceOption>,
    moves: bool,
    cleanup: StorageCleanup<'g>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReferenceOption {
    Shared,
    Exclusive,
    Empty,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct GuardRelease<'g> {
    implementation: &'g DeclarationId,
    entry: &'g DeclarationId,
}

impl<'g> GuardRelease<'g> {
    fn cleanup(self, operand: Place<'g>) -> Cleanup<'g> {
        Cleanup::ReleaseGuard {
            operand,
            implementation: self.implementation,
            entry: self.entry,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StorageCleanup<'g> {
    End,
    Drop,
    DropOptionPayload(&'g DeclarationId),
    Unlowered,
    ReleaseGuard(GuardRelease<'g>),
    DisposeListI32(ListDisposal<'g>),
    DisposeOwnedListIterI32(ListDisposal<'g>),
    DisposeListText(TextListDisposal<'g>),
    DisposeOwnedListIterText(TextListDisposal<'g>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct TextListDisposal<'g> {
    list: ListDisposal<'g>,
    element: TextElementDisposal<'g>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ListDisposal<'g> {
    implementation: &'g DeclarationId,
    bridge: &'g DeclarationId,
}

impl<'g> ListDisposal<'g> {
    fn cleanup(self, operand: Place<'g>) -> Cleanup<'g> {
        Cleanup::DisposeListI32 {
            operand,
            implementation: self.implementation,
            bridge: self.bridge,
        }
    }
}

impl<'g> Local<'g> {
    fn storage(self, slot: Slot) -> Range<Slot> {
        slot..slot + 1 + self.members
    }

    fn needs_boundary_cleanup(self) -> bool {
        self.members > 0
            || self.custom_disposer.is_some()
            || matches!(
                self.cleanup,
                StorageCleanup::ReleaseGuard(_)
                    | StorageCleanup::DisposeListI32(_)
                    | StorageCleanup::DisposeOwnedListIterI32(_)
                    | StorageCleanup::DisposeListText(_)
                    | StorageCleanup::DisposeOwnedListIterText(_)
            )
    }

    fn guard_release(self) -> Option<GuardRelease<'g>> {
        match self.cleanup {
            StorageCleanup::ReleaseGuard(release) => Some(release),
            _ => None,
        }
    }

    fn contains_guard(self) -> bool {
        self.has_guard_field || self.guard_release().is_some()
    }

    fn carried_loan_root(self, slot: Slot) -> Slot {
        if self.reference_member {
            self.owner
        } else {
            slot
        }
    }

    fn place(self, module: &'g Module) -> Place<'g> {
        Place {
            root: site_at(module, self.node),
            projection: self.projection,
        }
    }
}

#[derive(Clone, Copy)]
enum PairCall {
    Once,
    ReadOnly { typed_origin: Option<NodeId> },
}

#[derive(Clone, Copy)]
enum ScopedFrames {
    Prepared(usize),
    /// Captures are checked in caller flow before either body proof is built.
    DisjointExclusive {
        sources: [Slot; 2],
        tuple_depth: usize,
    },
}

#[derive(Clone, Copy)]
enum WrittenDisposal<'g> {
    Ordinary,
    Scalar(&'g DeclarationId),
    Text(TextElementDisposal<'g>),
}

#[derive(Clone, Copy)]
enum Action<'g> {
    None,
    BudgetBackedge,
    Init(Slot),
    CommitListText {
        first: Slot,
        count: usize,
        into: Slot,
        backing: CollectionRepresentation<'g>,
        provider: CollectionArtifact<'g>,
    },
    OpenOwnedList {
        source: Slot,
        cursor: Slot,
        backing: CollectionRepresentation<'g>,
        text_source_backing: Option<CollectionRepresentation<'g>>,
        provider: CollectionProvider<'g>,
    },
    AdvanceOwnedList {
        cursor: Slot,
        into: Option<Slot>,
        some: &'g DeclarationId,
        none: &'g DeclarationId,
        provider: CollectionProvider<'g>,
    },
    Read {
        from: Slot,
        into: Option<Slot>,
    },
    Move {
        from: Slot,
        into: Option<Slot>,
    },
    CheckRebuild {
        base: Slot,
        // None selects the base; Some is the contiguous stage range at commit.
        staged: Option<(Slot, usize)>,
    },
    CheckNestedFieldBase(Slot),
    UnloweredRebuildRead(Slot),
    ForbiddenBorrowedMove(Slot),
    Borrow {
        from: Slot,
        into: Slot,
        exclusive: bool,
        retained_target: bool,
    },
    Reborrow {
        parent: Slot,
        into: Slot,
        exclusive: bool,
        suspends: bool,
    },
    Dereference {
        from: Slot,
        into: Option<Slot>,
        explicit_copy: bool,
    },
    BindBorrowedMember {
        descriptor: Slot,
        projection: Projection<'g>,
        into: Slot,
        binding: BorrowedMemberBinding,
    },
    ReadListI32 {
        view: Slot,
        index: u64,
        into: Option<Slot>,
        owner: &'g DeclarationId,
        representation: CollectionArtifact<'g>,
        provider: CollectionArtifact<'g>,
    },
    EqualI32Literal {
        from: Slot,
        literal: i32,
        into: Slot,
    },
    Call(usize),
    CopyClosure {
        from: Slot,
        into: Slot,
    },
    InvokeClosurePair {
        receiver: Slot,
        into: Slot,
        call: PairCall,
    },
    InvokeMutableI32Cell {
        receiver: Slot,
        into: Slot,
    },
    TestVariant {
        operand: Slot,
        success: &'g DeclarationId,
    },
    CopyOptionPayload {
        operand: Slot,
        into: Slot,
    },
    TakeOptionPayload {
        operand: Slot,
        into: Slot,
    },
    EndEmptyOption(Slot),
    ConstructVariant {
        variant: &'g DeclarationId,
        payload: Option<Slot>,
        into: Slot,
    },
    ConstructResultError {
        constructor: &'g DeclarationId,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        payload: Slot,
        into: Slot,
    },
    TakeResultSuccess {
        operand: Slot,
        into: Option<Slot>,
    },
    ForwardResultFailure {
        operand: Slot,
        into: Slot,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
    },
    DropResultFailure {
        operand: Slot,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        storage_source: &'g std::path::Path,
    },
    WrapResultFailure {
        operand: Slot,
        payload: Slot,
        into: Slot,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        origin: &'g Span,
        cause_display: &'g DeclarationId,
    },
    FormListArrow {
        capture: Slot,
        into: Slot,
        mode: InvocationMode,
    },
    CallListArrows {
        pair: usize,
        receivers: [Slot; 2],
        into: Slot,
    },
    ScopedJoin {
        pair: ScopedFrames,
        receivers: [Slot; 2],
        into: Slot,
        role: super::typed::RoutineRole<'g>,
    },
    OverlappingScopedJoin {
        receivers: [Slot; 2],
        into: Slot,
    },
    CheckScopedReferenceResults {
        entries: [Slot; 2],
        body_depths: [usize; 2],
    },
    FormClosure {
        capture: Slot,
        second: Option<Slot>,
        into: Slot,
        mode: InvocationMode,
        cell_addend: Option<i32>,
        layout: Option<CallableLayout>,
        local_exclusive: bool,
    },
    Dispose {
        operand: Slot,
        target: WrittenDisposal<'g>,
    },
    Displace {
        target: Slot,
        incoming: Slot,
    },
    Install {
        target: Slot,
        incoming: Slot,
    },
    CopyThrough {
        descriptor: Slot,
        into: Option<Slot>,
    },
    InstallThrough {
        descriptor: Slot,
        incoming: Slot,
    },
    InstallThroughTwoHop {
        route: OriginalTwoHopBool,
        incoming: Slot,
    },
    CheckRetained,
    End(Slot),
    Cleanup(usize),
    EndScope(usize),
    CheckCallableReturn(Slot),
    Export(Slot),
}

#[derive(Clone, Copy)]
enum Edges {
    Next(Pc),
    Branch { yes: Pc, no: Pc },
    End,
    FinishStartup,
    FinishExpectation,
}

#[derive(Clone, Copy)]
struct Instruction<'g> {
    node: NodeId,
    action: Action<'g>,
    edges: Edges,
}

#[derive(Clone, Copy)]
struct Exits {
    returning: Option<Pc>,
    looping: Option<Loop>,
}

#[derive(Clone, Copy)]
struct Loop {
    breaks: Pc,
    continues: Pc,
}

type NodeIndex<'a, 'g> = BTreeMap<NodeId, &'a NodeFacts<'g>>;

enum BuildFacts<'a, 'g> {
    Local(NodeIndex<'a, 'g>),
    Shared(Arc<NodeIndex<'a, 'g>>),
    Indexed(&'a NodeIndex<'a, 'g>),
}

impl<'a, 'g> BuildFacts<'a, 'g> {
    fn nodes(&self) -> &NodeIndex<'a, 'g> {
        match self {
            Self::Local(nodes) => nodes,
            Self::Shared(nodes) => nodes,
            Self::Indexed(nodes) => nodes,
        }
    }

    // Graph-indexed delivery metering is separate from targeted index reuse.
    fn is_indexed(&self) -> bool {
        matches!(self, Self::Indexed(_))
    }
}

struct Build<'a, 'g, 'b> {
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    body: NodeId,
    facts: BuildFacts<'a, 'g>,
    locals: BTreeMap<&'g DeclarationId, Slot>,
    /// Checked nested regions expose value transfers, not general places.
    nested_record_values: BTreeMap<&'g DeclarationId, Slot>,
    /// Two-level owned tuple inputs kept out of general place resolution.
    nested_tuple_parameters: BTreeMap<&'g DeclarationId, Slot>,
    /// Exact named tuple bindings with their proved initializer slot.
    nested_tuple_bindings: BTreeMap<&'g DeclarationId, (Slot, Slot)>,
    /// Incoming narrow variants are visible only to the irrefutable binding path.
    narrow_variant_parameters:
        BTreeMap<*const DeclarationId, build::NarrowVariantParameter<'a, 'g>>,
    whole_child_patterns: BTreeSet<NodeId>,
    /// Separate contents of checked caller destinations from the access descriptors.
    retained: BTreeMap<Slot, RetainedContents>,
    retains_exclusive_bool: bool,
    incoming_exclusive_bools: Vec<Slot>,
    /// Incoming shared descriptors used by the exact scoped-join route.
    incoming_shared_i32: BTreeSet<Slot>,
    /// Receipt-local storage proof opened by an eligible incoming List<string>.
    text_list_disposal: Option<TextListDisposal<'g>>,
    slots: Vec<Local<'g>>,
    code: Vec<Instruction<'g>>,
    calls: Vec<Call<'a, 'g>>,
    list_arrow_pairs: Vec<build::ListArrowPair<'g>>,
    // A transferred descriptor, its external backing, and its exclusivity.
    // This is not a new acquisition.
    incoming_list_arrow: Option<(Slot, Slot, bool)>,
    cleanups: Vec<Vec<Slot>>,
    body_scope: Option<usize>,
    /// A top-level empty/continue loop whose only decision reads a Bool formal.
    list_delivery_loop: Option<(NodeId, NodeId)>,
    scalar_copy_read: Option<NodeId>,
    copied_origins: BTreeSet<NodeId>,
    dispose: Option<&'g DeclarationId>,
    return_contract: Option<(&'g DeclarationId, &'a Type<'g>)>,
    callable_frame: Option<(CallableLayout, Slot)>,
    proved_variant_cleanup: Vec<Type<'g>>,
    budget: &'b mut Budget,
}

#[derive(Clone, Copy)]
enum RetainedContents {
    OneHop(Slot),
    TwoHop(Slot),
}

impl RetainedContents {
    fn slot(self) -> Slot {
        match self {
            Self::OneHop(slot) | Self::TwoHop(slot) => slot,
        }
    }
}

/// The original exclusive route authorizes the store. Its inner descriptor
/// is never resolved as a separate value or replaced by the final contents.
#[derive(Clone, Copy)]
struct OriginalTwoHopBool {
    descriptor: Slot,
    contents: Slot,
}

struct Call<'a, 'g> {
    callee: &'g DeclarationId,
    args: Vec<Slot>,
    into: Option<Slot>,
    result: CallResult<'a, 'g>,
    destination: Option<CallDestination>,
}

#[derive(Clone, Copy)]
enum CallResult<'a, 'g> {
    Plain,
    ScalarCopy,
    RetainsInputs,
    RetainsRecordInput,
    TransfersCallable(&'a DeclarationFacts<'g>),
}

#[derive(Clone, Copy)]
struct CallDestination {
    storage: CallStorage,
    argument: Slot,
    input: Slot,
    second_input: Option<Slot>,
    extra: Option<RetainedCallExtra>,
}

#[derive(Clone, Copy)]
enum CallStorage {
    Local(Slot),
    ThroughParameter { descriptor: Slot, contents: Slot },
    ThroughTwoHopParameter(OriginalTwoHopBool),
    ThroughAlias { descriptor: Slot },
}
