//! Generational Arena Pool for fixed-capacity, zero-allocation object management.
//!
//! Inspired by Fyrox's `Pool` and `Handle` architecture, adapted for `no_std`
//! embedded environments with strict compile-time capacity caps.
//!
//! # Example
//! ```
//! use embedded_3dgfx::pool::{Handle, Pool};
//!
//! struct Entity {
//!     name: &'static str,
//!     hp: i32,
//! }
//!
//! let mut pool: Pool<Entity, 16> = Pool::new();
//! let h1 = pool.spawn(Entity { name: "Player", hp: 100 }).unwrap();
//! let h2 = pool.spawn(Entity { name: "Enemy", hp: 50 }).unwrap();
//!
//! assert_eq!(pool[h1].hp, 100);
//!
//! pool.free(h2).unwrap();
//! assert!(!pool.is_valid_handle(h2));
//! ```

use core::fmt;
use core::marker::PhantomData;
use core::ops::{Index, IndexMut};

/// A generational handle to an object stored in a [`Pool`].
///
/// Contains a slot index and a generation number to prevent stale reference /
/// use-after-free bugs.
pub struct Handle<T> {
    pub(crate) index: u32,
    pub(crate) generation: u32,
    pub(crate) _marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Handle<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T> Eq for Handle<T> {}

impl<T> PartialOrd for Handle<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Handle<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.index
            .cmp(&other.index)
            .then_with(|| self.generation.cmp(&other.generation))
    }
}

impl<T> core::hash::Hash for Handle<T> {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

impl<T> Handle<T> {
    /// A sentinel `None` handle that never references a valid object.
    pub const NONE: Self = Self {
        index: u32::MAX,
        generation: 0,
        _marker: PhantomData,
    };

    /// Create a new handle with a given raw index and generation.
    #[inline]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self {
            index,
            generation,
            _marker: PhantomData,
        }
    }

    /// Check if this is the `NONE` handle.
    #[inline]
    pub const fn is_none(&self) -> bool {
        self.index == u32::MAX
    }

    /// Check if this handle is not `NONE`.
    #[inline]
    pub const fn is_some(&self) -> bool {
        !self.is_none()
    }

    /// Returns the raw slot index.
    #[inline]
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Returns the generation number.
    #[inline]
    pub const fn generation(&self) -> u32 {
        self.generation
    }
}

impl<T> Default for Handle<T> {
    #[inline]
    fn default() -> Self {
        Self::NONE
    }
}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_none() {
            write!(f, "Handle::NONE")
        } else {
            f.debug_struct("Handle")
                .field("index", &self.index)
                .field("generation", &self.generation)
                .finish()
        }
    }
}

enum PoolEntry<T> {
    Occupied {
        value: T,
        generation: u32,
    },
    Vacant {
        next_free: Option<usize>,
        generation: u32,
    },
}

/// A fixed-capacity, zero-allocation generational arena pool.
///
/// `CAP` specifies the maximum number of items that can be allocated concurrently.
pub struct Pool<T, const CAP: usize> {
    entries: [Option<PoolEntry<T>>; CAP],
    free_head: Option<usize>,
    len: usize,
}

impl<T, const CAP: usize> Pool<T, CAP> {
    /// Create a new empty pool.
    pub fn new() -> Self {
        const {
            assert!(CAP <= u32::MAX as usize, "Pool capacity exceeds u32::MAX");
        }

        // Initialize all slots as Vacant linked in a freelist
        let mut entries: [Option<PoolEntry<T>>; CAP] = [const { None }; CAP];
        for i in 0..CAP {
            let next_free = if i + 1 < CAP { Some(i + 1) } else { None };
            entries[i] = Some(PoolEntry::Vacant {
                next_free,
                generation: 1,
            });
        }

        Self {
            entries,
            free_head: if CAP > 0 { Some(0) } else { None },
            len: 0,
        }
    }

    /// Number of active items in the pool.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns true if no items are currently allocated.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Total capacity of the pool.
    #[inline]
    pub const fn capacity(&self) -> usize {
        CAP
    }

    /// Spawn a new object into the pool, returning its generational handle.
    ///
    /// Returns `None` if the pool is full.
    pub fn spawn(&mut self, value: T) -> Option<Handle<T>> {
        let index = self.free_head?;

        let entry = self.entries[index].as_mut()?;
        let generation = match entry {
            PoolEntry::Vacant {
                next_free,
                generation,
            } => {
                let current_gen = *generation;
                self.free_head = *next_free;
                current_gen
            }
            PoolEntry::Occupied { .. } => return None,
        };

        self.entries[index] = Some(PoolEntry::Occupied { value, generation });
        self.len += 1;

        Some(Handle::new(index as u32, generation))
    }

    /// Check if a handle is currently valid (points to an alive object with matching generation).
    #[inline]
    pub fn is_valid_handle(&self, handle: Handle<T>) -> bool {
        if handle.is_none() || (handle.index as usize) >= CAP {
            return false;
        }

        match &self.entries[handle.index as usize] {
            Some(PoolEntry::Occupied { generation, .. }) => *generation == handle.generation,
            _ => false,
        }
    }

    /// Free an object referenced by the handle.
    ///
    /// Returns the freed object, or `None` if the handle was invalid.
    pub fn free(&mut self, handle: Handle<T>) -> Option<T> {
        if !self.is_valid_handle(handle) {
            return None;
        }

        let index = handle.index as usize;
        let old_entry = self.entries[index].take()?;

        match old_entry {
            PoolEntry::Occupied { value, generation } => {
                // Increment generation to invalidate existing handles (wrapping on overflow)
                let new_gen = generation.wrapping_add(1).max(1);
                self.entries[index] = Some(PoolEntry::Vacant {
                    next_free: self.free_head,
                    generation: new_gen,
                });
                self.free_head = Some(index);
                self.len = self.len.saturating_sub(1);
                Some(value)
            }
            PoolEntry::Vacant { .. } => None,
        }
    }

