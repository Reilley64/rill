//! Index the lexical ancestors of M2 nodes and the declarations M3 assigned to them.
use super::*;

#[derive(Debug, Default)]
pub(super) struct Lexical<'g> {
    modules: Vec<ScopeIndex<'g>>,
}

#[derive(Debug)]
struct ScopeIndex<'g> {
    enclosing_block: Vec<Option<usize>>,
    enclosing_scope: BTreeMap<usize, Option<usize>>,
    declarations: BTreeMap<(usize, Namespace, &'g str), &'g Declaration>,
}

impl<'g> Analyzer<'g> {
    pub(super) fn index_lexical(&mut self) -> Work<()> {
        let mut lexical = Lexical::default();
        for module in self.graph.modules() {
            let nodes = &module.syntax.nodes;
            self.reserve(1 + nodes.len())?;
            let mut scopes = ScopeIndex {
                enclosing_block: vec![None; nodes.len()],
                enclosing_scope: BTreeMap::new(),
                declarations: BTreeMap::new(),
            };
            // M2 assigns parent indices after their children.
            for (id, node) in nodes.iter().enumerate().rev() {
                self.step(1 + node.children.len())?;
                let mut scope = scopes.enclosing_block[id];
                if node.kind == NodeKind::Block {
                    let start = node.span.start();
                    self.reserve(1)?;
                    scopes.enclosing_scope.insert(start, scope);
                    scope = Some(start);
                }
                for &child in &node.children {
                    scopes.enclosing_block[child] = scope;
                }
            }
            for declaration in &module.declarations {
                self.step(1)?;
                let (Some(scope), Some(name)) = (declaration.scope, declaration.name.as_deref())
                else {
                    continue;
                };
                let namespace = match declaration.id.kind {
                    DeclarationKind::Function => Namespace::ValueFunction,
                    DeclarationKind::Record | DeclarationKind::Enum => Namespace::TypeModule,
                    _ => continue,
                };
                self.reserve(1)?;
                if scopes
                    .declarations
                    .insert((scope, namespace, name), declaration)
                    .is_some()
                {
                    self.finding(
                        module,
                        declaration.node,
                        FindingKind::DuplicateLocalDeclaration,
                    )?;
                }
            }
            lexical.modules.push(scopes);
        }
        self.lexical = Some(Arc::new(lexical));
        Ok(())
    }

    pub(super) fn lexical_declaration(
        &mut self,
        module: &'g Module,
        node: NodeId,
        namespace: Namespace,
        name: &str,
    ) -> Work<Option<&'g Declaration>> {
        let Some(lexical) = self.lexical.clone() else {
            return Ok(None);
        };
        self.step(1)?;
        let scopes = &lexical.modules[self.slot(module)];
        let mut scope = scopes.enclosing_block[node];
        while let Some(start) = scope {
            self.step(1)?;
            if let Some(declaration) = scopes.declarations.get(&(start, namespace, name)) {
                return Ok(Some(declaration));
            }
            scope = scopes.enclosing_scope[&start];
        }
        Ok(None)
    }
}
