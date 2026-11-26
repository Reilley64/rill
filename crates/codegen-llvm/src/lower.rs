use crate::error::BackendError;
use crate::facts::{Facts, Signature, callee_substitution};
use crate::mono::{self, Instance};
use crate::syntax;
use crate::types::{Shape, Type, kind_tag, origin_tag};
use compiler::analysis::compilable::{CheckedBody, CompilableProgram};
use compiler::analysis::ownership::{Block, Cleanup, Operation, Place, Terminator};
use compiler::analysis::scalars::{BodyOwner, Site};
use compiler::analysis::typed::TypedReport;
use compiler::loader::{DeclarationId, Graph, Module};
use compiler::parser::{NodeId, NodeKind};
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::builder::{Builder, BuilderError};
use inkwell::context::Context;
use inkwell::module::{Linkage, Module as LlvmModule};
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::types::{BasicMetadataTypeEnum, IntType, PointerType};
use inkwell::values::{
    BasicMetadataValueEnum, FunctionValue, GlobalValue, IntValue, PointerValue, ValueKind,
};
use inkwell::{AddressSpace, IntPredicate, OptimizationLevel};
use std::collections::BTreeMap;
use std::path::Path;

const STARTUP: &str = "rill_startup";
const STARTUP_CLEANUP: &str = "rill_startup_cleanup";
const CHECK_ALLOCATIONS: &str = "rill_check_allocations";
const RELEASE_STRING: &str = "rill.string.release";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Body {
    Startup,
    Declaration(DeclarationId),
}

#[derive(Clone, Copy)]
enum Value<'ctx> {
    Owned {
        pointer: PointerValue<'ctx>,
        length: IntValue<'ctx>,
        capacity: IntValue<'ctx>,
    },
    View {
        pointer: PointerValue<'ctx>,
        length: IntValue<'ctx>,
    },
}

impl<'ctx> Value<'ctx> {
    fn view(self) -> (PointerValue<'ctx>, IntValue<'ctx>) {
        match self {
            Self::Owned {
                pointer, length, ..
            }
            | Self::View { pointer, length } => (pointer, length),
        }
    }
}

type Values<'ctx> = BTreeMap<NodeId, Value<'ctx>>;

enum Flow {
    Continue,
    Diverged,
}

fn emit<T>(result: Result<T, BuilderError>) -> Result<T, BackendError> {
    result.map_err(|error| BackendError::internal(error.to_string()))
}

fn body_key(body: &CheckedBody<'_, '_>) -> Body {
    match body.owner() {
        BodyOwner::Startup => Body::Startup,
        BodyOwner::Declaration(id) => Body::Declaration(id.clone()),
    }
}

fn straight_line<'r, 'g>(body: &CheckedBody<'r, 'g>) -> Result<Vec<&'r Block<'g>>, BackendError> {
    let mir = body.mir();
    let mut seen = vec![false; mir.blocks.len()];
    let mut order = Vec::new();
    let mut at = mir.entry;
    loop {
        let block = mir
            .blocks
            .get(at)
            .ok_or_else(|| BackendError::internal("a block that is not in the body"))?;
        if std::mem::replace(&mut seen[at], true) {
            return Err(BackendError::unsupported(block.site, "a loop"));
        }
        order.push(block);
        match block.terminator {
            Terminator::Goto(next) => at = next,
            Terminator::Return | Terminator::FinishStartup => return Ok(order),
            Terminator::Branch { .. } => {
                return Err(BackendError::unsupported(block.site, "a branch"));
            }
            Terminator::FinishExpectation => {
                return Err(BackendError::unsupported(block.site, "an expectation"));
            }
        }
    }
}

fn callees(
    graph: &Graph,
    facts: &Facts<'_, '_>,
    bodies: &BTreeMap<Body, &CheckedBody<'_, '_>>,
    instance: &Instance<Body>,
) -> Result<Vec<Instance<Body>>, BackendError> {
    let body = bodies
        .get(&instance.body)
        .ok_or_else(|| BackendError::internal("a reached body the program does not carry"))?;
    let mut callees = Vec::new();
    for block in straight_line(body)? {
        if let Operation::Call(id) = &block.operation {
            let callee = Body::Declaration(DeclarationId::clone(id));
            if !bodies.contains_key(&callee) {
                return Err(BackendError::internal(
                    "a called body the program does not carry",
                ));
            }
            let module = syntax::module(graph, block.site.module)?;
            let substitution = callee_substitution(
                facts.call(module, block.site.node)?,
                id,
                &instance.substitution,
                block.site,
            )?;
            callees.push(Instance {
                body: callee,
                substitution,
            });
        }
    }
    Ok(callees)
}

