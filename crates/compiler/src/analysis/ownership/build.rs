//! Construct ownership operations from the M4 type and control facts.
use super::*;
use crate::analysis::scalars::ExpressionFacts;
use crate::analysis::scalars::generics::callables::CaptureForm;
use crate::loader::Origin;
use crate::parser::NodeKind;

fn primitive_reference(ty: &TypeTerm<'_>) -> bool {
    matches!(ty, TypeTerm::Reference { referent, .. } if matches!(referent.as_ref(), TypeTerm::Primitive(_)))
}

impl<'a, 'g> Build<'a, 'g> {
    fn depth(&mut self, depth: usize) -> ResultOf<()> {
        self.budget.step(1)?;
        if depth > self.budget.limits.max_depth.min(64) {
            return Err(Stop::Limit);
        }
        self.budget.statistics.max_depth = self.budget.statistics.max_depth.max(depth);
        Ok(())
    }

    pub(super) fn local(
        &mut self,
        id: &'g DeclarationId,
        node: NodeId,
        ty: &TypeTerm<'g>,
    ) -> ResultOf<Slot> {
        let slot = self.slot(node, ty)?;
        self.budget.reserve(1)?;
        self.locals.insert(id, slot);
        Ok(slot)
    }

    fn slot(&mut self, node: NodeId, ty: &TypeTerm<'g>) -> ResultOf<Slot> {
        let (moves, drops) = match ty {
            TypeTerm::Primitive(p) if p.name() == "string" => (true, true),
            TypeTerm::Primitive(_) => (false, false),
            TypeTerm::Reference { mutable, .. } => (*mutable, false),
            TypeTerm::Callable { .. } => (true, false),
            _ => return Err(Stop::Pending(node)),
        };
        self.budget.reserve(1)?;
        let slot = self.slots.len();
        self.slots.push(Local { node, moves, drops });
        Ok(slot)
    }

    fn value(&self, node: NodeId) -> ResultOf<&'a ExpressionFacts<'g>> {
        let Some(NodeFactKind::Expression(value)) = self.facts.get(&node).map(|f| &f.kind) else {
            return Err(Stop::Pending(node));
        };
        Ok(value)
    }

    fn place(&self, node: NodeId) -> ResultOf<Slot> {
        self.value(node)?
            .path
            .as_ref()
            .filter(|p| p.projections.is_empty())
            .and_then(|p| self.locals.get(p.root))
            .copied()
            .ok_or(Stop::Pending(node))
    }

    pub(super) fn emit(&mut self, node: NodeId, action: Action, edges: Edges) -> ResultOf<Pc> {
        self.budget.reserve(1)?;
        let pc = self.code.len();
        self.code.push(Instruction {
            node,
            action,
            edges,
        });
        Ok(pc)
    }

    pub(super) fn cleanup(&mut self, node: NodeId, slots: &[Slot], next: Pc) -> ResultOf<Pc> {
        if slots.is_empty() {
            return Ok(next);
        }
        self.budget.reserve(1 + slots.len())?;
        let index = self.cleanups.len();
        // Locals are collected in declaration order, and get consecutive slot ids.
        self.cleanups.push(slots.to_vec());
        self.emit(node, Action::Cleanup(index), Edges::Next(next))
    }

