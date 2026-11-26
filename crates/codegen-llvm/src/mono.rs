use crate::types::Type;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Instance<B> {
    pub body: B,
    pub substitution: Vec<Type>,
}

impl<B> Instance<B> {
    pub fn plain(body: B) -> Self {
        Self {
            body,
            substitution: Vec::new(),
        }
    }
}

pub(crate) fn suffix(substitution: &[Type]) -> String {
    if substitution.is_empty() {
        return String::new();
    }
    let arguments: Vec<String> = substitution.iter().map(Type::to_string).collect();
    format!("<{}>", arguments.join(","))
}

pub(crate) fn collect<B: Ord + Clone, E>(
    root: B,
    mut callees: impl FnMut(&Instance<B>) -> Result<Vec<Instance<B>>, E>,
) -> Result<Vec<Instance<B>>, E> {
    let root = Instance::plain(root);
    let mut seen = BTreeSet::from([root.clone()]);
    let mut instances = vec![root];
    let mut next = 0;
    while let Some(instance) = instances.get(next) {
        next += 1;
        for callee in callees(instance)? {
            if seen.insert(callee.clone()) {
                instances.push(callee);
            }
        }
    }
    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string() -> Type {
        Type::Primitive("string")
    }

    fn view() -> Type {
        Type::Reference {
            mutable: false,
            referent: Box::new(string()),
        }
    }

    fn call(body: &'static str, substitution: &[Type]) -> Instance<&'static str> {
        Instance {
            body,
            substitution: substitution.to_vec(),
        }
    }

    fn reached(
        root: &'static str,
        calls: &[(&'static str, Vec<Instance<&'static str>>)],
    ) -> Vec<(&'static str, String)> {
        collect(root, |instance| {
            Ok::<_, ()>(
                calls
                    .iter()
                    .find(|(body, _)| *body == instance.body)
                    .map(|(_, callees)| callees.clone())
                    .unwrap_or_default(),
            )
        })
        .unwrap()
        .into_iter()
        .map(|instance| (instance.body, suffix(&instance.substitution)))
        .collect()
    }

    #[test]
    fn a_generic_body_reached_at_two_types_gives_two_instances() {
        let calls = [
            (
                "main",
                vec![
                    call("id", &[string()]),
                    call("id", &[view()]),
                    call("id", &[string()]),
                    call("helper", &[]),
                ],
            ),
            ("id", vec![call("helper", &[])]),
        ];
        assert_eq!(
            reached("main", &calls),
            [
                ("main", String::new()),
                ("id", "<string>".into()),
                ("id", "<&string>".into()),
                ("helper", String::new()),
            ]
        );
    }

    #[test]
    fn a_body_nothing_reaches_has_no_instance() {
        let calls = [("main", vec![call("used", &[])]), ("unused", vec![])];
        assert_eq!(
            reached("main", &calls),
            [("main", String::new()), ("used", String::new())]
        );
    }

    #[test]
    fn a_cycle_ends_and_collection_order_is_repeatable() {
        let calls = [
            ("main", vec![call("a", &[]), call("b", &[])]),
            ("a", vec![call("b", &[]), call("main", &[])]),
            ("b", vec![call("a", &[])]),
        ];
        let first = reached("main", &calls);
        assert_eq!(
            first,
            [
                ("main", String::new()),
                ("a", String::new()),
                ("b", String::new())
            ]
        );
        assert_eq!(reached("main", &calls), first);
    }

    #[test]
    fn an_error_from_the_edges_is_returned() {
        let result = collect("main", |_| Err::<Vec<Instance<&str>>, _>("stop"));
        assert_eq!(result.unwrap_err(), "stop");
    }
}
