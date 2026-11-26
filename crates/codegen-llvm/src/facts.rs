use crate::error::BackendError;
use crate::syntax;
use crate::types::{Shape, Type};
use compiler::analysis::scalars::{CallFacts, LiteralEvidence, LiteralValue, NodeFactKind, Site};
use compiler::analysis::typed::TypedReport;
use compiler::analysis::{DeclarationFacts, DeclarationShape, Substitution};
use compiler::lexer::TokenKind;
use compiler::loader::{DeclarationId, DeclarationKind, Graph, Module};
use compiler::parser::NodeId;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct Parameter {
    pub node: NodeId,
    pub shape: Shape,
}

#[derive(Clone, Debug)]
pub(crate) struct Signature {
    pub parameters: Vec<Parameter>,
    pub never: bool,
}

pub(crate) struct Facts<'a, 'g> {
    graph: &'a Graph,
    typed: &'a TypedReport<'g>,
    shapes: BTreeMap<&'a DeclarationId, &'a DeclarationFacts<'g>>,
}

impl<'a, 'g> Facts<'a, 'g> {
    pub fn new(graph: &'a Graph, typed: &'a TypedReport<'g>) -> Self {
        let inventory = typed.inventory();
        let mut shapes = BTreeMap::new();
        for facts in inventory.type_facts().declarations() {
            shapes.insert(&facts.declaration.id, facts);
        }
        for declaration in graph
            .modules()
            .iter()
            .flat_map(|module| &module.declarations)
            .filter(|declaration| declaration.id.kind == DeclarationKind::Family)
        {
            if let Ok(Some(family)) = inventory.family(&declaration.id) {
                for implementation in &family.implementations {
                    shapes.insert(
                        &implementation.header.declaration.id,
                        &implementation.header,
                    );
                }
            }
        }
        Self {
            graph,
            typed,
            shapes,
        }
    }

    pub fn signature(
        &self,
        id: &DeclarationId,
        substitution: &[Type],
    ) -> Result<Signature, BackendError> {
        let (module, declaration) = syntax::declaration(self.graph, id)?;
        let unsupported =
            |what: String| BackendError::unsupported_at(&module.key.path, &declaration.span, what);
        let facts = self.shapes.get(id).ok_or_else(|| {
            BackendError::internal("a reached body without a published signature")
        })?;
        if facts.binders.len() != substitution.len() {
            return Err(unsupported(
                "a generic body reached without its type arguments".into(),
            ));
        }
        let DeclarationShape::Function { parameters, result } = &facts.shape else {
            return Err(BackendError::internal(
                "a reached body that is not a function",
            ));
        };
        let mut lowered = Vec::new();
        for parameter in parameters {
            let ty = Type::resolve(&parameter.ty, substitution)
                .ok_or_else(|| unsupported("a parameter whose type has no lowering yet".into()))?;
            let shape = ty
                .shape()
                .ok_or_else(|| unsupported(format!("a parameter of type {ty}")))?;
            lowered.push(Parameter {
                node: parameter.declaration.node,
                shape,
            });
        }
        let never = match Type::resolve(result, substitution) {
            Some(Type::Primitive("void")) => false,
            Some(Type::Primitive("never")) => true,
            Some(other) => return Err(unsupported(format!("a result of type {other}"))),
            None => {
                return Err(unsupported(
                    "a result whose type has no lowering yet".into(),
                ));
            }
        };
        Ok(Signature {
            parameters: lowered,
            never,
        })
    }

    pub fn literal(&self, module: &Module, node: NodeId) -> Result<Option<Vec<u8>>, BackendError> {
        let node = syntax::unwrap_groups(module, node)?;
        let Some(expression) = self.expression(module, node)? else {
            return Ok(None);
        };
        let Some(LiteralEvidence {
            token: Some(token),
            value: LiteralValue::Decoded,
            ..
        }) = expression.literal
        else {
            return Ok(None);
        };
        let TokenKind::Literal(literal) = &token.kind else {
            return Ok(None);
        };
        if Type::resolve(&expression.ty, &[]).as_ref() != Some(&Type::Primitive("string")) {
            return Ok(None);
        }
        let text: String = literal.decoded.iter().map(|scalar| scalar.value).collect();
        Ok(Some(text.into_bytes()))
    }