    pub(super) fn statement(
        &mut self,
        node: NodeId,
        next: Pc,
        exits: Exits,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.depth(depth)?;
        let syntax = &self.module.syntax.nodes[node];
        let children = &syntax.children;
        match syntax.kind {
            NodeKind::Block => {
                let mut locals = Vec::new();
                for &child in children {
                    self.budget.step(1)?;
                    let statement = &self.module.syntax.nodes[child];
                    if matches!(statement.kind, NodeKind::Binding { .. })
                        && self.module.syntax.nodes[statement.children[0]].kind
                            != NodeKind::Wildcard
                    {
                        let Some(NodeFactKind::Binding(binding)) =
                            self.facts.get(&child).map(|f| &f.kind)
                        else {
                            return Err(Stop::Pending(child));
                        };
                        let ty = binding.ty.as_ref().ok_or(Stop::Pending(child))?;
                        let local = self.local(
                            binding.declaration,
                            self.module.syntax.nodes[child].children[0],
                            ty,
                        )?;
                        self.budget.reserve(1)?;
                        locals.push(local);
                    }
                }
                let exits = Exits {
                    returning: self.cleanup(node, &locals, exits.returning)?,
                    looping: match exits.looping {
                        Some(looping) => Some(Loop {
                            breaks: self.cleanup(node, &locals, looping.breaks)?,
                            continues: self.cleanup(node, &locals, looping.continues)?,
                        }),
                        None => None,
                    },
                };
                let mut tail = self.cleanup(node, &locals, next)?;
                for &child in children.iter().rev() {
                    tail = self.statement(child, tail, exits, depth + 1)?;
                }
                Ok(tail)
            }
            NodeKind::Binding { .. } => {
                let value = *children.last().unwrap();
                if self.module.syntax.nodes[children[0]].kind == NodeKind::Wildcard {
                    return self.discard(node, value, next, depth + 1);
                }
                let Some(NodeFactKind::Binding(binding)) = self.facts.get(&node).map(|f| &f.kind)
                else {
                    return Err(Stop::Pending(node));
                };
                let slot = self.locals[binding.declaration];
                self.expression(value, Some(slot), next, depth + 1)
            }
            NodeKind::ExpressionStatement => self.discard(node, children[0], next, depth + 1),
            NodeKind::Assignment("=") => {
                let target = self.place(children[0])?;
                let value = children[1];
                let ty = self.value(value)?.ty.clone();
                let incoming = self.slot(value, &ty)?;
                let install = self.emit(
                    node,
                    Action::Install { target, incoming },
                    Edges::Next(next),
                )?;
                let displace = self.emit(
                    node,
                    Action::Displace { target, incoming },
                    Edges::Next(install),
                )?;
                self.expression(value, Some(incoming), displace, depth + 1)
            }
            NodeKind::Return => {
                if let Some(&value) = children.first() {
                    let ty = self.value(value)?.ty.clone();
                    let output = self.slot(value, &ty)?;
                    let export =
                        self.emit(node, Action::Export(output), Edges::Next(exits.returning))?;
                    self.expression(value, Some(output), export, depth + 1)
                } else {
                    Ok(exits.returning)
                }
            }
            NodeKind::IfStatement => {
                let cond = children[0];
                let (can_be_true, can_be_false) = self.outcomes(cond)?;
                let yes = if can_be_true {
                    self.statement(children[1], next, exits, depth + 1)?
                } else {
                    next
                };
                let no = if let Some(&alternate) = children.get(2).filter(|_| can_be_false) {
                    self.statement(alternate, next, exits, depth + 1)?
                } else {
                    next
                };
                self.condition(cond, yes, no, depth + 1)
            }
            NodeKind::While => {
                let cond = children[0];
                let (can_be_true, _) = self.outcomes(cond)?;
                let yes = if can_be_true {
                    let backedge = self.emit(node, Action::None, Edges::End)?;
                    let looping = Some(Loop {
                        breaks: next,
                        continues: backedge,
                    });
                    let yes = self.statement(
                        children[1],
                        backedge,
                        Exits { looping, ..exits },
                        depth + 1,
                    )?;
                    let head = self.condition(cond, yes, next, depth + 1)?;
                    self.code[backedge].edges = Edges::Next(head);
                    return Ok(head);
                } else {
                    next
                };
                self.condition(cond, yes, next, depth + 1)
            }
            NodeKind::Break => Ok(exits.looping.ok_or(Stop::Pending(node))?.breaks),
            NodeKind::Continue => Ok(exits.looping.ok_or(Stop::Pending(node))?.continues),
            _ => Err(Stop::Pending(node)),
        }
    }

    fn discard(
        &mut self,
        statement: NodeId,
        value: NodeId,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let ty = self.value(value)?.ty.clone();
        let slot = self.slot(value, &ty)?;
        let end = self.emit(statement, Action::End(slot), Edges::Next(next))?;
        self.expression(value, Some(slot), end, depth)
    }

    fn outcomes(&self, node: NodeId) -> ResultOf<(bool, bool)> {
        Ok(self
            .value(node)?
            .boolean
            .map_or((true, true), |b| (b.can_be_true(), b.can_be_false())))
    }

    fn condition(&mut self, node: NodeId, yes: Pc, no: Pc, depth: usize) -> ResultOf<Pc> {
        let edges = match self.outcomes(node)? {
            (false, true) => Edges::Next(no),
            (true, false) => Edges::Next(yes),
            (true, true) => Edges::Branch { yes, no },
            (false, false) => return Err(Stop::Pending(node)),
        };
        let branch = self.emit(node, Action::None, edges)?;
        self.expression(node, None, branch, depth + 1)
    }

    fn expression(
        &mut self,
        node: NodeId,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.depth(depth)?;
        let syntax = &self.module.syntax.nodes[node];
        let facts = self.value(node)?;
        let flow = &self.facts[&node].flow;
        if !flow.normal
            || flow.pending
            || flow.returns
            || !flow.breaks.is_empty()
            || !flow.continues.is_empty()
        {
            return Err(Stop::Pending(node));
        }
        let action = match syntax.kind {
            NodeKind::Group => return self.expression(syntax.children[0], into, next, depth + 1),
            NodeKind::Literal => into.map_or(Action::None, Action::Init),
            NodeKind::ValueReference => Action::Read {
                from: self.place(node)?,
                into,
            },
            NodeKind::Prefix("&" | "&mut") => {
                let operand = syntax.children[0];
                let into = into.ok_or(Stop::Pending(node))?;
                let exclusive = syntax.kind == NodeKind::Prefix("&mut");
                let source = &self.module.syntax.nodes[operand];
                if source.kind == NodeKind::Prefix("*") && primitive_reference(&facts.ty) {
                    let parent = source.children[0];
                    let suspends = matches!(
                        self.value(parent)?.ty.as_ref(),
                        TypeTerm::Reference { mutable: true, .. }
                    );
                    Action::Reborrow {
                        parent: self.place(parent)?,
                        into,
                        exclusive,
                        suspends,
                    }
                } else {
                    Action::Borrow {
                        from: self.place(operand)?,
                        into,
                        exclusive,
                    }
                }
            }
            NodeKind::Prefix("*") if matches!(facts.ty.as_ref(), TypeTerm::Primitive(p) if p.name() != "string") => {
                Action::Dereference {
                    from: self.place(syntax.children[0])?,
                    into,
                }
            }
            NodeKind::Arrow => {
                return self.closure(node, into.ok_or(Stop::Pending(node))?, next, depth + 1);
            }
            NodeKind::Call => return self.call(node, into, next, depth + 1),
            _ => return Err(Stop::Pending(node)),
        };
        self.emit(node, action, Edges::Next(next))
    }

