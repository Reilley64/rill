use super::{DeclarationId, GraphId, Module, ModuleKey};
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Index;

pub(crate) trait Meter {
    fn step(&mut self, n: usize) -> Result<(), ()>;
}

pub(crate) trait PathKey {
    fn path(&self) -> &ModuleKey;
}

impl PathKey for ModuleKey {
    fn path(&self) -> &ModuleKey {
        self
    }
}

impl PathKey for DeclarationId {
    fn path(&self) -> &ModuleKey {
        &self.module
    }
}

impl<T: PathKey + ?Sized> PathKey for &T {
    fn path(&self) -> &ModuleKey {
        (**self).path()
    }
}

impl<T: PathKey, X> PathKey for (T, X) {
    fn path(&self) -> &ModuleKey {
        self.0.path()
    }
}

impl<T: PathKey, X, Y> PathKey for (T, X, Y) {
    fn path(&self) -> &ModuleKey {
        self.0.path()
    }
}

pub(crate) fn probes(len: usize) -> usize {
    (usize::BITS - len.leading_zeros() + 1) as usize
}

pub(crate) fn charge_probes(
    key: &ModuleKey,
    probes: usize,
    meter: &mut impl Meter,
) -> Result<(), ()> {
    let width = key.path.len().checked_add(1).ok_or(())?;
    meter.step(width.checked_mul(probes).ok_or(())?)
}

pub(crate) fn charge(key: &ModuleKey, len: usize, meter: &mut impl Meter) -> Result<(), ()> {
    charge_probes(key, probes(len), meter)
}

pub(crate) fn module_position(
    modules: &[Module],
    key: &ModuleKey,
    meter: &mut impl Meter,
) -> Result<Option<usize>, ()> {
    charge(key, modules.len(), meter)?;
    Ok(modules.binary_search_by(|module| module.key.cmp(key)).ok())
}

pub(crate) fn module_slot(modules: &[Module], module: &Module) -> Option<usize> {
    let size = std::mem::size_of::<Module>();
    let at = (module as *const Module as usize).checked_sub(modules.as_ptr() as usize)?;
    let index = at / size;
    (at % size == 0 && std::ptr::eq(modules.get(index)?, module)).then_some(index)
}

pub(crate) struct ModuleTables<K, V>(Vec<BTreeMap<K, V>>);

impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for ModuleTables<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.0).finish()
    }
}

impl<K, V> Default for ModuleTables<K, V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<K: Ord, V> ModuleTables<K, V> {
    pub(crate) fn size_for(&mut self, modules: usize) {
        self.0.resize_with(modules, BTreeMap::new);
    }

    pub(crate) fn is_sized(&self) -> bool {
        !self.0.is_empty()
    }

    pub(crate) fn get(
        &self,
        slot: usize,
        key: &K,
        meter: &mut impl Meter,
    ) -> Result<Option<&V>, ()> {
        meter.step(1)?;
        Ok(self.0.get(slot).and_then(|table| table.get(key)))
    }

    pub(crate) fn get_mut(
        &mut self,
        slot: usize,
        key: &K,
        meter: &mut impl Meter,
    ) -> Result<Option<&mut V>, ()> {
        meter.step(1)?;
        Ok(self.0.get_mut(slot).and_then(|table| table.get_mut(key)))
    }

    pub(crate) fn insert(
        &mut self,
        slot: usize,
        key: K,
        value: V,
        meter: &mut impl Meter,
    ) -> Result<Option<V>, ()> {
        meter.step(1)?;
        Ok(self
            .0
            .get_mut(slot)
            .expect("a module table is sized for its graph before it is filled")
            .insert(key, value))
    }

