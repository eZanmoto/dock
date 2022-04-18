// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;

use snafu::Snafu;

#[derive(Debug)]
pub struct Trie<K, V> {
    dirs: Vec<Dir<K, V>>,
}

type Dir<K, V> = HashMap<K, Node<V>>;

#[derive(Debug)]
enum Node<V> {
    Value(V),
    DirIndex(usize),
}

impl<K: Clone + Eq + Hash, V> Trie<K, V> {
    pub fn new() -> Self {
        Trie{dirs: vec![HashMap::new()]}
    }

    pub fn insert(&mut self, key: &[K], value: V) -> Result<(), InsertError> {
        let mut key_components = key.to_vec();

        let last =
            if let Some(v) = key_components.pop() {
                v
            } else {
                return Err(InsertError::EmptyKey);
            };

        let mut dir_index = 0;
        for k in key_components {
            let num_dirs = self.dirs.len();

            let cur_dir = &mut self.dirs[dir_index];

            if let Some(node) = cur_dir.get(&k) {
                if let Node::DirIndex(i) = node {
                    dir_index = *i;
                } else {
                    // TODO Add the prefix to the error context.
                    return Err(InsertError::PrefixContainsValue);
                }
            } else {
                dir_index = num_dirs;
                cur_dir.insert(k, Node::DirIndex(dir_index));
                self.dirs.push(HashMap::new());
            }
        }

        let cur_dir = &mut self.dirs[dir_index];

        match cur_dir.entry(last) {
            Entry::Vacant(entry) => {
                entry.insert(Node::Value(value));

                Ok(())
            },
            Entry::Occupied(mut entry) => {
                if let Node::DirIndex(_) = entry.get() {
                    return Err(InsertError::DirAtKey);
                }
                entry.insert(Node::Value(value));

                Ok(())
            },
        }
    }

    /// Returns the prefix of `key` that leads to a value in `self`, if one
    /// exists, with the value found at that location; otherwise returns
    /// `None`.
    pub fn value_at_prefix<'a, 'b>(&'a self, key: &'b [K])
        -> Option<(Vec<&'b K>, &'a V)>
    {
        let mut dir_index = 0;

        let mut prefix = vec![];
        for k in key {
            prefix.push(k);

            let cur_dir = &self.dirs[dir_index];

            let node = cur_dir.get(&k)?;

            match node {
                Node::DirIndex(i) => {
                    dir_index = *i;
                },
                Node::Value(v) => {
                    return Some((prefix, v));
                },
            }
        }

        None
    }
}

#[derive(Debug, Snafu)]
pub enum InsertError {
    #[snafu(display("The key was empty"))]
    EmptyKey,
    #[snafu(display("There was a Trie \"directory\" at the key"))]
    DirAtKey,
    #[snafu(display("A value was encountered at a prefix of the key"))]
    PrefixContainsValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // Given (1) a new `Trie`
    // When `insert` is called with `a/b/c` as the key
    // Then (A) the result is `Ok`
    fn test_insert_into_new_trie() {
        // (1)
        let mut t = Trie::new();

        let result = t.insert(&['a', 'b', 'c'], 1);

        // (A)
        assert!(result.is_ok());
    }

