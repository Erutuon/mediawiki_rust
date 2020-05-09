use serde_json::{Map, Value};

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    hash::Hash,
};

pub trait Countable {
    fn count(&self) -> usize;
}

impl Countable for Value {
    /// Tries to return the len() of an API query result. Returns 0 if unknown
    fn count(&self) -> usize {
        if let Some(query) = self["query"].as_object() {
            query
                .iter()
                .find_map(|(_key, part)| part.as_array().map(|a| a.len()))
                .unwrap_or(0)
        } else {
            0 // Don't know size
        }
    }
}

/// A trait required for the type returned by `api::get_query_api_json_limit`.
pub trait Mergeable: Default {
    fn merge(&mut self, other: Self);
}

impl Mergeable for Value {
    fn merge(&mut self, b: Self) {
        match (self, b) {
            (a @ &mut Value::Object(_), Value::Object(b)) => {
                if let Some(a) = a.as_object_mut() {
                    for (k, v) in b {
                        a.entry(k).or_insert(Value::Null).merge(v);
                    }
                }
            }
            (a @ &mut Value::Array(_), Value::Array(b)) => {
                if let Some(a) = a.as_array_mut() {
                    a.merge(b);
                }
            }
            (a, b) => *a = b,
        }
    }
}

impl<T> Mergeable for Vec<T> {
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

impl<K, V> Mergeable for HashMap<K, V>
where
    K: Eq + Hash,
{
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

impl<K> Mergeable for HashSet<K>
where
    K: Eq + Hash,
{
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

impl<K, V> Mergeable for BTreeMap<K, V>
where
    K: Ord,
{
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

impl<K> Mergeable for BTreeSet<K>
where
    K: Ord,
{
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

/// Trait to return number of results in query response.
pub trait Continuable {
    type Continue: IntoIterator<Item = (String, String)> + Default;
    fn get_continue_params(&mut self) -> Option<Self::Continue>;
}

impl Continuable for Value {
    type Continue = BTreeMap<String, String>;
    fn get_continue_params(&mut self) -> Option<Self::Continue> {
        if let Value::Object(o) = self["continue"].take() {
            let o = o
                .into_iter()
                .filter(|x| x.0 != "continue")
                // Continue values are probably always strings;
                // if not, they will be converted into strings,
                // in JSON format.
                .map(|(k, v)| {
                    let v = if let Value::String(s) = v {
                        s
                    } else {
                        v.to_string()
                    };
                    (k, v)
                })
                .collect();
            Some(o)
        } else {
            None
        }
    }
}

pub trait Errorable {
    fn get_error(&self) -> Option<&Map<String, Value>>;
}

impl Errorable for Value {
    fn get_error(&self) -> Option<&Map<String, Value>> {
        self["error"].as_object()
    }
}
