//! Map deserialization that rejects duplicate keys.
//!
//! `serde_yaml` keeps the last of two equal mapping keys silently. A project
//! file declaring the same service or environment variable twice is
//! ambiguous, so berth rejects it instead of guessing which entry wins.

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

/// Collects map entries into `M`, failing on the first repeated key.
trait InsertUnique<K, V> {
    fn insert_unique(&mut self, key: K, value: V) -> bool;
}

impl<K: Eq + Hash, V> InsertUnique<K, V> for HashMap<K, V> {
    fn insert_unique(&mut self, key: K, value: V) -> bool {
        if self.contains_key(&key) {
            return false;
        }
        self.insert(key, value);
        true
    }
}

struct UniqueMapVisitor<M, K, V>(PhantomData<(M, K, V)>);

impl<'de, M, K, V> Visitor<'de> for UniqueMapVisitor<M, K, V>
where
    M: Default + InsertUnique<K, V>,
    K: Deserialize<'de> + fmt::Display,
    V: Deserialize<'de>,
{
    type Value = M;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a mapping with unique keys")
    }

    fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut map = M::default();
        while let Some(key) = access.next_key::<K>()? {
            let rendered = key.to_string();
            let value = access.next_value::<V>()?;
            if !map.insert_unique(key, value) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate key '{rendered}'"
                )));
            }
        }
        Ok(map)
    }
}

/// Ordered map entries in declaration order, with unique keys.
#[derive(Debug)]
pub(crate) struct OrderedEntries<K, V>(pub(crate) Vec<(K, V)>);

impl<K, V> Default for OrderedEntries<K, V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<K: PartialEq, V> InsertUnique<K, V> for OrderedEntries<K, V> {
    fn insert_unique(&mut self, key: K, value: V) -> bool {
        if self.0.iter().any(|(existing, _)| *existing == key) {
            return false;
        }
        self.0.push((key, value));
        true
    }
}

/// Deserializes map entries in declaration order, rejecting repeated keys.
///
/// Declaration order is significant: 7.0 assigns default ports and picks the
/// primary `DB_*` service in the order services are declared.
pub(crate) fn unique_ordered_map<'de, D, K, V>(deserializer: D) -> Result<Vec<(K, V)>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + PartialEq + fmt::Display,
    V: Deserialize<'de>,
{
    deserializer
        .deserialize_map(UniqueMapVisitor::<OrderedEntries<K, V>, K, V>(PhantomData))
        .map(|entries| entries.0)
}

/// Deserializes an optional `HashMap` whose keys must be unique.
pub(crate) fn optional_unique_hash_map<'de, D, K, V>(
    deserializer: D,
) -> Result<Option<HashMap<K, V>>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Eq + Hash + fmt::Display,
    V: Deserialize<'de>,
{
    struct OptionalVisitor<K, V>(PhantomData<(K, V)>);

    impl<'de, K, V> Visitor<'de> for OptionalVisitor<K, V>
    where
        K: Deserialize<'de> + Eq + Hash + fmt::Display,
        V: Deserialize<'de>,
    {
        type Value = Option<HashMap<K, V>>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an optional mapping with unique keys")
        }

        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where
            D2: Deserializer<'de>,
        {
            deserializer
                .deserialize_map(UniqueMapVisitor(PhantomData))
                .map(Some)
        }
    }

    deserializer.deserialize_option(OptionalVisitor(PhantomData))
}