    /// Borrow an object by handle.
    #[inline]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        if !self.is_valid_handle(handle) {
            return None;
        }

        match &self.entries[handle.index as usize] {
            Some(PoolEntry::Occupied { value, .. }) => Some(value),
            _ => None,
        }
    }

    /// Mutably borrow an object by handle.
    #[inline]
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        if !self.is_valid_handle(handle) {
            return None;
        }

        match &mut self.entries[handle.index as usize] {
            Some(PoolEntry::Occupied { value, .. }) => Some(value),
            _ => None,
        }
    }

    /// Iterate over active (handle, &value) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.entries.iter().enumerate().filter_map(|(idx, entry)| {
            if let Some(PoolEntry::Occupied { value, generation }) = entry {
                Some((Handle::new(idx as u32, *generation), value))
            } else {
                None
            }
        })
    }

    /// Iterate over active (handle, &mut value) pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Handle<T>, &mut T)> {
        self.entries
            .iter_mut()
            .enumerate()
            .filter_map(|(idx, entry)| {
                if let Some(PoolEntry::Occupied { value, generation }) = entry {
                    Some((Handle::new(idx as u32, *generation), value))
                } else {
                    None
                }
            })
    }

    /// Clear all items from the pool.
    pub fn clear(&mut self) {
        for i in 0..CAP {
            let next_gen = match &self.entries[i] {
                Some(PoolEntry::Occupied { generation, .. }) => generation.wrapping_add(1).max(1),
                Some(PoolEntry::Vacant { generation, .. }) => *generation,
                None => 1,
            };
            let next_free = if i + 1 < CAP { Some(i + 1) } else { None };
            self.entries[i] = Some(PoolEntry::Vacant {
                next_free,
                generation: next_gen,
            });
        }
        self.free_head = if CAP > 0 { Some(0) } else { None };
        self.len = 0;
    }
}

impl<T, const CAP: usize> Default for Pool<T, CAP> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const CAP: usize> Index<Handle<T>> for Pool<T, CAP> {
    type Output = T;

    #[inline]
    fn index(&self, handle: Handle<T>) -> &Self::Output {
        self.get(handle).expect("invalid pool handle index")
    }
}

impl<T, const CAP: usize> IndexMut<Handle<T>> for Pool<T, CAP> {
    #[inline]
    fn index_mut(&mut self, handle: Handle<T>) -> &mut Self::Output {
        self.get_mut(handle).expect("invalid pool handle index")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_spawn_get_free() {
        let mut pool: Pool<&'static str, 4> = Pool::new();
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.capacity(), 4);

        let h1 = pool.spawn("first").unwrap();
        let h2 = pool.spawn("second").unwrap();
        assert_eq!(pool.len(), 2);

        assert_eq!(pool[h1], "first");
        assert_eq!(pool[h2], "second");

        let freed = pool.free(h1);
        assert_eq!(freed, Some("first"));
        assert!(!pool.is_valid_handle(h1));
        assert_eq!(pool.get(h1), None);
        assert_eq!(pool.len(), 1);

        // Reusing slot should yield new generation
        let h3 = pool.spawn("third").unwrap();
        assert_eq!(h3.index(), h1.index());
        assert_ne!(h3.generation(), h1.generation());
        assert_eq!(pool[h3], "third");
        assert!(!pool.is_valid_handle(h1));
    }

    #[test]
    fn test_pool_full_rejection() {
        let mut pool: Pool<i32, 2> = Pool::new();
        let h1 = pool.spawn(10).unwrap();
        let h2 = pool.spawn(20).unwrap();
        assert!(pool.spawn(30).is_none());

        pool.free(h1);
        let h3 = pool.spawn(30).unwrap();
        assert_eq!(pool[h3], 30);
        assert_eq!(pool[h2], 20);
    }

    #[test]
    fn test_pool_iterators() {
        let mut pool: Pool<u32, 4> = Pool::new();
        let h1 = pool.spawn(100).unwrap();
        let h2 = pool.spawn(200).unwrap();

        let mut sum = 0;
        for (_, val) in pool.iter() {
            sum += *val;
        }
        assert_eq!(sum, 300);

        for (_, val) in pool.iter_mut() {
            *val += 1;
        }

        assert_eq!(pool[h1], 101);
        assert_eq!(pool[h2], 201);
    }

    #[test]
    fn test_pool_empty_invalid_handles_mut_and_clear() {
        let mut pool: Pool<i32, 0> = Pool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.capacity(), 0);
        assert!(pool.spawn(1).is_none());

        let mut pool2: Pool<i32, 2> = Pool::new();
        assert!(pool2.get(Handle::new(0, 1)).is_none());
        assert!(pool2.get_mut(Handle::new(0, 1)).is_none());
        assert!(pool2.free(Handle::new(0, 1)).is_none());
        assert!(!pool2.is_valid_handle(Handle::new(0, 1)));

        let h = pool2.spawn(42).unwrap();
        assert!(pool2.is_valid_handle(h));
        *pool2.get_mut(h).unwrap() = 43;
        assert_eq!(pool2.get(h), Some(&43));

        let freed = pool2.free(h).unwrap();
        assert_eq!(freed, 43);
        assert!(pool2.free(h).is_none());
        assert!(pool2.get_mut(h).is_none());
        pool2.spawn(7).unwrap();
        pool2.clear();
        assert!(pool2.is_empty());
        assert_eq!(pool2.len(), 0);
    }
}
