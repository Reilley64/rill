//! What the backend lowers today, as data the compilable-program rule reads.
//!
//! [`Shape`] lists every construct the backend lowers. A program that uses
//! anything else is not compilable yet: the rule reports it as R0605 at the
//! first unadmitted construct, from `check`, `build` and `run` alike. The
//! backend keeps its own refusals only as internal errors, because a program
//! that passes the rule never reaches one; `tests/coverage.rs` in
//! `codegen-llvm` lowers one program per shape listed here.
//!
//! To admit a shape, lower it in the backend first, add its variant to
//! [`Shape`] and to [`Shape::ALL`], make the classifiers below admit it, and
//! add its program to the coverage table. To narrow, remove it from all
//! four. The classifiers match every `Operation`, `Terminator` and `Cleanup`
//! variant with no wildcard, so a new MIR variant fails to compile until it is
//! decided here.
//!
//! This is analysis work and is metered under
//! `docs/agents/compiler-metering.md`: every probe is charged and every
//! allocation reserves first.
use super::CheckedBody;
use crate::analysis::declarations::Budget;
use crate::analysis::ownership::{Block, Cleanup, Operation, Place, Terminator};
use crate::analysis::scalars::{BodyOwner, LiteralValue, NodeFactKind, NodeFacts, Site};
use crate::analysis::typed::TypedReport;
use crate::analysis::{DeclarationFacts, DeclarationShape, TypeTerm};
use crate::loader::{DeclarationId, DeclarationKind, Graph, ModuleKey, keyed};
use crate::parser::NodeId;
use std::collections::{BTreeMap, BTreeSet};

type Work<T> = Result<T, ()>;

/// A construct the backend lowers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    /// A block that continues at the next block.
    Goto,
    /// The end of a function.
    Return,
    /// The end of the entry's startup.
    FinishStartup,
    /// A block that does nothing.
    Nothing,
    /// Initialize a place with a string literal.
    InitializeText,
    /// A shared view of a string literal's static bytes.
    BorrowStaticText,
    /// A shared borrow of a place.
    BorrowShared,
    /// Copy a view into a place.
    CopyInto,
    /// Move a value into a place.
    MoveInto,
    /// A direct call of a function or implementation.
    Call,
    /// Write one string and a newline to standard output.
    WriteLine,
    /// A block of cleanup actions.
    Cleanup,
    /// A cleanup action that ends storage and does nothing else.
    EndStorage,
    /// A cleanup action that drops an owned string.
    DropText,
    /// A parameter of type `string`.
    TextParameter,
    /// A parameter of type `&string`.
    TextViewParameter,
    /// A result of type `void`.
    VoidResult,
}

impl Shape {
    /// Every shape the backend lowers. A program that uses a construct outside
    /// this list is not compilable.
    pub const ALL: &'static [Shape] = &[
        Shape::Goto,
        Shape::Return,
        Shape::FinishStartup,
        Shape::Nothing,
        Shape::InitializeText,
        Shape::BorrowStaticText,
        Shape::BorrowShared,
        Shape::CopyInto,
        Shape::MoveInto,
        Shape::Call,
        Shape::WriteLine,
        Shape::Cleanup,
        Shape::EndStorage,
        Shape::DropText,
        Shape::TextParameter,
        Shape::TextViewParameter,
        Shape::VoidResult,
    ];
}

pub(super) struct Facts<'a, 'g> {
    graph: &'a Graph,
    typed: &'a TypedReport<'g>,
    modules: Vec<&'g ModuleKey>,
    nodes: Vec<BTreeMap<NodeId, &'a NodeFacts<'g>>>,
    functions: keyed::PathMap<&'g DeclarationId, &'a DeclarationFacts<'g>>,
}

