use super::{DeclarationId, Module, ModuleKey};
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

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

    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn remove<Q>(&mut self, key: &Q, meter: &mut impl Meter) -> Result<Option<V>, ()>
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

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter()
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn keys(&self) -> impl Iterator<Item = &K> {
        self.0.keys()
    }

    #[cfg_attr(not(test), expect(dead_code))]
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
}