fn encode(path: &str) -> String {
    path.bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02x}"),
        })
        .collect()
}

fn symbol(id: &DeclarationId, substitution: &[Type]) -> String {
    format!(
        "rill.{}.{}.{}.{}{}",
        origin_tag(id.module.origin),
        encode(&id.module.path),
        id.start,
        kind_tag(id.kind),
        mono::suffix(substitution)
    )
}

pub(crate) fn emit_object(
    graph: &Graph,
    typed: &TypedReport<'_>,
    program: &CompilableProgram<'_, '_>,
    object: &Path,
    check_allocations: bool,
) -> Result<(), BackendError> {
    let mut bodies = BTreeMap::new();
    bodies.insert(Body::Startup, program.startup());
    for body in program.functions() {
        bodies.insert(body_key(body), body);
    }
    let facts = Facts::new(graph, typed);
    let instances = mono::collect(Body::Startup, |instance| {
        callees(graph, &facts, &bodies, instance)
    })?;

    Target::initialize_native(&InitializationConfig::default()).map_err(BackendError::Target)?;
    let triple = TargetMachine::get_default_triple();
    let target =
        Target::from_triple(&triple).map_err(|error| BackendError::Target(error.to_string()))?;
    let machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| BackendError::Target("no target machine for the host".into()))?;

    let context = Context::create();
    let module = context.create_module("rill");
    module.set_triple(&triple);
    let layout = machine.get_target_data();
    module.set_data_layout(&layout.get_data_layout());
    let mut emitter = Emitter {
        graph,
        facts,
        context: &context,
        module,
        builder: context.create_builder(),
        word: context.ptr_sized_int_type(&layout, None),
        pointer: context.ptr_type(AddressSpace::default()),
        literals: BTreeMap::new(),
        functions: BTreeMap::new(),
    };
    for instance in &instances {
        emitter.declare(instance)?;
    }
    for instance in &instances {
        let body = bodies
            .get(&instance.body)
            .ok_or_else(|| BackendError::internal("an instance without a body"))?;
        emitter.lower(instance, body)?;
    }
    emitter.define_startup_cleanup()?;
    emitter.define_check_allocations(check_allocations);

    emitter
        .module
        .verify()
        .map_err(|error| BackendError::Verifier(error.to_string()))?;
    machine
        .write_to_file(&emitter.module, FileType::Object, object)
        .map_err(|error| BackendError::Target(error.to_string()))
}

struct Emitter<'a, 'g, 'ctx> {
    graph: &'a Graph,
    facts: Facts<'a, 'g>,
    context: &'ctx Context,
    module: LlvmModule<'ctx>,
    builder: Builder<'ctx>,
    word: IntType<'ctx>,
    pointer: PointerType<'ctx>,
    literals: BTreeMap<Vec<u8>, GlobalValue<'ctx>>,
    functions: BTreeMap<Instance<Body>, (FunctionValue<'ctx>, Signature)>,
}

impl<'ctx> Emitter<'_, '_, 'ctx> {
    fn declare(&mut self, instance: &Instance<Body>) -> Result<(), BackendError> {
        let (name, linkage, signature) = match &instance.body {
            Body::Startup => (
                STARTUP.to_owned(),
                None,
                Signature {
                    parameters: Vec::new(),
                    never: false,
                },
            ),
            Body::Declaration(id) => (
                symbol(id, &instance.substitution),
                Some(Linkage::Internal),
                self.facts.signature(id, &instance.substitution)?,
            ),
        };
        let mut parameters: Vec<BasicMetadataTypeEnum<'ctx>> = Vec::new();
        for parameter in &signature.parameters {
            parameters.push(self.pointer.into());
            parameters.push(self.word.into());
            if parameter.shape == Shape::Owned {
                parameters.push(self.word.into());
            }
        }
        let function = self.module.add_function(
            &name,
            self.context.void_type().fn_type(&parameters, false),
            linkage,
        );
        self.functions
            .insert(instance.clone(), (function, signature));
        Ok(())
    }