    pub fn call(&self, module: &Module, node: NodeId) -> Result<&'a CallFacts<'g>, BackendError> {
        self.expression(module, node)?
            .and_then(|expression| expression.call.as_deref())
            .ok_or_else(|| BackendError::internal("a call the typed report has no call facts for"))
    }

    fn expression(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&'a compiler::analysis::scalars::ExpressionFacts<'g>>, BackendError> {
        let facts = self
            .typed
            .bodies()
            .node(module, node)
            .map_err(|_| BackendError::internal("a node the typed report does not cover"))?;
        Ok(facts.and_then(|facts| match &facts.kind {
            NodeFactKind::Expression(expression) => Some(expression),
            _ => None,
        }))
    }
}

pub(crate) fn callee_substitution(
    call: &CallFacts<'_>,
    callee: &DeclarationId,
    caller: &[Type],
    site: Site<'_>,
) -> Result<Vec<Type>, BackendError> {
    let published = if call.callee == callee {
        &call.substitution
    } else {
        match &call.selection {
            Some(selection) if selection.implementation == callee => &selection.substitution,
            _ => {
                return Err(BackendError::internal(
                    "a call whose typed facts name another callee than its operation",
                ));
            }
        }
    };
    resolve_substitution(published, caller, site)
}

fn resolve_substitution(
    published: &[Substitution<'_>],
    caller: &[Type],
    site: Site<'_>,
) -> Result<Vec<Type>, BackendError> {
    let mut resolved: Vec<Option<Type>> = vec![None; published.len()];
    for entry in published {
        let ty = Type::resolve(&entry.argument, caller).ok_or_else(|| {
            BackendError::unsupported(site, "a type argument with no lowering yet")
        })?;
        let slot = resolved
            .get_mut(entry.position)
            .ok_or_else(|| BackendError::internal("a type argument position out of range"))?;
        *slot = Some(ty);
    }
    resolved
        .into_iter()
        .map(|ty| ty.ok_or_else(|| BackendError::internal("a missing type argument")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiler::analysis::AnalysisLimits;
    use compiler::analysis::typed::analyze_typed;
    use compiler::loader::{Limits, Prelude, Request, Selector, load};
    use compiler::parser::NodeKind;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(source: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "rill-codegen-facts-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("std")).unwrap();
            fs::create_dir_all(root.join("app")).unwrap();
            fs::write(
                root.join("std/core.rill"),
                "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
            )
            .unwrap();
            fs::write(root.join("app/main.rill"), source).unwrap();
            Self(root)
        }

        fn graph(&self) -> Graph {
            let selector = |name: &str| Selector {
                module: "core.rill".into(),
                name: name.into(),
            };
            load(Request {
                entry: self.0.join("app/main.rill"),
                standard_root: self.0.join("std"),
                prelude: Prelude {
                    copy: selector("copy"),
                    dispose: selector("dispose"),
                },
                target: "facts-test".into(),
                options: Default::default(),
                test_mode: false,
                limits: Limits::default(),
            })
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn string() -> Type {
        Type::Primitive("string")
    }

    fn view() -> Type {
        Type::Reference {
            mutable: false,
            referent: Box::new(string()),
        }
    }

    fn main(graph: &Graph) -> &Module {
        graph
            .modules()
            .iter()
            .find(|module| module.key.path == "main.rill")
            .unwrap()
    }

    fn function<'g>(module: &'g Module, name: &str) -> &'g DeclarationId {
        &module
            .declarations
            .iter()
            .find(|declaration| {
                declaration.name.as_deref() == Some(name)
                    && declaration.id.kind == DeclarationKind::Function
            })
            .unwrap()
            .id
    }

    fn substitution_at_call_of(
        graph: &Graph,
        facts: &Facts<'_, '_>,
        name: &str,
        caller: &[Type],
    ) -> Result<Vec<Type>, String> {
        let module = main(graph);
        let callee = function(module, name);
        let (node, call) = (0..module.syntax.nodes.len())
            .filter(|&node| module.syntax.nodes[node].kind == NodeKind::Call)
            .find_map(|node| {
                let call = facts.call(module, node).ok()?;
                (call.callee == callee).then_some((node, call))
            })
            .expect("a call of the function");
        let site = Site {
            module: &module.key,
            node,
            span: &module.syntax.nodes[node].span,
        };
        callee_substitution(call, callee, caller, site).map_err(|error| error.to_string())
    }

    #[test]
    fn a_generic_signature_is_resolved_through_the_instances_type_arguments() {
        let fixture = Fixture::new("function keep<T>(value: T): void { }\nkeep(\"a\");\n");
        let graph = fixture.graph();
        let typed = analyze_typed(&graph, AnalysisLimits::default());
        let facts = Facts::new(&graph, &typed);
        let keep = function(main(&graph), "keep");
        let shapes = |argument: Type| {
            facts
                .signature(keep, &[argument])
                .map(|signature| {
                    signature
                        .parameters
                        .iter()
                        .map(|parameter| parameter.shape)
                        .collect::<Vec<_>>()
                })
                .map_err(|error| error.to_string())
        };
        assert_eq!(shapes(string()), Ok(vec![Shape::Owned]));
        assert_eq!(shapes(view()), Ok(vec![Shape::View]));
        let refused = shapes(Type::Primitive("i32")).unwrap_err();
        assert!(refused.contains("a parameter of type i32"), "{refused}");
        let missing = facts.signature(keep, &[]).unwrap_err().to_string();
        assert!(missing.contains("without its type arguments"), "{missing}");
    }

    #[test]
    fn a_call_publishes_the_type_arguments_of_its_instance() {
        let fixture = Fixture::new("function keep<T>(value: T): void { }\nkeep(\"a\");\n");
        let graph = fixture.graph();
        let typed = analyze_typed(&graph, AnalysisLimits::default());
        let facts = Facts::new(&graph, &typed);
        assert_eq!(
            substitution_at_call_of(&graph, &facts, "keep", &[]),
            Ok(vec![string()])
        );
    }

    #[test]
    fn a_call_inside_a_generic_body_resolves_its_arguments_through_the_callers_instance() {
        let fixture = Fixture::new(
            "function inner<U>(value: U): void { }\nfunction outer<T>(value: T): void { inner(value); }\nouter(\"a\");\n",
        );
        let graph = fixture.graph();
        let typed = analyze_typed(&graph, AnalysisLimits::default());
        let facts = Facts::new(&graph, &typed);
        assert_eq!(
            substitution_at_call_of(&graph, &facts, "inner", &[view()]),
            Ok(vec![view()])
        );
        assert_eq!(
            substitution_at_call_of(&graph, &facts, "inner", &[]).unwrap_err(),
            "backend failure: the backend cannot lower a type argument with no lowering yet at main.rill:2:37, which the compilable-program rule admitted"
        );
        assert_eq!(
            substitution_at_call_of(&graph, &facts, "outer", &[]),
            Ok(vec![string()])
        );
    }

    #[test]
    fn a_callee_the_facts_do_not_name_is_an_error() {
        let fixture = Fixture::new(
            "function a<T>(value: T): void { }\nfunction b<T>(value: T): void { }\na(\"a\");\n",
        );
        let graph = fixture.graph();
        let typed = analyze_typed(&graph, AnalysisLimits::default());
        let facts = Facts::new(&graph, &typed);
        let module = main(&graph);
        let call = (0..module.syntax.nodes.len())
            .find_map(|node| facts.call(module, node).ok())
            .expect("a call");
        let site = Site {
            module: &module.key,
            node: 0,
            span: &module.syntax.nodes[0].span,
        };
        let error = callee_substitution(call, function(module, "b"), &[], site).unwrap_err();
        assert!(error.to_string().contains("another callee"), "{error}");
    }
}