impl<'a, 'g> Facts<'a, 'g> {
    pub(super) fn new(
        graph: &'a Graph,
        typed: &'a TypedReport<'g>,
        bodies: impl Iterator<Item = &'g ModuleKey>,
        budget: &mut Budget,
    ) -> Work<Self> {
        let mut modules: Vec<&'g ModuleKey> = Vec::new();
        let mut nodes = Vec::new();
        let mut width = 0usize;
        for module in bodies {
            budget.step(modules.len().max(1))?;
            if !modules.iter().any(|known| std::ptr::eq(*known, module)) {
                budget.reserve(2)?;
                width = width.max(module.path.len().checked_add(1).ok_or(())?);
                modules.push(module);
                nodes.push(BTreeMap::new());
            }
        }
        for fact in typed.bodies().typed_nodes() {
            budget.step(modules.len().max(1))?;
            if let Some(at) = modules
                .iter()
                .position(|known| std::ptr::eq(*known, fact.site.module))
            {
                budget.reserve(1)?;
                nodes[at].insert(fact.site.node, fact);
            }
        }
        let mut functions = keyed::PathMap::new();
        for fact in typed.inventory().type_facts().declarations() {
            budget.step(1)?;
            if matches!(fact.shape, DeclarationShape::Function { .. })
                && fact.declaration.id.kind == DeclarationKind::Function
            {
                budget.step(modules.len().checked_mul(width).ok_or(())?)?;
                if !modules
                    .iter()
                    .any(|known| fact.declaration.id.module == **known)
                {
                    continue;
                }
                budget.reserve(1)?;
                functions.insert(&fact.declaration.id, fact, budget)?;
            }
        }
        Ok(Self {
            graph,
            typed,
            modules,
            nodes,
            functions,
        })
    }

    fn expression(
        &self,
        site: Site<'g>,
        budget: &mut Budget,
    ) -> Work<Option<&'a crate::analysis::scalars::ExpressionFacts<'g>>> {
        budget.step(self.modules.len().max(1))?;
        let Some(at) = self
            .modules
            .iter()
            .position(|known| std::ptr::eq(*known, site.module))
        else {
            return Ok(None);
        };
        budget.step(self.nodes[at].len().max(1).ilog2() as usize + 1)?;
        Ok(
            match self.nodes[at].get(&site.node).map(|fact| &fact.kind) {
                Some(NodeFactKind::Expression(expression)) => Some(expression),
                _ => None,
            },
        )
    }

    fn string_literal(&self, site: Site<'g>, budget: &mut Budget) -> Work<bool> {
        Ok(self.expression(site, budget)?.is_some_and(|expression| {
            matches!(expression.ty.as_ref(), TypeTerm::Primitive(ty) if ty.name() == "string")
                && expression
                    .literal
                    .is_some_and(|literal| literal.value == LiteralValue::Decoded)
        }))
    }

    fn plain_call(&self, site: Site<'g>, budget: &mut Budget) -> Work<bool> {
        Ok(self
            .expression(site, budget)?
            .and_then(|expression| expression.call.as_deref())
            .is_some_and(|call| call.pipe_input.is_none() && call.defaults.is_empty()))
    }

    fn header(
        &self,
        id: &DeclarationId,
        budget: &mut Budget,
    ) -> Work<Option<&'a DeclarationFacts<'g>>> {
        if id.kind != DeclarationKind::Implementation {
            return Ok(self.functions.get(id, budget)?.copied());
        }
        let Some(implementation) = self.graph.implementation(id) else {
            return Ok(None);
        };
        let inventory = self.typed.inventory();
        let Some(family) = inventory.captured_family(&implementation.family) else {
            return Ok(None);
        };
        for candidate in &family.implementations {
            budget.step(1)?;
            if candidate.source.declaration == *id {
                return Ok(Some(&candidate.header));
            }
        }
        Ok(None)
    }

    fn signature(
        &self,
        id: &DeclarationId,
        shapes: &mut Shapes,
        budget: &mut Budget,
    ) -> Work<bool> {
        let Some(facts) = self.header(id, budget)? else {
            return Ok(false);
        };
        let DeclarationShape::Function { parameters, result } = &facts.shape else {
            return Ok(false);
        };
        if !facts.binders.is_empty() {
            return Ok(false);
        }
        for parameter in parameters {
            budget.step(1)?;
            let shape = match parameter.ty.as_ref() {
                TypeTerm::Primitive(ty) if ty.name() == "string" => Shape::TextParameter,
                TypeTerm::Reference {
                    mutable: false,
                    referent,
                } if matches!(referent.as_ref(), TypeTerm::Primitive(ty) if ty.name() == "string") => {
                    Shape::TextViewParameter
                }
                _ => return Ok(false),
            };
            shapes.note(shape, budget)?;
        }
        budget.step(1)?;
        let shape = match result.as_ref() {
            TypeTerm::Primitive(ty) if ty.name() == "void" => Shape::VoidResult,
            _ => return Ok(false),
        };
        shapes.note(shape, budget)?;
        Ok(true)
    }
}

