use super::*;
use crate::analysis::ordinary::{RequirementTarget, selection, shape};
use crate::analysis::scalars::Site;
use crate::analysis::{Declaration, Module, NodeId, NodeKind, Type, TypeTerm};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicInterfaceKind {
    Payload,
    Default,
    Requirement,
}

#[derive(Debug)]
pub struct PublicInterfaceFinding<'g> {
    pub kind: PublicInterfaceKind,
    pub owner: &'g Declaration,
    pub site: Site<'g>,
    pub target: &'g Declaration,
}

impl PublicInterfaceFinding<'_> {
    pub fn code(&self) -> &'static str {
        "R0401"
    }
}

struct Position<'g> {
    owner: &'g Declaration,
    site: Site<'g>,
    kind: PublicInterfaceKind,
}

impl<'g> Position<'g> {
    fn new(
        owner: &'g Declaration,
        module: &'g Module,
        node: NodeId,
        kind: PublicInterfaceKind,
    ) -> Self {
        Self {
            owner,
            site: Site {
                module: &module.key,
                node,
                span: &module.syntax.nodes[node].span,
            },
            kind,
        }
    }

    fn check_target(
        &self,
        check: &mut Interfaces<'_, 'g>,
        id: &crate::analysis::DeclarationId,
    ) -> Work<()> {
        let (_, target) = selection::declaration(check.inventory.graph(), id, check.budget)?
            .expect("canonical interface target in capture");
        if !target.public {
            check.budget.reserve(1)?;
            check.findings.push(PublicInterfaceFinding {
                kind: self.kind,
                owner: self.owner,
                site: self.site,
                target,
            });
        }
        Ok(())
    }

    fn check_type(&self, check: &mut Interfaces<'_, 'g>, ty: &Type<'g>) -> Work<()> {
        check.budget.reserve(1)?;
        let mut terms = vec![ty];
        while let Some(ty) = terms.pop() {
            check.budget.step(1)?;
            if let TypeTerm::Nominal { declaration, .. } = ty.as_ref() {
                self.check_target(check, declaration)?;
            }
            let (_, children) = shape(ty);
            check.budget.reserve(children.len())?;
            terms.extend(children.rev());
        }
        Ok(())
    }
}

struct Interfaces<'a, 'g> {
    inventory: &'a OrdinaryAnalysis<'g>,
    budget: &'a mut Budget,
    findings: &'a mut Vec<PublicInterfaceFinding<'g>>,
}

impl<'g> Interfaces<'_, 'g> {
    fn payload_type(
        &mut self,
        owner: &'g Declaration,
        module: &'g Module,
        node: NodeId,
    ) -> Work<()> {
        let uses = &self.inventory.type_facts().uses;
        let slot = crate::loader::keyed::module_slot(self.inventory.graph().modules(), module)
            .expect("a module of the graph");
        self.budget.reserve(1)?;
        let mut pending = vec![node];
        while let Some(node) = pending.pop() {
            let syntax = &module.syntax.nodes[node];
            self.budget.step(1)?;
            if let Some(ty) = uses.get(slot, &(syntax.span.start(), node), self.budget)? {
                Position::new(owner, module, node, PublicInterfaceKind::Payload)
                    .check_type(self, &ty.ty)?;
            } else {
                // A failed enclosing type expression can still contain resolved type uses.
                self.budget.reserve(syntax.children.len())?;
                pending.extend(syntax.children.iter().rev());
            }
        }
        Ok(())
    }
}

pub(super) fn check<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    budget: &mut Budget,
    findings: &mut Vec<PublicInterfaceFinding<'g>>,
) -> Work<()> {
    let mut check = Interfaces {
        inventory,
        budget,
        findings,
    };
    for module in inventory.graph().modules() {
        check.budget.step(1)?;
        for declaration in &module.declarations {
            check.budget.step(1)?;
            if !declaration.public {
                continue;
            }
            if let Some(header) = inventory.headers().get(&declaration.id) {
                for default in &header.defaults {
                    check.budget.step(1)?;
                    if let Some(ty) = &default.ty {
                        Position::new(
                            declaration,
                            module,
                            default.node,
                            PublicInterfaceKind::Default,
                        )
                        .check_type(&mut check, ty)?;
                    }
                }
                for requirement in &header.requirements {
                    check.budget.step(1)?;
                    let position = Position::new(
                        declaration,
                        module,
                        requirement.node,
                        PublicInterfaceKind::Requirement,
                    );
                    if let Some(RequirementTarget::Family(id)) = requirement.target {
                        position.check_target(&mut check, id)?;
                    }
                    for argument in &requirement.arguments {
                        check.budget.step(1)?;
                        if let Some(ty) = argument {
                            position.check_type(&mut check, ty)?;
                        }
                    }
                }
            }
            if declaration.id.kind == crate::loader::DeclarationKind::Enum {
                let nodes = &module.syntax.nodes;
                for &child in &nodes[declaration.node].children {
                    check.budget.step(1)?;
                    if nodes[child].kind != NodeKind::EnumVariant {
                        continue;
                    }
                    let Some(&payload) = nodes[child].children.get(1) else {
                        continue;
                    };
                    for &component in &nodes[payload].children {
                        let node = if nodes[payload].kind == NodeKind::RecordPayload {
                            nodes[component].children[1]
                        } else {
                            component
                        };
                        check.payload_type(declaration, module, node)?;
                    }
                }
            }
        }
    }
    Ok(())
}
