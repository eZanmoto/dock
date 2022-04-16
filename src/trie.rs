// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;

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

        let mut index = 0;
        for k in key_components {
            let num_dirs = self.dirs.len();

            let cur_dir = &mut self.dirs[index];

            if let Some(node) = cur_dir.get(&k) {
                if let Node::DirIndex(i) = node {
                    index = *i;
                } else {
                    // TODO Add the prefix to the error context.
                    return Err(InsertError::PrefixContainsValue);
                }
            } else {
                index = num_dirs;
                cur_dir.insert(k, Node::DirIndex(index));
                self.dirs.push(HashMap::new());
            }
        }

        let cur_dir = &mut self.dirs[index];

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
}

#[derive(Debug)]
pub enum InsertError {
    EmptyKey,
    DirAtKey,
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
}
