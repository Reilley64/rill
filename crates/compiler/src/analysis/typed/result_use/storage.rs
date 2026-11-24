//! Exact residences on one control-flow alternative.
use crate::analysis::{DeclarationId, Work, declarations::Budget, scalars::data::Constructor};
use crate::parser::NodeId;
use std::{cmp::Ordering, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ResultComponent<'g> {
    TupleElement(usize),
    ListElement(usize),
    Field(&'g DeclarationId),
    Capture(&'g DeclarationId),
    VariantPayload {
        variant: &'g DeclarationId,
        index: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Root<'g> {
    Temporary(NodeId),
    Local(&'g DeclarationId),
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Occurrence<'g> {
    pub(super) origin: NodeId,
    pub(super) component: Arc<[ResultComponent<'g>]>,
}

pub(super) type Address<'g> = (Root<'g>, Vec<ResultComponent<'g>>);

/// Identity of an unknown scalar value, not the address of mutable storage.
#[derive(Clone, Debug, Eq)]
pub(super) struct BooleanCopy(Arc<()>);

// Arc orders and compares its contents, so use a pointer comparison instead.
impl PartialEq for BooleanCopy {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Ord for BooleanCopy {
    fn cmp(&self, other: &Self) -> Ordering {
        Arc::as_ptr(&self.0).cmp(&Arc::as_ptr(&other.0))
    }
}

impl PartialOrd for BooleanCopy {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

type ClassPair<'a> = (&'a BooleanCopy, &'a BooleanCopy);

#[derive(Default)]
struct BooleanClasses<'a> {
    first: Option<ClassPair<'a>>,
    rest: Vec<ClassPair<'a>>,
}

impl<'a> BooleanClasses<'a> {
    fn agree(
        &mut self,
        left: &'a BooleanCopy,
        right: &'a BooleanCopy,
        budget: &mut Budget,
    ) -> Work<bool> {
        let Some(first) = self.first else {
            self.first = Some((left, right));
            return Ok(true);
        };
        budget.step(1 + self.rest.len())?;
        if let Some((x, y)) = std::iter::once(first)
            .chain(self.rest.iter().copied())
            .find(|&(x, y)| left == x || right == y)
        {
            return Ok(left == x && right == y);
        }
        budget.reserve(1)?;
        self.rest.push((left, right));
        Ok(true)
    }
}

pub(super) enum BooleanValue {
    Known(bool),
    Copy(BooleanCopy),
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Content<'g> {
    Result(Occurrence<'g>),
    Test(Arc<Constructor<'g>>, bool),
    /// A yielded canonical Option<string>, materialized on its first test.
    OptionItem {
        some: &'g DeclarationId,
        none: &'g DeclarationId,
    },
    BooleanCopy(BooleanCopy),
    Alias(Address<'g>),
    PossibleAlias(Address<'g>),
    /// A borrowed Bool descriptor from a named function's incoming parameter.
    CallerBoolOrigin(&'g DeclarationId),
    /// Typed source origin of the callable stored at this exact residence.
    Callable(NodeId),
    /// An exact, Result-free local pair was consumed by a checked once call.
    ConsumedPair,
    /// Containment is unresolved at this origin, not known empty.
    Unclassified(NodeId),
}

impl Content<'_> {
    fn is_boolean_hint(&self) -> bool {
        matches!(self, Self::BooleanCopy(_))
            || matches!(self, Self::Test(tag, _) if matches!(tag.as_ref(), Constructor::Boolean(_)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Located<'g> {
    pub(super) content: Content<'g>,
    pub(super) component: Vec<ResultComponent<'g>>,
}

impl Located<'_> {
    fn size(&self) -> usize {
        1 + self.component.len()
            + match &self.content {
                Content::Result(value) => value.component.len(),
                Content::Test(tag, _) => tag_size(tag),
                Content::Alias((_, path)) | Content::PossibleAlias((_, path)) => path.len(),
                Content::OptionItem { .. }
                | Content::BooleanCopy(_)
                | Content::CallerBoolOrigin(_)
                | Content::Callable(_)
                | Content::ConsumedPair
                | Content::Unclassified(_) => 0,
            }
    }
}

fn tag_size(tag: &Constructor<'_>) -> usize {
    match tag {
        Constructor::Text(text) => text.len() + 1,
        _ => 1,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OptionListSource {
    Fresh,
    GuardInspection { guard: NodeId, borrow: NodeId },
}

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub(super) struct Residence<'g> {
    locations: Vec<(Root<'g>, Located<'g>)>,
    // Entry evidence belongs to the formal in this frame, not to the value
    // being moved or borrowed. Never install it in a temporary or capture.
    option_lists: Vec<(&'g DeclarationId, OptionListSource)>,
    boolean_copies: usize,
}

impl<'g> Residence<'g> {
    pub(super) fn fork(&self, budget: &mut Budget) -> Work<Self> {
        for (_, value) in &self.locations {
            budget.reserve(value.size())?;
        }
        budget.reserve(self.option_lists.len())?;
        Ok(self.clone())
    }

    pub(super) fn add_option_list(
        &mut self,
        parameter: &'g DeclarationId,
        budget: &mut Budget,
    ) -> Work<()> {
        budget.reserve(1)?;
        self.option_lists.push((parameter, OptionListSource::Fresh));
        Ok(())
    }

    pub(super) fn has_option_lists(&self) -> bool {
        !self.option_lists.is_empty()
    }

    fn option_list_index(&self, id: &DeclarationId, budget: &mut Budget) -> Work<Option<usize>> {
        if id.kind == crate::loader::DeclarationKind::Parameter {
            for (index, (parameter, _)) in self.option_lists.iter().enumerate() {
                budget.step(1)?;
                if std::ptr::eq(*parameter, id) {
                    return Ok(Some(index));
                }
            }
        }
        Ok(None)
    }

    pub(super) fn option_list_source(
        &self,
        id: &DeclarationId,
        budget: &mut Budget,
    ) -> Work<Option<OptionListSource>> {
        Ok(self
            .option_list_index(id, budget)?
            .map(|i| self.option_lists[i].1))
    }

    pub(super) fn access_option_list(
        &mut self,
        id: &DeclarationId,
        inspection: Option<OptionListSource>,
        budget: &mut Budget,
    ) -> Work<()> {
        if let Some(index) = self.option_list_index(id, budget)? {
            if let Some(inspection) = inspection
                && self.option_lists[index].1 == OptionListSource::Fresh
            {
                self.option_lists[index].1 = inspection;
            } else {
                // Keep entry order stable for worklist comparisons.
                budget.step(self.option_lists.len() - index - 1)?;
                self.option_lists.remove(index);
            }
        }
        Ok(())
    }

    pub(super) fn contains_local(
        &self,
        local: &'g DeclarationId,
        budget: &mut Budget,
    ) -> Work<bool> {
        for (root, _) in &self.locations {
            budget.step(1)?;
            if matches!(root,Root::Local(id) if std::ptr::eq(*id,local)) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn may_owe_result(&self, budget: &mut Budget) -> Work<bool> {
        for (_, value) in &self.locations {
            budget.step(1)?;
            if matches!(value.content, Content::Result(_) | Content::Unclassified(_)) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn normalize(&mut self, budget: &mut Budget) -> Work<()> {
        let n = self.locations.len();
        let mut deepest = 1;
        for (_, value) in &self.locations {
            budget.step(1)?;
            deepest = deepest.max(value.size());
        }
        let comparisons = n
            .checked_mul(1 + n.checked_ilog2().unwrap_or(0) as usize)
            .ok_or(())?;
        budget.step(comparisons.checked_mul(deepest).ok_or(())?)?;
        self.locations.sort_unstable_by(|(a_root, a), (b_root, b)| {
            a_root
                .cmp(b_root)
                .then_with(|| match (&a.content, &b.content) {
                    // The labels name values, not positions. Order symbolic
                    // fields by their stable locations, never by allocation.
                    (Content::BooleanCopy(_), Content::BooleanCopy(_)) => {
                        a.component.cmp(&b.component)
                    }
                    _ => a.cmp(b),
                })
        });
        Ok(())
    }

    // Any alternative join must preserve this evidence as well as values.
    fn same_option_lists(&self, other: &Self, budget: &mut Budget) -> Work<bool> {
        if self.option_lists.len() != other.option_lists.len() {
            return Ok(false);
        }
        for ((a, a_source), (b, b_source)) in self.option_lists.iter().zip(&other.option_lists) {
            budget.step(1)?;
            if !std::ptr::eq(*a, *b) || a_source != b_source {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn same_as(&self, other: &Self, budget: &mut Budget) -> Work<bool> {
        budget.step(1)?;
        if self.locations.len() != other.locations.len()
            || !self.same_option_lists(other, budget)?
        {
            return Ok(false);
        }
        let mut copies = BooleanClasses::default();
        for (a, b) in self.locations.iter().zip(&other.locations) {
            budget.step(a.1.size() + b.1.size())?;
            if !Self::same_location(a, b, &mut copies, budget)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn same_location<'a>(
        a: &'a (Root<'g>, Located<'g>),
        b: &'a (Root<'g>, Located<'g>),
        copies: &mut BooleanClasses<'a>,
        budget: &mut Budget,
    ) -> Work<bool> {
        if let (Content::BooleanCopy(left), Content::BooleanCopy(right)) =
            (&a.1.content, &b.1.content)
        {
            // Compare the entire partition, including pairs with the same
            // label; otherwise another pair could split or identify a class.
            return Ok(a.0 == b.0
                && a.1.component == b.1.component
                && copies.agree(left, right, budget)?);
        }
        Ok(a == b)
    }

    /// Join two fallthrough alternatives only when the ordered residences
    /// have identical facts (up to copy labels), except for complementary
    /// truth at the matched source. Different ordering can miss a join.
    pub(super) fn join_boolean(
        &mut self,
        other: &Self,
        source: &Address<'g>,
        budget: &mut Budget,
    ) -> Work<bool> {
        budget.step(1)?;
        if self.locations.len() != other.locations.len()
            || !self.same_option_lists(other, budget)?
        {
            return Ok(false);
        }
        let mut remove = None;
        let mut copies = BooleanClasses::default();
        for (i, (a, b)) in self.locations.iter().zip(&other.locations).enumerate() {
            budget.step(a.1.size() + b.1.size())?;
            if Self::same_location(a, b, &mut copies, budget)? {
                continue;
            }
            budget.step(1 + source.1.len())?;
            if remove.is_some()
                || a.0 != source.0
                || b.0 != source.0
                || a.1.component != source.1
                || b.1.component != source.1
                || !matches!(
                    (&a.1.content, &b.1.content),
                    (Content::Test(a, positive_a), Content::Test(b, positive_b))
                        if matches!((a.as_ref(), b.as_ref()),
                            (Constructor::Boolean(a), Constructor::Boolean(b))
                                if (a == positive_a) != (b == positive_b))
                )
            {
                return Ok(false);
            }
            remove = Some(i);
        }
        if let Some(index) = remove {
            self.locations.swap_remove(index);
        }
        Ok(true)
    }

    pub(super) fn take_scope(
        &mut self,
        belongs: impl Fn(Root<'g>) -> bool,
        budget: &mut Budget,
    ) -> Work<Vec<(Root<'g>, Located<'g>)>> {
        let mut removed = Vec::new();
        let mut i = 0;
        while let Some((root, _)) = self.locations.get(i) {
            budget.step(1)?;
            if belongs(*root) {
                budget.reserve(1)?;
                let entry = self.locations.swap_remove(i);
                self.boolean_copies -=
                    usize::from(matches!(entry.1.content, Content::BooleanCopy(_)));
                removed.push(entry);
            } else {
                i += 1;
            }
        }
        Ok(removed)
    }

    pub(super) fn place(
        &mut self,
        occurrence: Occurrence<'g>,
        root: Root<'g>,
        component: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<()> {
        self.store(root, component, Content::Result(occurrence), budget)
    }

    pub(super) fn store(
        &mut self,
        root: Root<'g>,
        component: &[ResultComponent<'g>],
        content: Content<'g>,
        budget: &mut Budget,
    ) -> Work<()> {
        budget.reserve(1 + component.len())?;
        self.boolean_copies += usize::from(matches!(content, Content::BooleanCopy(_)));
        self.locations.push((
            root,
            Located {
                content,
                component: component.to_vec(),
            },
        ));
        Ok(())
    }

    pub(super) fn test(
        &self,
        root: Root<'g>,
        component: &[ResultComponent<'g>],
        tag: &Constructor<'g>,
        budget: &mut Budget,
    ) -> Work<Option<bool>> {
        let mut excluded = None;
        let mut complement = None;
        for (candidate, value) in &self.locations {
            budget.step(1 + component.len())?;
            if *candidate != root || value.component != component {
                continue;
            }
            match &value.content {
                Content::Test(known, positive) => {
                    budget.step(tag_size(known) + tag_size(tag))?;
                    if known.as_ref() == tag {
                        return Ok(Some(*positive));
                    }
                    if *positive {
                        return Ok(Some(false));
                    }
                    match (known.as_ref(), tag) {
                        (Constructor::Boolean(_), Constructor::Boolean(_)) => {
                            return Ok(Some(true));
                        }
                        (Constructor::Variant(other), Constructor::Variant(_)) => {
                            excluded = Some(*other)
                        }
                        _ => {}
                    }
                }
                Content::OptionItem { some, none } => {
                    if let Constructor::Variant(id) = tag {
                        complement = if std::ptr::eq(*id, *some) {
                            Some(*none)
                        } else if std::ptr::eq(*id, *none) {
                            Some(*some)
                        } else {
                            return Ok(Some(false));
                        };
                    }
                }
                _ => {}
            }
        }
        Ok(complement
            .zip(excluded)
            .and_then(|(a, b)| std::ptr::eq(a, b).then_some(true)))
    }

    pub(super) fn boolean_value(
        &self,
        root: Root<'g>,
        path: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<Option<BooleanValue>> {
        for (candidate, value) in &self.locations {
            budget.step(1 + path.len())?;
            if *candidate != root || value.component != path {
                continue;
            }
            match &value.content {
                Content::Test(tag, positive) => {
                    budget.step(tag_size(tag) + 1)?;
                    if let Constructor::Boolean(truth) = tag.as_ref() {
                        return Ok(Some(BooleanValue::Known(truth == positive)));
                    }
                }
                Content::BooleanCopy(copy) => return Ok(Some(BooleanValue::Copy(copy.clone()))),
                _ => {}
            }
        }
        Ok(None)
    }

    pub(super) fn has_boolean_copies(&self) -> bool {
        self.boolean_copies != 0
    }

    pub(super) fn boolean_copy_at(
        &self,
        root: Root<'g>,
        path: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<Option<BooleanCopy>> {
        if self.boolean_copies != 0
            && let Some(BooleanValue::Copy(copy)) = self.boolean_value(root, path, budget)?
        {
            return Ok(Some(copy));
        }
        Ok(None)
    }

    /// Re-root a scalar copy in the existing residence vector. An exact
    /// replacement can also drop a displaced Boolean hint, but never debt
    /// or other storage evidence. Leave mixed residences to the caller.
    pub(super) fn transfer_boolean_copy(
        &mut self,
        from: Root<'g>,
        to: Root<'g>,
        replace: bool,
        budget: &mut Budget,
    ) -> Work<bool> {
        if self.boolean_copies == 0 || from == to {
            return Ok(false);
        }
        let mut incoming = None;
        let mut displaced = None;
        for (i, (root, value)) in self.locations.iter().enumerate() {
            budget.step(1)?;
            if *root == from {
                if incoming.is_some()
                    || !value.component.is_empty()
                    || !matches!(value.content, Content::BooleanCopy(_))
                {
                    return Ok(false);
                }
                incoming = Some(i);
            } else if replace && *root == to {
                if displaced.is_some()
                    || !value.component.is_empty()
                    || !value.content.is_boolean_hint()
                {
                    return Ok(false);
                }
                displaced = Some(i);
            }
        }
        let Some(index) = incoming else {
            return Ok(false);
        };
        let (_, value) = self.locations.swap_remove(index);
        if let Some(mut old) = displaced {
            if old == self.locations.len() {
                old = index;
            }
            let (_, replaced) = self.locations.swap_remove(old);
            self.boolean_copies -= usize::from(matches!(replaced.content, Content::BooleanCopy(_)));
        }
        self.locations.push((to, value));
        Ok(true)
    }

    pub(super) fn move_boolean(
        &mut self,
        from: Root<'g>,
        to: Root<'g>,
        budget: &mut Budget,
    ) -> Work<()> {
        for (root, value) in &mut self.locations {
            budget.step(1)?;
            if *root == from && value.component.is_empty() {
                let boolean = match &value.content {
                    Content::BooleanCopy(_) => true,
                    Content::Test(tag, _) => matches!(tag.as_ref(), Constructor::Boolean(_)),
                    _ => false,
                };
                if boolean {
                    *root = to;
                    break;
                }
            }
        }
        Ok(())
    }

    pub(super) fn new_boolean_copy(
        &mut self,
        root: Root<'g>,
        path: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<BooleanCopy> {
        budget.reserve(1)?;
        let copy = BooleanCopy(Arc::new(()));
        self.store(root, path, Content::BooleanCopy(copy.clone()), budget)?;
        Ok(copy)
    }

    /// A source or snapshot with no surviving copies has no equality to retain.
    pub(super) fn remove_single_boolean_copy(
        &mut self,
        copy: &BooleanCopy,
        budget: &mut Budget,
    ) -> Work<()> {
        if self.boolean_copies == 0 {
            return Ok(());
        }
        let mut only = None;
        for (index, (_, value)) in self.locations.iter().enumerate() {
            budget.step(1)?;
            if matches!(&value.content, Content::BooleanCopy(known) if known == copy) {
                if only.is_some() {
                    return Ok(());
                }
                only = Some(index);
            }
        }
        if let Some(index) = only {
            self.locations.swap_remove(index);
            self.boolean_copies -= 1;
        }
        Ok(())
    }

    pub(super) fn refine_boolean_copy(
        &mut self,
        copy: &BooleanCopy,
        truth: bool,
        budget: &mut Budget,
    ) -> Work<()> {
        if self.boolean_copies == 0 {
            return Ok(());
        }
        budget.reserve(1)?;
        let tag = Arc::new(Constructor::Boolean(truth));
        for (_, value) in &mut self.locations {
            budget.step(1)?;
            if matches!(&value.content, Content::BooleanCopy(known) if known == copy) {
                value.content = Content::Test(tag.clone(), true);
                self.boolean_copies -= 1;
            }
        }
        Ok(())
    }

    pub(super) fn alias(
        &self,
        root: Root<'g>,
        path: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<Option<Address<'g>>> {
        for (candidate, value) in &self.locations {
            budget.step(1 + path.len())?;
            if *candidate == root
                && value.component == path
                && let Content::Alias(address) = &value.content
            {
                budget.reserve(address.1.len())?;
                return Ok(Some(address.clone()));
            }
        }
        Ok(None)
    }

    pub(super) fn caller_bool_origin(
        &self,
        root: Root<'g>,
        budget: &mut Budget,
    ) -> Work<Option<&'g DeclarationId>> {
        for (candidate, value) in &self.locations {
            budget.step(1)?;
            if *candidate == root
                && value.component.is_empty()
                && let Content::CallerBoolOrigin(parameter) = value.content
            {
                return Ok(Some(parameter));
            }
        }
        Ok(None)
    }

    pub(super) fn aliases_under(
        &self,
        root: Root<'g>,
        path: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<Vec<Address<'g>>> {
        let mut result = Vec::new();
        for (candidate, value) in &self.locations {
            budget.step(1 + path.len() + value.component.len())?;
            if *candidate != root {
                continue;
            }
            let address = match &value.content {
                Content::Alias(address) if value.component.starts_with(path) => address,
                Content::PossibleAlias(address)
                    if value.component.starts_with(path) || path.starts_with(&value.component) =>
                {
                    address
                }
                _ => continue,
            };
            budget.reserve(1 + address.1.len())?;
            result.push(address.clone());
        }
        Ok(result)
    }

    pub(super) fn owes_under(&self, root: Root<'g>, budget: &mut Budget) -> Work<bool> {
        for (candidate, value) in &self.locations {
            budget.step(1)?;
            if *candidate == root
                && matches!(value.content, Content::Result(_) | Content::Unclassified(_))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn contents(
        &self,
        root: Root<'g>,
        component: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<Vec<Located<'g>>> {
        let mut values = Vec::new();
        for (candidate, value) in &self.locations {
            budget.step(1 + component.len())?;
            if *candidate == root && value.component.starts_with(component) {
                budget.reserve(value.size())?;
                let mut value = value.clone();
                value.component.drain(..component.len());
                values.push(value);
            }
        }
        Ok(values)
    }

    pub(super) fn take(
        &mut self,
        root: Root<'g>,
        component: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<Vec<Located<'g>>> {
        let mut removed = Vec::new();
        let mut i = 0;
        while let Some((candidate, located)) = self.locations.get(i) {
            budget.step(1)?;
            if *candidate == root {
                budget.step(component.len())?;
                if located.component.starts_with(component) {
                    budget.reserve(1)?;
                    budget.step(located.component.len())?;
                    let (_, mut located) = self.locations.swap_remove(i);
                    self.boolean_copies -=
                        usize::from(matches!(located.content, Content::BooleanCopy(_)));
                    located.component.drain(..component.len());
                    removed.push(located);
                    continue;
                }
            }
            i += 1;
        }
        Ok(removed)
    }

    pub(super) fn install(
        &mut self,
        values: Vec<Located<'g>>,
        root: Root<'g>,
        component: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<()> {
        for mut value in values {
            if matches!(value.content, Content::ConsumedPair) {
                // Consumption describes the storage version, not a value in it.
                budget.step(1)?;
                continue;
            }
            budget.reserve(1 + component.len())?;
            // Rebase only the current location, not the diagnostic's origin path.
            if !component.is_empty() {
                budget.step(value.component.len())?;
                value.component.splice(..0, component.iter().copied());
            }
            self.boolean_copies += usize::from(matches!(value.content, Content::BooleanCopy(_)));
            self.locations.push((root, value));
        }
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        root: Root<'g>,
        component: &[ResultComponent<'g>],
        budget: &mut Budget,
    ) -> Work<()> {
        let mut i = 0;
        while let Some((candidate, located)) = self.locations.get(i) {
            budget.step(1 + component.len())?;
            if *candidate == root
                && located.component == component
                && matches!(located.content, Content::Result(_))
            {
                self.locations.swap_remove(i);
            } else {
                i += 1;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(node: NodeId) -> Root<'static> {
        Root::Temporary(node)
    }

    #[test]
    fn temporaries_of_the_same_origin_can_coexist_in_separate_components() {
        let mut state = Residence::default();
        let mut budget = test_budget(50);
        let first = [ResultComponent::TupleElement(0)];
        let second = [ResultComponent::TupleElement(1)];
        let origin = Occurrence {
            origin: 1,
            component: Arc::from([]),
        };
        state
            .place(origin.clone(), temp(2), &first, &mut budget)
            .unwrap();
        state.place(origin, temp(2), &second, &mut budget).unwrap();

        state.observe(temp(2), &first, &mut budget).unwrap();
        let remaining = state.take(temp(2), &[], &mut budget).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].component, second);
        let Content::Result(value) = &remaining[0].content else {
            panic!()
        };
        assert_eq!(value.origin, 1);
        assert!(value.component.is_empty());
    }

    #[test]
    fn forwarding_a_projection_keeps_its_origin_and_relative_components() {
        let mut state = Residence::default();
        let mut budget = test_budget(80);
        let field = ResultComponent::TupleElement(0);
        let nested = ResultComponent::TupleElement(2);
        let other = ResultComponent::TupleElement(1);
        state
            .place(
                Occurrence {
                    origin: 1,
                    component: Arc::from([field, nested]),
                },
                temp(2),
                &[field, nested],
                &mut budget,
            )
            .unwrap();
        state
            .place(
                Occurrence {
                    origin: 7,
                    component: Arc::from([]),
                },
                temp(2),
                &[other],
                &mut budget,
            )
            .unwrap();
        let moved = state.take(temp(2), &[field], &mut budget).unwrap();
        state
            .install(moved, temp(3), &[other], &mut budget)
            .unwrap();
        let remaining = state.take(temp(2), &[], &mut budget).unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(matches!(&remaining[0].content, Content::Result(value) if value.origin == 7));
        let discarded = state.take(temp(3), &[other], &mut budget).unwrap();
        assert_eq!(discarded.len(), 1);
        assert_eq!(discarded[0].component, vec![nested]);
        assert!(
            matches!(&discarded[0].content, Content::Result(value) if value.component.as_ref() == [field, nested])
        );
    }

    #[test]
    fn exhausting_a_residence_comparison_is_not_a_proved_discard() {
        let mut state = Residence::default();
        state
            .place(
                Occurrence {
                    origin: 1,
                    component: Arc::from([]),
                },
                temp(2),
                &[],
                &mut test_budget(10),
            )
            .unwrap();
        assert!(state.take(temp(2), &[], &mut test_budget(0)).is_err());
    }

    fn test_budget(max_work: usize) -> Budget {
        Budget {
            limits: crate::analysis::AnalysisLimits {
                max_work,
                ..Default::default()
            },
            statistics: Default::default(),
        }
    }
}