    pub(crate) fn table(&self, slot: usize) -> Option<&BTreeMap<K, V>> {
        self.0.get(slot)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.iter().map(BTreeMap::len).sum()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (usize, &K, &V)> {
        self.0
            .iter()
            .enumerate()
            .flat_map(|(slot, table)| table.iter().map(move |(key, value)| (slot, key, value)))
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.0.iter().flat_map(BTreeMap::values)
    }
}

pub(crate) struct PathMap<K, V>(BTreeMap<K, V>);

impl<K, V> Default for PathMap<K, V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: PathKey + Ord, V> PathMap<K, V> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn get<Q>(&self, key: &Q, meter: &mut impl Meter) -> Result<Option<&V>, ()>
    where
        K: Borrow<Q>,
        Q: PathKey + Ord + ?Sized,
    {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.get(key))
    }

    pub(crate) fn get_mut<Q>(
        &mut self,
        key: &Q,
        meter: &mut impl Meter,
    ) -> Result<Option<&mut V>, ()>
    where
        K: Borrow<Q>,
        Q: PathKey + Ord + ?Sized,
    {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.get_mut(key))
    }

    pub(crate) fn contains_key<Q>(&self, key: &Q, meter: &mut impl Meter) -> Result<bool, ()>
    where
        K: Borrow<Q>,
        Q: PathKey + Ord + ?Sized,
    {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.contains_key(key))
    }

    pub(crate) fn insert(
        &mut self,
        key: K,
        value: V,
        meter: &mut impl Meter,
    ) -> Result<Option<V>, ()> {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.insert(key, value))
    }

    pub(crate) fn get_or_insert_with(
        &mut self,
        key: K,
        make: impl FnOnce() -> V,
        meter: &mut impl Meter,
    ) -> Result<&mut V, ()> {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.entry(key).or_insert_with(make))
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn remove<Q>(&mut self, key: &Q, meter: &mut impl Meter) -> Result<Option<V>, ()>
    where
        K: Borrow<Q>,
        Q: PathKey + Ord + ?Sized,
    {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.remove(key))
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter()
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &K> {
        self.0.keys()
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.0.values()
    }
}

impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for PathMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub(crate) struct PathSet<K>(BTreeSet<K>);

impl<K> Default for PathSet<K> {
    fn default() -> Self {
        Self(BTreeSet::new())
    }
}

impl<K: PathKey + Ord> PathSet<K> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn contains<Q>(&self, key: &Q, meter: &mut impl Meter) -> Result<bool, ()>
    where
        K: Borrow<Q>,
        Q: PathKey + Ord + ?Sized,
    {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.contains(key))
    }

    pub(crate) fn insert(&mut self, key: K, meter: &mut impl Meter) -> Result<bool, ()> {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.insert(key))
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn remove<Q>(&mut self, key: &Q, meter: &mut impl Meter) -> Result<bool, ()>
    where
        K: Borrow<Q>,
        Q: PathKey + Ord + ?Sized,
    {
        charge(key.path(), self.0.len(), meter)?;
        Ok(self.0.remove(key))
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn iter(&self) -> impl Iterator<Item = &K> {
        self.0.iter()
    }
}

impl<K: fmt::Debug> fmt::Debug for PathSet<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub(crate) struct DeclarationTable<'g, V> {
    context: Option<GraphId>,
    slots: Vec<Option<usize>>,
    entries: Vec<Option<(&'g DeclarationId, V)>>,
    present: usize,
}

impl<V> Default for DeclarationTable<'_, V> {
    fn default() -> Self {
        Self {
            context: None,
            slots: Vec::new(),
            entries: Vec::new(),
            present: 0,
        }
    }
}

impl<'g, V> DeclarationTable<'g, V> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn size_for(&mut self, context: GraphId, count: usize) {
        self.context = Some(context);
        self.slots = vec![None; count];
    }

    fn index(&self, id: &DeclarationId) -> Option<usize> {
        if self.context.as_ref() != Some(&id.context) {
            return None;
        }
        self.slots.get(id.ordinal.index()).copied().flatten()
    }

    pub(crate) fn get(&self, id: &DeclarationId) -> Option<&V> {
        let index = self.index(id)?;
        self.entries[index].as_ref().map(|(_, value)| value)
    }

    pub(crate) fn get_mut(&mut self, id: &DeclarationId) -> Option<&mut V> {
        let index = self.index(id)?;
        self.entries[index].as_mut().map(|(_, value)| value)
    }

    pub(crate) fn contains_key(&self, id: &DeclarationId) -> bool {
        self.index(id).is_some()
    }

    pub(crate) fn insert(&mut self, id: &'g DeclarationId, value: V) -> Option<V> {
        if let Some(index) = self.index(id) {
            let (_, old) = self.entries[index].replace((id, value))?;
            return Some(old);
        }
        let slot = self
            .slots
            .get_mut(id.ordinal.index())
            .expect("a declaration table is sized for its graph before it is filled");
        *slot = Some(self.entries.len());
        self.entries.push(Some((id, value)));
        self.present += 1;
        None
    }

    pub(crate) fn remove(&mut self, id: &DeclarationId) -> Option<V> {
        let index = self.index(id)?;
        self.slots[id.ordinal.index()] = None;
        self.present -= 1;
        self.entries[index].take().map(|(_, value)| value)
    }

    pub(crate) fn len(&self) -> usize {
        self.present
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.present == 0
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&&'g DeclarationId, &V)> {
        self.entries.iter().flatten().map(|(id, value)| (id, value))
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &&'g DeclarationId> {
        self.entries.iter().flatten().map(|(id, _)| id)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().flatten().map(|(_, value)| value)
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.entries.iter_mut().flatten().map(|(_, value)| value)
    }

    pub(crate) fn follow_order<'a>(&mut self, order: impl Iterator<Item = &'a DeclarationId>) {
        let mut old = std::mem::take(&mut self.entries);
        let mut sorted = Vec::with_capacity(self.present);
        for id in order {
            let Some(index) = self.index(id) else {
                continue;
            };
            self.slots[id.ordinal.index()] = Some(sorted.len());
            sorted.push(old[index].take());
        }
        self.entries = sorted;
    }
}