    fn lower(
        &mut self,
        instance: &Instance<Body>,
        body: &CheckedBody<'_, '_>,
    ) -> Result<(), BackendError> {
        let (function, signature) = self
            .functions
            .get(instance)
            .cloned()
            .ok_or_else(|| BackendError::internal("an instance that was never declared"))?;
        self.builder
            .position_at_end(self.context.append_basic_block(function, "entry"));
        let mut values = Values::new();
        let mut raw = function.get_param_iter();
        let mut next = || {
            raw.next()
                .ok_or_else(|| BackendError::internal("a missing parameter word"))
        };
        for parameter in &signature.parameters {
            let pointer = next()?.into_pointer_value();
            let length = next()?.into_int_value();
            let value = match parameter.shape {
                Shape::Owned => Value::Owned {
                    pointer,
                    length,
                    capacity: next()?.into_int_value(),
                },
                Shape::View => Value::View { pointer, length },
            };
            values.insert(parameter.node, value);
        }
        for block in straight_line(body)? {
            if let Flow::Diverged = self.operation(block, &mut values, &instance.substitution)? {
                return Ok(());
            }
        }
        if signature.never {
            emit(self.builder.build_unreachable())?;
        } else {
            emit(self.builder.build_return(None))?;
        }
        Ok(())
    }

    fn define_startup_cleanup(&self) -> Result<(), BackendError> {
        let function = self.module.add_function(
            STARTUP_CLEANUP,
            self.context.void_type().fn_type(&[], false),
            None,
        );
        self.builder
            .position_at_end(self.context.append_basic_block(function, "entry"));
        emit(self.builder.build_return(None))?;
        Ok(())
    }

    fn define_check_allocations(&self, enabled: bool) {
        let byte = self.context.i8_type();
        let global = self.module.add_global(byte, None, CHECK_ALLOCATIONS);
        global.set_constant(true);
        global.set_initializer(&byte.const_int(u64::from(enabled), false));
    }

    fn operation(
        &mut self,
        block: &Block<'_>,
        values: &mut Values<'ctx>,
        caller: &[Type],
    ) -> Result<Flow, BackendError> {
        let site = block.site;
        let module = syntax::module(self.graph, site.module)?;
        match &block.operation {
            Operation::None => {}
            Operation::Initialize(place) => {
                let bytes = self.facts.literal(module, site.node)?.ok_or_else(|| {
                    BackendError::unsupported(site, "an initialization other than a string literal")
                })?;
                let value = self.owned_literal(&bytes)?;
                values.insert(slot(place, site)?, value);
            }
            Operation::BorrowStatic { holder } => {
                let literal = match syntax::node(module, site.node)?.children.as_slice() {
                    [literal] => self.facts.literal(module, *literal)?,
                    _ => None,
                };
                let bytes = literal.ok_or_else(|| {
                    BackendError::unsupported(
                        site,
                        "a static view of something but a string literal",
                    )
                })?;
                let pointer = self.literal(&bytes);
                let length = self.word.const_int(bytes.len() as u64, false);
                values.insert(slot(holder, site)?, Value::View { pointer, length });
            }
            Operation::Borrow {
                source,
                holder,
                exclusive: false,
            } => {
                let (pointer, length) = lookup(values, source, site)?.view();
                values.insert(slot(holder, site)?, Value::View { pointer, length });
            }
            Operation::Copy {
                from,
                into: Some(into),
            } => {
                let value = lookup(values, from, site)?;
                if !matches!(value, Value::View { .. }) {
                    return Err(BackendError::unsupported(site, "a copy of an owned string"));
                }
                values.insert(slot(into, site)?, value);
            }
            Operation::Move {
                from,
                into: Some(into),
            } => {
                let value = lookup(values, from, site)?;
                values.insert(slot(into, site)?, value);
            }
            Operation::Call(id) => self.call(block, module, id, caller, values)?,
            Operation::WriteLine { text } => {
                let (pointer, length) = lookup(values, text, site)?.view();
                self.call_runtime(
                    "rill_rt_write_line",
                    self.context
                        .void_type()
                        .fn_type(&[self.pointer.into(), self.word.into()], false),
                    &[pointer.into(), length.into()],
                )?;
            }
            Operation::Panic { message } => {
                let (pointer, length) = lookup(values, message, site)?.view();
                let function = self.runtime(
                    "rill_rt_panic",
                    self.context
                        .void_type()
                        .fn_type(&[self.pointer.into(), self.word.into()], false),
                );
                function.add_attribute(
                    AttributeLoc::Function,
                    self.context
                        .create_enum_attribute(Attribute::get_named_enum_kind_id("noreturn"), 0),
                );
                emit(
                    self.builder
                        .build_call(function, &[pointer.into(), length.into()], ""),
                )?;
                emit(self.builder.build_unreachable())?;
                return Ok(Flow::Diverged);
            }
            Operation::Drop(place) => self.drop_value(lookup(values, place, site)?, site)?,
            Operation::Cleanup(actions) => {
                for action in actions {
                    match action {
                        Cleanup::EndStorage(_) => {}
                        Cleanup::Drop(place) => {
                            self.drop_value(lookup(values, place, site)?, site)?;
                        }
                        _ => return Err(BackendError::unsupported(site, "this cleanup action")),
                    }
                }
            }
            _ => {
                return Err(BackendError::unsupported(
                    site,
                    "an operation with no lowering yet",
                ));
            }
        }
        Ok(Flow::Continue)
    }