    #[test]
    // Given (1) a `Trie` `t`
    // When `insert` is called with an empty slice as the key
    // Then (A) the result is `Err(InsertError::EmptyKey)`
    fn test_insert_with_empty_key_fails() {
        // (1)
        let mut t: Trie<u8, u8> = Trie::new();

        let result = t.insert(&[], 1);

        // (A)
        assert!(matches!(result, Err(InsertError::EmptyKey)));
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) a value was inserted into `t` at `a/b`
    // When `insert` is called with `a` as the key
    // Then (A) the result is `Err(InsertError::DirAtKey)`
    fn test_insert_at_dir_fails() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'b'], 1)
            .expect("couldn't insert value");

        let result = t.insert(&['a'], 1);

        // (A)
        assert!(matches!(result, Err(InsertError::DirAtKey)));
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) a value was inserted into `t` at `a/b/c`
    // When `insert` is called with `a/b` as the key
    // Then (A) the result is `Err(InsertError::DirAtKey)`
    fn test_insert_at_nested_dir_fails() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'b', 'c'], 1)
            .expect("couldn't insert value");

        let result = t.insert(&['a', 'b'], 1);

        // (A)
        assert!(matches!(result, Err(InsertError::DirAtKey)));
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) a value was inserted into `t` at `a/b`
    // When `insert` is called with `a/b/c` as the key
    // Then (A) the result is `Err(InsertError::PrefixContainsValue)`
    fn test_insert_past_value_node() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'b'], 1)
            .expect("couldn't insert value");

        let result = t.insert(&['a', 'b', 'c'], 1);

        // (A)
        assert!(matches!(result, Err(InsertError::PrefixContainsValue)));
    }

    #[test]
    // Given (1) a new `Trie`
    // When `value_at_prefix` is called
    // Then (A) the result is `None`
    fn test_value_at_prefix_on_new_trie() {
        // (1)
        let t: Trie<u8, u8> = Trie::new();

        let result = t.value_at_prefix(&[1, 2]);

        // (A)
        assert_eq!(result, None);
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) `1` was inserted into `t` at `a/b`
    // When `value_at_prefix` is called with an empty key
    // Then (A) the result is `None`
    fn test_value_at_prefix_with_empty_key() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'b'], 1)
            .expect("couldn't insert value");

        let result = t.value_at_prefix(&[]);

        // (A)
        assert_eq!(result, None);
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) `1` was inserted into `t` at `a/b`
    // When `value_at_prefix` is called with `a` as the key
    // Then (A) the result is `None`
    fn test_value_at_prefix_with_no_value_at_prefix() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'b'], 1)
            .expect("couldn't insert value");

        let result = t.value_at_prefix(&['a']);

        // (A)
        assert_eq!(result, None);
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) `1` was inserted into `t` at `a/b`
    // When `value_at_prefix` is called with `a/b` as the key
    // Then (A) the result contains a reference to `a/b` and `1`
    fn test_value_at_prefix_with_value_at_path() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'b'], 1)
            .expect("couldn't insert value");

        let result = t.value_at_prefix(&['a', 'b']);

        // (A)
        assert_eq!(result, Some((vec![&'a', &'b'], &1)));
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) `1` was inserted into `t` at `a/b`
    // When `value_at_prefix` is called with `a/b/c` as the key
    // Then (A) the result contains a reference to `a/b` and `1`
    fn test_value_at_prefix_with_value_at_prefix_of_path() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'b'], 1)
            .expect("couldn't insert value");

        let result = t.value_at_prefix(&['a', 'b', 'c']);

        // (A)
        assert_eq!(result, Some((vec![&'a', &'b'], &1)));
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) `1` was inserted into `t` at `a/b`
    // When `value_at_prefix` is called with `a/b/c/d` as the key
    // Then (A) the result contains a reference to `a/b` and `1`
    fn test_value_at_prefix_with_value_at_deeper_prefix_of_path() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'b'], 1)
            .expect("couldn't insert value");

        let result = t.value_at_prefix(&['a', 'b', 'c', 'd']);

        // (A)
        assert_eq!(result, Some((vec![&'a', &'b'], &1)));
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) `1` was inserted into `t` at `a/x`
    //     AND (3) `2` was inserted into `t` at `a/y`
    // When `value_at_prefix` is called with `a/x/a` as the key
    // Then (A) the result contains a reference to `a/x` and `1`
    fn test_value_at_prefix_first_of_two_paths() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'x'], 1)
            .expect("couldn't insert value");
        // (3)
        t.insert(&['a', 'y'], 2)
            .expect("couldn't insert value");

        let result = t.value_at_prefix(&['a', 'x', 'a']);

        // (A)
        assert_eq!(result, Some((vec![&'a', &'x'], &1)));
    }

    #[test]
    // Given (1) a `Trie` `t`
    //     AND (2) `1` was inserted into `t` at `a/x`
    //     AND (3) `2` was inserted into `t` at `a/y`
    // When `value_at_prefix` is called with `a/y/a` as the key
    // Then (A) the result contains a reference to `a/x` and `1`
    fn test_value_at_prefix_second_of_two_paths() {
        // (1)
        let mut t = Trie::new();
        // (2)
        t.insert(&['a', 'x'], 1)
            .expect("couldn't insert value");
        // (3)
        t.insert(&['a', 'y'], 2)
            .expect("couldn't insert value");

        let result = t.value_at_prefix(&['a', 'y', 'a']);

        // (A)
        assert_eq!(result, Some((vec![&'a', &'y'], &2)));
    }
}