#[derive(Debug, Default)]
pub(super) struct Shapes(BTreeSet<Shape>);

impl Shapes {
    fn note(&mut self, shape: Shape, budget: &mut Budget) -> Work<()> {
        budget.step(1)?;
        if !self.0.contains(&shape) {
            budget.reserve(1)?;
            self.0.insert(shape);
        }
        Ok(())
    }

    pub(super) fn into_set(self) -> BTreeSet<Shape> {
        self.0
    }
}

pub(super) fn first_unadmitted<'g>(
    body: &CheckedBody<'_, 'g>,
    facts: &Facts<'_, 'g>,
    shapes: &mut Shapes,
    budget: &mut Budget,
) -> Work<Option<Site<'g>>> {
    if let BodyOwner::Declaration(id) = body.owner()
        && !facts.signature(id, shapes, budget)?
    {
        return Ok(Some(body.site()));
    }
    let mir = body.mir();
    budget.reserve(mir.blocks.len())?;
    let mut seen = vec![false; mir.blocks.len()];
    let mut at = mir.entry;
    loop {
        budget.step(1)?;
        let Some(block) = mir.blocks.get(at) else {
            return Ok(Some(body.site()));
        };
        if std::mem::replace(&mut seen[at], true) {
            return Ok(Some(block.site));
        }
        if !operation(block, facts, shapes, budget)? {
            return Ok(Some(block.site));
        }
        let (next, shape) = match block.terminator {
            Terminator::Goto(next) => (Some(next), Shape::Goto),
            Terminator::Return => (None, Shape::Return),
            Terminator::FinishStartup => (None, Shape::FinishStartup),
            Terminator::Branch { .. } | Terminator::FinishExpectation => {
                return Ok(Some(block.site));
            }
        };
        shapes.note(shape, budget)?;
        match next {
            Some(next) => at = next,
            None => return Ok(None),
        }
    }
}

fn plain(place: &Place<'_>) -> bool {
    place.projection.is_none()
}

