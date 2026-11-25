use compiler::analysis::TypeTerm;
use compiler::loader::{DeclarationId, DeclarationKind, Origin};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Type {
    Primitive(&'static str),
    Reference {
        mutable: bool,
        referent: Box<Type>,
    },
    Nominal {
        declaration: DeclarationId,
        arguments: Vec<Type>,
    },
    Tuple(Vec<Type>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Shape {
    Owned,
    View,
}

impl Type {
    pub fn resolve(term: &TypeTerm<'_>, substitution: &[Type]) -> Option<Type> {
        Some(match term {
            TypeTerm::Primitive(intrinsic) => Type::Primitive(intrinsic.name()),
            TypeTerm::Parameter { position, .. } => substitution.get(*position)?.clone(),
            TypeTerm::Reference { mutable, referent } => Type::Reference {
                mutable: *mutable,
                referent: Box::new(Type::resolve(referent, substitution)?),
            },
            TypeTerm::Nominal {
                declaration,
                arguments,
            } => Type::Nominal {
                declaration: DeclarationId::clone(declaration),
                arguments: Type::resolve_all(arguments, substitution)?,
            },
            TypeTerm::Tuple(elements) => Type::Tuple(Type::resolve_all(elements, substitution)?),
            TypeTerm::Variant { .. } | TypeTerm::Callable { .. } => return None,
        })
    }

    fn resolve_all(
        terms: &[compiler::analysis::Type<'_>],
        substitution: &[Type],
    ) -> Option<Vec<Type>> {
        terms
            .iter()
            .map(|term| Type::resolve(term, substitution))
            .collect()
    }

    pub fn shape(&self) -> Option<Shape> {
        match self {
            Type::Primitive("string") => Some(Shape::Owned),
            Type::Reference {
                mutable: false,
                referent,
            } if **referent == Type::Primitive("string") => Some(Shape::View),
            _ => None,
        }
    }
}

pub(crate) fn kind_tag(kind: DeclarationKind) -> &'static str {
    match kind {
        DeclarationKind::Record => "record",
        DeclarationKind::Enum => "enum",
        DeclarationKind::Family => "family",
        DeclarationKind::Function => "function",
        DeclarationKind::Implementation => "implementation",
        DeclarationKind::Variant => "variant",
        DeclarationKind::Field => "field",
        DeclarationKind::Parameter => "parameter",
        DeclarationKind::TypeParameter => "typeparameter",
        DeclarationKind::Binding => "binding",
        DeclarationKind::Expectation => "expectation",
        DeclarationKind::Startup => "startup",
    }
}

pub(crate) fn origin_tag(origin: Origin) -> &'static str {
    match origin {
        Origin::Application => "app",
        Origin::Standard => "std",
    }
}

impl fmt::Display for Type {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Primitive(name) => formatter.write_str(name),
            Type::Reference { mutable, referent } => {
                write!(
                    formatter,
                    "&{}{referent}",
                    if *mutable { "mut " } else { "" }
                )
            }
            Type::Nominal {
                declaration,
                arguments,
            } => {
                write!(
                    formatter,
                    "{}:{}:{}",
                    origin_tag(declaration.module.origin),
                    declaration.module.path,
                    declaration.start
                )?;
                list(formatter, arguments, "<", ">")
            }
            Type::Tuple(elements) => list(formatter, elements, "[", "]"),
        }
    }
}

fn list(
    formatter: &mut fmt::Formatter<'_>,
    types: &[Type],
    open: &str,
    close: &str,
) -> fmt::Result {
    if types.is_empty() && open == "<" {
        return Ok(());
    }
    formatter.write_str(open)?;
    for (at, ty) in types.iter().enumerate() {
        if at > 0 {
            formatter.write_str(",")?;
        }
        write!(formatter, "{ty}")?;
    }
    formatter.write_str(close)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_is_owned_and_a_shared_reference_to_one_is_a_view() {
        let string = Type::Primitive("string");
        let view = Type::Reference {
            mutable: false,
            referent: Box::new(string.clone()),
        };
        assert_eq!(string.shape(), Some(Shape::Owned));
        assert_eq!(view.shape(), Some(Shape::View));
    }

    #[test]
    fn every_other_type_has_no_shape() {
        let string = Box::new(Type::Primitive("string"));
        for ty in [
            Type::Primitive("i32"),
            Type::Reference {
                mutable: true,
                referent: string.clone(),
            },
            Type::Reference {
                mutable: false,
                referent: Box::new(Type::Primitive("bool")),
            },
            Type::Tuple(vec![*string]),
        ] {
            assert_eq!(ty.shape(), None, "{ty}");
        }
    }

    #[test]
    fn types_render_without_debug_output() {
        let ty = Type::Tuple(vec![
            Type::Primitive("string"),
            Type::Reference {
                mutable: true,
                referent: Box::new(Type::Primitive("i32")),
            },
        ]);
        assert_eq!(ty.to_string(), "[string,&mut i32]");
    }
}
