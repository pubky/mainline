//! Simple Lru cache implementation

use std::{collections::HashMap, num::NonZeroUsize};

use crate::common::Id;

#[derive(Debug)]
pub struct Lru<V> {
    capacity: usize,
    map: HashMap<Id, Node<V>>,
    head: Option<Id>,
    tail: Option<Id>,
}

#[derive(Debug)]
struct Node<V> {
    target: Id,
    value: V,
    prev: Option<Id>,
    next: Option<Id>,
}

impl<V: std::fmt::Debug> Lru<V> {
    fn new(capacity: NonZeroUsize) -> Self {
        let capacity = capacity.into();

        Self {
            capacity,
            map: HashMap::with_capacity(capacity),
            head: None,
            tail: None,
        }
    }

    fn len(&self) -> usize {
        self.map.len()
    }

    // === Public Methods ===

    fn get(&mut self, target: Id) -> Option<&V> {
        self.map.get(&target).map(|n| &n.value)
    }

    fn insert(&mut self, target: Id, value: V) {
        if self.map.contains_key(&target) {
            // Update the value if the key exists
            self.map.get_mut(&target).unwrap().value = value;
        } else {
            // Insert a new node

            if self.map.len() == self.capacity {
                // Evict the least recently used item (tail)
                self.pop();
            }

            // Insert the new node at the head
            self.push(target, value);
        }
    }

    // === Private Methods ===

    fn pop(&mut self) {
        let current_tail_id = self.tail.take().unwrap();
        let current_tail = self.map.remove(&current_tail_id).unwrap();
        self.tail = current_tail.prev;
    }

    fn push(&mut self, target: Id, value: V) {
        let mut new_node = Node {
            target,
            value,
            prev: None,
            next: None,
        };

        // Insert the new node at the head
        if let Some(id) = self.head.take() {
            let mut head_node = self.map.get_mut(&id).unwrap();
            head_node.prev = Some(new_node.target);
            new_node.next = Some(id);
        };

        self.map.insert(target, new_node);

        self.head = Some(target);

        if self.tail.is_none() {
            self.tail = Some(target)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn lru() {
        let mut cache = Lru::new(NonZeroUsize::new(3).unwrap());

        let info_hash_a = Id::random();
        let info_hash_b = Id::random();
        let info_hash_c = Id::random();
        let info_hash_d = Id::random();

        cache.insert(info_hash_a, "a");
        cache.insert(info_hash_b, "b");
        cache.insert(info_hash_c, "c");
        cache.insert(info_hash_d, "d");

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get(info_hash_a), None);

        let mut existing = cache.map.values().map(|n| n.value).collect::<Vec<_>>();
        existing.sort();

        assert_eq!(existing, vec!["b", "c", "d"])
    }
}