fn operation<'g>(
    block: &Block<'g>,
    facts: &Facts<'_, 'g>,
    shapes: &mut Shapes,
    budget: &mut Budget,
) -> Work<bool> {
    let site = block.site;
    let shape = match &block.operation {
        Operation::None => Some(Shape::Nothing),
        Operation::Initialize(place) => {
            (plain(place) && facts.string_literal(site, budget)?).then_some(Shape::InitializeText)
        }
        Operation::BorrowStatic { holder } => plain(holder).then_some(Shape::BorrowStaticText),
        Operation::Borrow {
            source,
            holder,
            exclusive: false,
        } => (plain(source) && plain(holder)).then_some(Shape::BorrowShared),
        Operation::Copy {
            from,
            into: Some(into),
        } => (plain(from) && plain(into)).then_some(Shape::CopyInto),
        Operation::Move {
            from,
            into: Some(into),
        } => (plain(from) && plain(into)).then_some(Shape::MoveInto),
        Operation::Call(_) => facts.plain_call(site, budget)?.then_some(Shape::Call),
        Operation::WriteLine { text } => plain(text).then_some(Shape::WriteLine),
        Operation::Cleanup(actions) => {
            for action in actions {
                budget.step(1)?;
                let Some(shape) = cleanup(action) else {
                    return Ok(false);
                };
                shapes.note(shape, budget)?;
            }
            Some(Shape::Cleanup)
        }
        Operation::Borrow {
            exclusive: true, ..
        }
        | Operation::Copy { into: None, .. }
        | Operation::Move { into: None, .. }
        | Operation::CommitListText { .. }
        | Operation::CommitListRecordText { .. }
        | Operation::CommitListI32 { .. }
        | Operation::OpenOwnedListI32 { .. }
        | Operation::AdvanceOwnedListI32 { .. }
        | Operation::OpenOwnedListText { .. }
        | Operation::AdvanceOwnedListText { .. }
        | Operation::OpenOwnedListOptionText { .. }
        | Operation::AdvanceOwnedListOptionText { .. }
        | Operation::OpenOwnedListRecordText { .. }
        | Operation::AdvanceOwnedListRecordText { .. }
        | Operation::Reborrow { .. }
        | Operation::Dereference(_)
        | Operation::ReadListI32 { .. }
        | Operation::LookupListElement { .. }
        | Operation::EqualI32Literal { .. }
        | Operation::CheckedI32Multiply { .. }
        | Operation::CheckedI32Add { .. }
        | Operation::EqualI32 { .. }
        | Operation::FormatI32Template { .. }
        | Operation::CopyReferent { .. }
        | Operation::BindBorrowedMember { .. }
        | Operation::FormClosure { .. }
        | Operation::FormScopedValueArrow { .. }
        | Operation::StageScopedValue { .. }
        | Operation::FormClosurePair { .. }
        | Operation::CopyClosure { .. }
        | Operation::CallCallableTransfer { .. }
        | Operation::CallListArrows { .. }
        | Operation::ScopedJoin(_)
        | Operation::CallScalarCopy { .. }
        | Operation::DisposeText { .. }
        | Operation::CallScalarDispose { .. }
        | Operation::CallBorrowingRecord { .. }
        | Operation::CallExclusiveView { .. }
        | Operation::CallIntoTemporary { .. }
        | Operation::InvokeClosurePair { .. }
        | Operation::InvokeDeclaredOnce { .. }
        | Operation::InvokeDeclaredShared { .. }
        | Operation::InvokeReadOnlyClosurePair { .. }
        | Operation::InvokeMutableI32Cell { .. }
        | Operation::CallRetaining { .. }
        | Operation::CallRetainingNoInput { .. }
        | Operation::CallRetainingPair { .. }
        | Operation::CallRetainingWithBool { .. }
        | Operation::CallRetainingWithOptionBool { .. }
        | Operation::TestVariant { .. }
        | Operation::CopyOptionPayload { .. }
        | Operation::TakeOptionPayload { .. }
        | Operation::EndEmptyOption(_)
        | Operation::ConstructVariant { .. }
        | Operation::ConstructResultError { .. }
        | Operation::TakeResultSuccess { .. }
        | Operation::ForwardResultFailure { .. }
        | Operation::DropResultFailure { .. }
        | Operation::WrapResultFailure { .. }
        | Operation::Dispose { .. }
        | Operation::EndDescriptor { .. }
        | Operation::Install { .. }
        | Operation::CopyThrough { .. }
        | Operation::InstallThrough { .. }
        | Operation::InstallThroughTwoHop { .. }
        | Operation::Export(_)
        | Operation::Drop(_)
        | Operation::Panic { .. }
        | Operation::DropCapture { .. } => None,
    };
    match shape {
        Some(shape) => {
            shapes.note(shape, budget)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

fn cleanup(action: &Cleanup<'_>) -> Option<Shape> {
    match action {
        Cleanup::EndStorage(_) => Some(Shape::EndStorage),
        Cleanup::Drop(place) => plain(place).then_some(Shape::DropText),
        Cleanup::DropFreshResultError { .. }
        | Cleanup::DropOptionPayload { .. }
        | Cleanup::DropCapture { .. }
        | Cleanup::ConsumeCallableTransfer { .. }
        | Cleanup::InvokeDisposer { .. }
        | Cleanup::ReleaseGuard { .. }
        | Cleanup::DisposeListI32 { .. }
        | Cleanup::DisposeOwnedListIterI32 { .. }
        | Cleanup::DisposeListText { .. }
        | Cleanup::DisposeOwnedListIterText { .. }
        | Cleanup::DropYieldedOptionText { .. }
        | Cleanup::DisposeListOptionText { .. }
        | Cleanup::DisposeOwnedListIterOptionText { .. }
        | Cleanup::DisposeOwnedListIterRecordText { .. }
        | Cleanup::DeferDisposerFields { .. } => None,
    }
}