    fn call(
        &mut self,
        block: &Block<'_>,
        module: &Module,
        id: &DeclarationId,
        caller: &[Type],
        values: &Values<'ctx>,
    ) -> Result<(), BackendError> {
        let site = block.site;
        let call = self.facts.call(module, site.node)?;
        let callee = Instance {
            body: Body::Declaration(id.clone()),
            substitution: callee_substitution(call, id, caller, site)?,
        };
        let (function, signature) = self
            .functions
            .get(&callee)
            .cloned()
            .ok_or_else(|| BackendError::internal("a callee that was never declared"))?;
        if call.arguments.len() != signature.parameters.len() {
            return Err(BackendError::unsupported(
                site,
                "a call whose arguments do not match its parameters",
            ));
        }
        let mut words: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
        for (&argument, parameter) in call.arguments.iter().zip(&signature.parameters) {
            match (parameter.shape, argument_value(module, values, argument)?) {
                (
                    Shape::Owned,
                    Value::Owned {
                        pointer,
                        length,
                        capacity,
                    },
                ) => {
                    words.push(pointer.into());
                    words.push(length.into());
                    words.push(capacity.into());
                }
                (Shape::View, Value::View { pointer, length }) => {
                    words.push(pointer.into());
                    words.push(length.into());
                }
                _ => {
                    return Err(BackendError::unsupported(
                        site,
                        "an argument of another kind than its parameter",
                    ));
                }
            }
        }
        emit(self.builder.build_call(function, &words, ""))?;
        Ok(())
    }

    fn runtime(&self, name: &str, ty: inkwell::types::FunctionType<'ctx>) -> FunctionValue<'ctx> {
        self.module
            .get_function(name)
            .unwrap_or_else(|| self.module.add_function(name, ty, None))
    }

    fn call_runtime(
        &self,
        name: &str,
        ty: inkwell::types::FunctionType<'ctx>,
        arguments: &[BasicMetadataValueEnum<'ctx>],
    ) -> Result<(), BackendError> {
        let function = self.runtime(name, ty);
        emit(self.builder.build_call(function, arguments, ""))?;
        Ok(())
    }

    fn literal(&mut self, bytes: &[u8]) -> PointerValue<'ctx> {
        if bytes.is_empty() {
            return self.word.const_int(1, false).const_to_pointer(self.pointer);
        }
        if let Some(global) = self.literals.get(bytes) {
            return global.as_pointer_value();
        }
        let array = self.context.const_string(bytes, false);
        let global = self.module.add_global(
            array.get_type(),
            None,
            &format!("rill.literal.{}", self.literals.len()),
        );
        global.set_initializer(&array);
        global.set_constant(true);
        global.set_linkage(Linkage::Private);
        global.set_unnamed_addr(true);
        global.set_alignment(1);
        self.literals.insert(bytes.to_vec(), global);
        global.as_pointer_value()
    }

    fn owned_literal(&mut self, bytes: &[u8]) -> Result<Value<'ctx>, BackendError> {
        let length = self.word.const_int(bytes.len() as u64, false);
        if bytes.is_empty() {
            return Ok(Value::Owned {
                pointer: self.literal(bytes),
                length,
                capacity: self.word.const_zero(),
            });
        }
        let source = self.literal(bytes);
        let allocate = self.runtime(
            "rill_rt_allocate",
            self.pointer
                .fn_type(&[self.word.into(), self.word.into()], false),
        );
        let one = self.word.const_int(1, false);
        let pointer = match emit(self.builder.build_call(
            allocate,
            &[length.into(), one.into()],
            "owned",
        ))?
        .try_as_basic_value()
        {
            ValueKind::Basic(value) => value.into_pointer_value(),
            ValueKind::Instruction(_) => {
                return Err(BackendError::internal("the allocator returned no pointer"));
            }
        };
        emit(self.builder.build_memcpy(pointer, 1, source, 1, length))?;
        Ok(Value::Owned {
            pointer,
            length,
            capacity: length,
        })
    }

