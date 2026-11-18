use super::*;
use crate::analysis::ordinary::{RequirementTarget, selection, shape};
use crate::analysis::scalars::Site;
use crate::analysis::{Declaration, DeclarationShape, Module, NodeId, Payload, Type, TypeTerm};

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
            if let Some(facts) = inventory.type_facts().declarations.get(&declaration.id)
                && let DeclarationShape::Enum { variants } = &facts.shape
            {
                for variant in variants {
                    check.budget.step(1)?;
                    match &variant.payload {
                        Payload::Unit => (),
                        Payload::Tuple(types) => {
                            let payload = module.syntax.nodes[variant.declaration.node].children[1];
                            for (&node, ty) in
                                module.syntax.nodes[payload].children.iter().zip(types)
                            {
                                Position::new(
                                    declaration,
                                    module,
                                    node,
                                    PublicInterfaceKind::Payload,
                                )
                                .check_type(&mut check, ty)?;
                            }
                        }
                        Payload::Record(fields) => {
                            for field in fields {
                                let node = module.syntax.nodes[field.declaration.node].children[1];
                                Position::new(
                                    declaration,
                                    module,
                                    node,
                                    PublicInterfaceKind::Payload,
                                )
                                .check_type(&mut check, &field.ty)?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