impl<'a, 'g, V> IntoIterator for &'a DeclarationTable<'g, V> {
    type Item = (&'a &'g DeclarationId, &'a V);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl<V> Index<&DeclarationId> for DeclarationTable<'_, V> {
    type Output = V;

    fn index(&self, id: &DeclarationId) -> &V {
        self.get(id).expect("a declaration with a fact")
    }
}

impl<V: fmt::Debug> fmt::Debug for DeclarationTable<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::Origin;

    struct Meterstick {
        spent: usize,
        limit: usize,
    }

    impl Meterstick {
        fn with_limit(limit: usize) -> Self {
            Self { spent: 0, limit }
        }
    }

    impl Meter for Meterstick {
        fn step(&mut self, n: usize) -> Result<(), ()> {
            let next = self.spent.checked_add(n).ok_or(())?;
            if next > self.limit {
                return Err(());
            }
            self.spent = next;
            Ok(())
        }
    }

    fn key(path: &str) -> ModuleKey {
        ModuleKey {
            origin: Origin::Application,
            path: path.to_string(),
        }
    }

    fn map_of(entries: usize) -> PathMap<ModuleKey, usize> {
        let mut map = PathMap::new();
        for index in 0..entries {
            let mut meter = Meterstick::with_limit(usize::MAX);
            map.insert(key(&format!("m{index:03}")), index, &mut meter)
                .unwrap();
        }
        map
    }

    fn assert_charges_exactly<T>(
        expected: usize,
        mut probe: impl FnMut(&mut Meterstick) -> Result<T, ()>,
    ) {
        let mut exact = Meterstick::with_limit(expected);
        assert!(probe(&mut exact).is_ok());
        assert_eq!(exact.spent, expected);
        let mut short = Meterstick::with_limit(expected - 1);
        assert!(probe(&mut short).is_err());
        assert_eq!(short.spent, 0);
    }

    #[test]
    fn probe_counts_follow_the_halving_formula() {
        let counts: Vec<usize> = [0, 1, 2, 3, 4, 7, 8].into_iter().map(probes).collect();
        assert_eq!(counts, [1, 2, 3, 3, 4, 4, 5]);
    }

    #[test]
    fn every_map_probe_charges_path_plus_one_times_probes() {
        for entries in [0, 1, 5, 64] {
            for path in ["", "a", "app/deep/module"] {
                let expected = (path.len() + 1) * probes(entries);
                let probe_key = key(path);
                assert_charges_exactly(expected, |m| {
                    map_of(entries).get(&probe_key, m).map(|_| ())
                });
                assert_charges_exactly(expected, |m| {
                    map_of(entries).get_mut(&probe_key, m).map(|_| ())
                });
                assert_charges_exactly(expected, |m| map_of(entries).contains_key(&probe_key, m));
                assert_charges_exactly(expected, |m| {
                    map_of(entries).insert(probe_key.clone(), 0, m).map(|_| ())
                });
                assert_charges_exactly(expected, |m| {
                    map_of(entries).remove(&probe_key, m).map(|_| ())
                });
            }
        }
    }

    #[test]
    fn every_set_probe_charges_path_plus_one_times_probes() {
        fn set_of(entries: usize) -> PathSet<ModuleKey> {
            let mut set = PathSet::new();
            for index in 0..entries {
                let mut meter = Meterstick::with_limit(usize::MAX);
                set.insert(key(&format!("m{index:03}")), &mut meter)
                    .unwrap();
            }
            set
        }
        for entries in [0, 1, 5, 64] {
            for path in ["", "a", "app/deep/module"] {
                let expected = (path.len() + 1) * probes(entries);
                let probe_key = key(path);
                assert_charges_exactly(expected, |m| set_of(entries).contains(&probe_key, m));
                assert_charges_exactly(expected, |m| set_of(entries).insert(probe_key.clone(), m));
                assert_charges_exactly(expected, |m| set_of(entries).remove(&probe_key, m));
            }
        }
    }

    #[test]
    fn a_refused_insert_leaves_the_map_unchanged() {
        let mut map = map_of(3);
        let mut meter = Meterstick::with_limit(0);
        assert!(map.insert(key("late"), 9, &mut meter).is_err());
        assert_eq!(map.len(), 3);
        let mut meter = Meterstick::with_limit(usize::MAX);
        assert_eq!(map.get(&key("late"), &mut meter), Ok(None));
    }

    #[test]
    fn iteration_len_and_emptiness_cost_nothing_and_follow_key_order() {
        let mut map = PathMap::new();
        let mut meter = Meterstick::with_limit(usize::MAX);
        for path in ["b", "a", "c"] {
            map.insert(key(path), path.len(), &mut meter).unwrap();
        }
        let spent = meter.spent;
        let order: Vec<&str> = map.keys().map(|k| k.path.as_str()).collect();
        assert_eq!(order, ["a", "b", "c"]);
        assert_eq!(map.iter().count(), map.len());
        assert_eq!(map.values().count(), 3);
        assert!(!map.is_empty() && PathMap::<ModuleKey, ()>::new().is_empty());
        assert_eq!(meter.spent, spent);
        let mut set = PathSet::new();
        for path in ["b", "a", "c"] {
            set.insert(key(path), &mut meter).unwrap();
        }
        let spent_with_set = meter.spent;
        let order: Vec<&str> = set.iter().map(|k| k.path.as_str()).collect();
        assert_eq!(order, ["a", "b", "c"]);
        assert_eq!(set.len(), 3);
        assert!(!set.is_empty() && PathSet::<ModuleKey>::new().is_empty());
        assert_eq!(meter.spent, spent_with_set);
    }

    #[test]
    fn reference_keys_probe_through_borrowed_paths() {
        let a = key("a");
        let b = key("b");
        let mut map: PathMap<&ModuleKey, u8> = PathMap::new();
        let mut meter = Meterstick::with_limit(usize::MAX);
        map.insert(&a, 1, &mut meter).unwrap();
        assert_eq!(map.get(&a, &mut meter), Ok(Some(&1)));
        assert_eq!(map.get(&b, &mut meter), Ok(None));
    }

    #[test]
    fn debug_prints_exactly_like_the_btree_collections() {
        let pairs = [("b", 2usize), ("a", 1), ("c", 3)];
        let mut map = PathMap::new();
        let mut set = PathSet::new();
        let mut plain_map = BTreeMap::new();
        let mut plain_set = BTreeSet::new();
        let mut meter = Meterstick::with_limit(usize::MAX);
        for (path, value) in pairs {
            map.insert(key(path), value, &mut meter).unwrap();
            set.insert(key(path), &mut meter).unwrap();
            plain_map.insert(key(path), value);
            plain_set.insert(key(path));
        }
        assert_eq!(format!("{map:?}"), format!("{plain_map:?}"));
        assert_eq!(format!("{map:#?}"), format!("{plain_map:#?}"));
        assert_eq!(format!("{set:?}"), format!("{plain_set:?}"));
        assert_eq!(format!("{set:#?}"), format!("{plain_set:#?}"));
        let empty_map: PathMap<ModuleKey, usize> = PathMap::new();
        assert_eq!(format!("{empty_map:?}"), "{}");
        let empty_set: PathSet<ModuleKey> = PathSet::new();
        assert_eq!(format!("{empty_set:?}"), "{}");
    }
    fn declaration(context: &GraphId, module: &str, start: usize, ordinal: usize) -> DeclarationId {
        DeclarationId {
            context: context.clone(),
            module: key(module),
            start,
            kind: super::super::DeclarationKind::Function,
            ordinal: super::super::DeclarationOrdinal(ordinal),
        }
    }

    fn graph_id() -> GraphId {
        GraphId(std::sync::Arc::new(()))
    }

    #[test]
    fn a_declaration_table_reads_by_ordinal_and_rejects_a_foreign_graph() {
        let context = graph_id();
        let ids = [
            declaration(&context, "a", 1, 2),
            declaration(&context, "a", 9, 0),
            declaration(&context, "b", 3, 1),
        ];
        let mut table = DeclarationTable::new();
        assert!(table.get(&ids[0]).is_none());
        table.size_for(context.clone(), 3);
        for (value, id) in ids.iter().enumerate() {
            assert_eq!(table.insert(id, value), None);
        }
        assert_eq!(table.insert(&ids[1], 10), Some(1));
        assert_eq!(table.len(), 3);
        assert_eq!(table.get(&ids[1]), Some(&10));
        assert_eq!(table[&ids[2]], 2);
        *table.get_mut(&ids[0]).unwrap() += 5;
        assert_eq!(table.get(&ids[0]), Some(&5));
        let other = declaration(&graph_id(), "a", 1, 2);
        assert!(!table.contains_key(&other));
        assert_eq!(table.remove(&other), None);
        assert_eq!(table.remove(&ids[1]), Some(10));
        assert!(!table.contains_key(&ids[1]));
        assert_eq!(table.len(), 2);
        assert_eq!(table.keys().count(), 2);
        assert_eq!(table.values().copied().collect::<Vec<_>>(), [5, 2]);
        for value in table.values_mut() {
            *value += 1;
        }
        assert_eq!(table.values().copied().collect::<Vec<_>>(), [6, 3]);
    }

    #[test]
    fn a_declaration_table_prints_and_iterates_like_the_btree_map_it_replaces() {
        let context = graph_id();
        let ids = [
            declaration(&context, "a", 1, 5),
            declaration(&context, "a", 9, 4),
            declaration(&context, "b", 3, 3),
            declaration(&context, "c", 0, 0),
            declaration(&context, "c", 7, 1),
        ];
        let mut table = DeclarationTable::new();
        table.size_for(context.clone(), 6);
        let mut plain = BTreeMap::new();
        for index in [3, 0, 4, 2, 1] {
            table.insert(&ids[index], index);
            plain.insert(&ids[index], index);
        }
        let order: Vec<_> = ids.iter().collect();
        table.follow_order(order.iter().copied());
        assert_eq!(format!("{table:?}"), format!("{plain:?}"));
        assert_eq!(format!("{table:#?}"), format!("{plain:#?}"));
        assert!(
            table
                .iter()
                .map(|(id, v)| (*id, *v))
                .eq(plain.iter().map(|(id, v)| (*id, *v)))
        );
        assert_eq!(table.get(&ids[3]), Some(&3));
        assert_eq!(table.get(&ids[0]), Some(&0));
        let empty: DeclarationTable<'_, usize> = DeclarationTable::new();
        assert_eq!(format!("{empty:?}"), "{}");
    }
    #[test]
    fn module_tables_charge_one_step_and_iterate_in_module_then_key_order() {
        let mut tables: ModuleTables<(usize, usize), u8> = ModuleTables::default();
        tables.size_for(3);
        let mut meter = Meterstick::with_limit(usize::MAX);
        for (slot, key, value) in [
            (2, (5, 1), 5),
            (0, (9, 2), 1),
            (0, (3, 7), 2),
            (2, (5, 0), 4),
        ] {
            assert_eq!(tables.insert(slot, key, value, &mut meter), Ok(None));
        }
        assert_eq!(tables.insert(0, (3, 7), 3, &mut meter), Ok(Some(2)));
        assert_eq!(meter.spent, 5);
        assert_eq!(tables.get(0, &(3, 7), &mut meter), Ok(Some(&3)));
        assert_eq!(tables.get(1, &(3, 7), &mut meter), Ok(None));
        *tables.get_mut(2, &(5, 1), &mut meter).unwrap().unwrap() += 10;
        assert_eq!(meter.spent, 8);
        assert_eq!(tables.len(), 4);
        let order: Vec<_> = tables
            .iter()
            .map(|(slot, key, value)| (slot, *key, *value))
            .collect();
        assert_eq!(
            order,
            [
                (0, (3, 7), 3),
                (0, (9, 2), 1),
                (2, (5, 0), 4),
                (2, (5, 1), 15)
            ]
        );
        assert_eq!(tables.table(2).map(BTreeMap::len), Some(2));
        assert!(tables.table(3).is_none());
        let mut refused = Meterstick::with_limit(0);
        assert!(tables.get(0, &(3, 7), &mut refused).is_err());
        assert!(tables.insert(0, (1, 1), 9, &mut refused).is_err());
        assert_eq!(tables.len(), 4);
    }
}