    fn drop_value(&self, value: Value<'ctx>, site: Site<'_>) -> Result<(), BackendError> {
        let Value::Owned {
            pointer, capacity, ..
        } = value
        else {
            return Err(BackendError::unsupported(site, "a drop of a view"));
        };
        let release = self.release_helper()?;
        emit(
            self.builder
                .build_call(release, &[pointer.into(), capacity.into()], ""),
        )?;
        Ok(())
    }

    fn release_helper(&self) -> Result<FunctionValue<'ctx>, BackendError> {
        if let Some(function) = self.module.get_function(RELEASE_STRING) {
            return Ok(function);
        }
        let resume = self.builder.get_insert_block();
        let function = self.module.add_function(
            RELEASE_STRING,
            self.context
                .void_type()
                .fn_type(&[self.pointer.into(), self.word.into()], false),
            Some(Linkage::Internal),
        );
        let entry = self.context.append_basic_block(function, "entry");
        let release = self.context.append_basic_block(function, "release");
        let done = self.context.append_basic_block(function, "done");
        self.builder.position_at_end(entry);
        let pointer = function.get_nth_param(0).map(|p| p.into_pointer_value());
        let capacity = function.get_nth_param(1).map(|p| p.into_int_value());
        let (Some(pointer), Some(capacity)) = (pointer, capacity) else {
            return Err(BackendError::internal(
                "the release helper lost a parameter",
            ));
        };
        let owns = emit(self.builder.build_int_compare(
            IntPredicate::NE,
            capacity,
            self.word.const_zero(),
            "owns",
        ))?;
        emit(self.builder.build_conditional_branch(owns, release, done))?;
        self.builder.position_at_end(release);
        let one = self.word.const_int(1, false);
        self.call_runtime(
            "rill_rt_release",
            self.context.void_type().fn_type(
                &[self.pointer.into(), self.word.into(), self.word.into()],
                false,
            ),
            &[pointer.into(), capacity.into(), one.into()],
        )?;
        emit(self.builder.build_unconditional_branch(done))?;
        self.builder.position_at_end(done);
        emit(self.builder.build_return(None))?;
        if let Some(resume) = resume {
            self.builder.position_at_end(resume);
        }
        Ok(function)
    }
}

fn slot(place: &Place<'_>, site: Site<'_>) -> Result<NodeId, BackendError> {
    match place.projection {
        None => Ok(place.root.node),
        Some(_) => Err(BackendError::unsupported(site, "a projected place")),
    }
}

fn lookup<'ctx>(
    values: &Values<'ctx>,
    place: &Place<'_>,
    site: Site<'_>,
) -> Result<Value<'ctx>, BackendError> {
    values
        .get(&slot(place, site)?)
        .copied()
        .ok_or_else(|| BackendError::internal("a place read before it was initialized"))
}

fn argument_value<'ctx>(
    module: &Module,
    values: &Values<'ctx>,
    argument: NodeId,
) -> Result<Value<'ctx>, BackendError> {
    let mut at = argument;
    loop {
        if let Some(value) = values.get(&at) {
            return Ok(*value);
        }
        let node = syntax::node(module, at)?;
        match (node.kind, node.children.as_slice()) {
            (NodeKind::Group, [inner]) => at = *inner,
            _ => return Err(BackendError::internal("an argument no operation staged")),
        }
    }
}