    fn closure(&mut self, node: NodeId, into: Slot, next: Pc, depth: usize) -> ResultOf<Pc> {
        let arrow = self
            .facts
            .get(&node)
            .and_then(|f| f.callable.as_ref()?.arrow.as_deref())
            .ok_or(Stop::Pending(node))?;
        let body = self.module.syntax.nodes[node].children[1];
        let [capture] = arrow.captures.as_slice() else {
            return Err(Stop::Pending(node));
        };
        let mode = arrow.mode.ok_or(Stop::Pending(node))?;
        // Only an arrow whose entire body returns its single captured value is
        // lowered here. It has no additional dynamic access or cleanup to check.
        if arrow.premise.is_some()
            || !arrow.parameters.is_empty()
            || capture.site.node != body
            || !capture.reachable
            || capture.through.is_some()
            || !capture.path.projections.is_empty()
            || !matches!(
                (capture.form, self.module.syntax.nodes[body].kind),
                (
                    CaptureForm::ExplicitBorrow { mutable: false },
                    NodeKind::Prefix("&")
                ) | (
                    CaptureForm::Value | CaptureForm::CarriedReference { mutable: false },
                    NodeKind::ValueReference
                )
            )
        {
            return Err(Stop::Pending(node));
        }
        let ty = self.value(body)?.ty.clone();
        if !matches!(ty.as_ref(), TypeTerm::Primitive(_)) && !primitive_reference(&ty) {
            return Err(Stop::Pending(node));
        }
        let capture = self.slot(body, &ty)?;
        let form = self.emit(
            node,
            Action::FormClosure {
                capture,
                into,
                mode,
            },
            Edges::Next(next),
        )?;
        self.expression(body, Some(capture), form, depth + 1)
    }

    fn call(&mut self, node: NodeId, into: Option<Slot>, next: Pc, depth: usize) -> ResultOf<Pc> {
        let facts = self.value(node)?;
        let call = facts.call.as_deref().ok_or(Stop::Pending(node))?;
        if Some(call.callee) == self.dispose {
            let [arg] = call.arguments.as_slice() else {
                return Err(Stop::Pending(node));
            };
            // This slice lowers only explicit canonical disposal of primitives.
            if !matches!(self.value(*arg)?.ty.as_ref(), TypeTerm::Primitive(_)) {
                return Err(Stop::Pending(node));
            }
            let next = if let Some(into) = into {
                self.emit(node, Action::Init(into), Edges::Next(next))?
            } else {
                next
            };
            return self.emit(node, Action::Dispose(self.place(*arg)?), Edges::Next(next));
        }
        if call.callee.kind != DeclarationKind::Function
            || call.callee.module.origin != Origin::Application
            || !call.substitution.is_empty()
            || !matches!(facts.ty.as_ref(), TypeTerm::Primitive(_))
                && !primitive_reference(&facts.ty)
        {
            return Err(Stop::Pending(node));
        }
        self.budget.reserve(1 + call.arguments.len())?;
        let mut args = Vec::with_capacity(call.arguments.len());
        for arg in call.argument_evaluation_order() {
            self.budget.step(1)?;
            let ty = self.value(arg)?.ty.clone();
            if !matches!(ty.as_ref(), TypeTerm::Primitive(_)) && !primitive_reference(&ty) {
                return Err(Stop::Pending(arg));
            }
            args.push(self.slot(arg, &ty)?);
        }
        let index = self.calls.len();
        self.calls.push(Call {
            callee: call.callee,
            args,
            into,
            retains: primitive_reference(&facts.ty),
        });
        let mut next = self.emit(node, Action::Call(index), Edges::Next(next))?;
        for (i, &arg) in call.arguments.iter().enumerate().rev() {
            let slot = self.calls[index].args[i];
            let existing_exclusive =
                matches!(
                    self.value(arg)?.ty.as_ref(),
                    TypeTerm::Reference { mutable: true, .. }
                ) && !matches!(self.module.syntax.nodes[arg].kind, NodeKind::Prefix("&mut"));
            next = if existing_exclusive {
                self.emit(
                    arg,
                    Action::Reborrow {
                        parent: self.place(arg)?,
                        into: slot,
                        exclusive: true,
                        suspends: true,
                    },
                    Edges::Next(next),
                )?
            } else {
                self.expression(arg, Some(slot), next, depth + 1)?
            };
        }
        Ok(next)
    }
}
