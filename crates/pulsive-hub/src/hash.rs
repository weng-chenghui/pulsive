//! Deterministic Hashing for Partitioning
//!
//! This module provides deterministic hash functions for partitioning entities
//! across cores. All hashing is seed-based and uses the same mixing function
//! as the hub's RNG seeding ([`crate::hash_seed`]).
//!
//! # Determinism
//!
//! Unlike `std::collections::hash_map::DefaultHasher` which uses random keys,
//! these functions produce the same output for the same inputs across runs
//! and platforms, making partition assignments reproducible.
//!
//! # Seed Configuration
//!
//! All hash functions accept a seed parameter, allowing partition layouts to
//! be controlled deterministically. Use [`crate::DEFAULT_GLOBAL_SEED`] for
//! default behavior, or provide a custom seed via [`crate::HubConfig::global_seed()`].
//!
//! # Example
//!
//! ```
//! use pulsive_hub::hash::{hash_u64_with_seed, hash_bytes_with_seed, hash_value_with_seed};
//! use pulsive_hub::DEFAULT_GLOBAL_SEED;
//! use pulsive_core::Value;
//!
//! // Hash a u64 value
//! let h1 = hash_u64_with_seed(42, DEFAULT_GLOBAL_SEED);
//! let h2 = hash_u64_with_seed(42, DEFAULT_GLOBAL_SEED);
//! assert_eq!(h1, h2); // Deterministic
//!
//! // Hash bytes
//! let h = hash_bytes_with_seed(b"hello", DEFAULT_GLOBAL_SEED);
//!
//! // Hash a Value
//! let v = Value::String("france".into());
//! let h = hash_value_with_seed(&v, DEFAULT_GLOBAL_SEED);
//! ```

use crate::config::hash_seed;
use pulsive_core::Value;

// Type discriminators for Value hashing.
// Using hash_seed with different "slot" values ensures type-specific mixing.
const TYPE_NULL: u64 = 0;
const TYPE_BOOL: u64 = 1;
const TYPE_INT: u64 = 2;
const TYPE_FLOAT: u64 = 3;
const TYPE_STRING: u64 = 4;
const TYPE_ENTITY_REF: u64 = 5;
const TYPE_LIST: u64 = 6;
const TYPE_MAP: u64 = 7;

/// Hash a u64 value with a seed
///
/// Uses the hub's [`hash_seed`] mixing function to combine the seed
/// with the value, producing a deterministic hash.
///
/// # Arguments
///
/// * `value` - The u64 value to hash
/// * `seed` - The seed for deterministic hashing (use [`crate::DEFAULT_GLOBAL_SEED`] for default)
///
/// # Example
///
/// ```
/// use pulsive_hub::hash::hash_u64_with_seed;
/// use pulsive_hub::DEFAULT_GLOBAL_SEED;
///
/// let h1 = hash_u64_with_seed(42, DEFAULT_GLOBAL_SEED);
/// let h2 = hash_u64_with_seed(42, DEFAULT_GLOBAL_SEED);
/// assert_eq!(h1, h2);
///
/// // Different values produce different hashes
/// let h3 = hash_u64_with_seed(43, DEFAULT_GLOBAL_SEED);
/// assert_ne!(h1, h3);
/// ```
pub fn hash_u64_with_seed(value: u64, seed: u64) -> u64 {
    // Use hash_seed with value in the "core_id" slot and 0 in the "tick" slot
    hash_seed(seed, value, 0)
}

/// Hash a byte slice with a seed
///
/// Uses FNV-1a inspired hashing with the seed as the initial state.
/// The result is further mixed using [`hash_seed`] for better distribution.
///
/// # Arguments
///
/// * `bytes` - The byte slice to hash
/// * `seed` - The seed for deterministic hashing
///
/// # Example
///
/// ```
/// use pulsive_hub::hash::hash_bytes_with_seed;
/// use pulsive_hub::DEFAULT_GLOBAL_SEED;
///
/// let h1 = hash_bytes_with_seed(b"hello", DEFAULT_GLOBAL_SEED);
/// let h2 = hash_bytes_with_seed(b"hello", DEFAULT_GLOBAL_SEED);
/// assert_eq!(h1, h2);
/// ```
pub fn hash_bytes_with_seed(bytes: &[u8], seed: u64) -> u64 {
    // FNV-1a inspired hash with seed as initial value
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut h = seed;
    for (i, &b) in bytes.iter().enumerate() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
        // Periodically mix with hash_seed to maintain good distribution
        if i % 8 == 7 {
            h = hash_seed(seed, h, i as u64);
        }
    }
    // Final mix
    hash_seed(seed, h, bytes.len() as u64)
}

/// Hash a [`Value`] with a seed
///
/// Produces a deterministic hash for any pulsive [`Value`], including
/// nested lists and maps. Each value type is tagged with a discriminator
/// to ensure different types produce different hashes (e.g., `Int(0)` vs `Bool(false)`).
///
/// # Arguments
///
/// * `value` - The Value to hash
/// * `seed` - The seed for deterministic hashing
///
/// # Note on Maps
///
/// **Map hashing uses sorted keys** to ensure the hash is independent of
/// insertion order. This means maps with the same key-value pairs will
/// produce the same hash regardless of how they were built.
///
/// # Example
///
/// ```
/// use pulsive_hub::hash::hash_value_with_seed;
/// use pulsive_hub::DEFAULT_GLOBAL_SEED;
/// use pulsive_core::Value;
///
/// let v1 = Value::String("france".into());
/// let v2 = Value::String("france".into());
/// assert_eq!(
///     hash_value_with_seed(&v1, DEFAULT_GLOBAL_SEED),
///     hash_value_with_seed(&v2, DEFAULT_GLOBAL_SEED)
/// );
///
/// // Different values produce different hashes
/// let v3 = Value::String("england".into());
/// assert_ne!(
///     hash_value_with_seed(&v1, DEFAULT_GLOBAL_SEED),
///     hash_value_with_seed(&v3, DEFAULT_GLOBAL_SEED)
/// );
/// ```
pub fn hash_value_with_seed(value: &Value, seed: u64) -> u64 {
    match value {
        Value::Null => {
            // Mix type discriminator with seed
            hash_seed(seed, TYPE_NULL, 0)
        }
        Value::Bool(b) => {
            let val = if *b { 1 } else { 0 };
            // Mix type, then mix value
            let h = hash_seed(seed, TYPE_BOOL, 0);
            hash_seed(h, val, 1)
        }
        Value::Int(i) => {
            let h = hash_seed(seed, TYPE_INT, 0);
            hash_seed(h, *i as u64, 1)
        }
        Value::Float(f) => {
            let h = hash_seed(seed, TYPE_FLOAT, 0);
            hash_seed(h, f.to_bits(), 1)
        }
        Value::String(s) => {
            let h = hash_seed(seed, TYPE_STRING, 0);
            // Mix in the string bytes hash
            let string_hash = hash_bytes_with_seed(s.as_bytes(), h);
            hash_seed(h, string_hash, 1)
        }
        Value::EntityRef(id) => {
            let h = hash_seed(seed, TYPE_ENTITY_REF, 0);
            hash_seed(h, id.raw(), 1)
        }
        Value::List(list) => {
            let mut h = hash_seed(seed, TYPE_LIST, 0);
            for (i, v) in list.iter().enumerate() {
                let elem_hash = hash_value_with_seed(v, h);
                h = hash_seed(h, elem_hash, i as u64 + 1);
            }
            h
        }
        Value::Map(map) => {
            // Sort keys to ensure hash is order-independent
            let mut h = hash_seed(seed, TYPE_MAP, 0);
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for (i, k) in keys.into_iter().enumerate() {
                let v = map.get(k).unwrap();
                let key_hash = hash_bytes_with_seed(k.as_bytes(), h);
                let val_hash = hash_value_with_seed(v, h);
                h = hash_seed(h, key_hash, i as u64 * 2 + 1);
                h = hash_seed(h, val_hash, i as u64 * 2 + 2);
            }
            h
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_GLOBAL_SEED;

    #[test]
    fn test_hash_u64_deterministic() {
        let h1 = hash_u64_with_seed(42, DEFAULT_GLOBAL_SEED);
        let h2 = hash_u64_with_seed(42, DEFAULT_GLOBAL_SEED);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_u64_different_values() {
        let h1 = hash_u64_with_seed(0, DEFAULT_GLOBAL_SEED);
        let h2 = hash_u64_with_seed(1, DEFAULT_GLOBAL_SEED);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_u64_different_seeds() {
        let h1 = hash_u64_with_seed(42, 100);
        let h2 = hash_u64_with_seed(42, 200);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_bytes_deterministic() {
        let h1 = hash_bytes_with_seed(b"hello", DEFAULT_GLOBAL_SEED);
        let h2 = hash_bytes_with_seed(b"hello", DEFAULT_GLOBAL_SEED);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_bytes_different_values() {
        let h1 = hash_bytes_with_seed(b"hello", DEFAULT_GLOBAL_SEED);
        let h2 = hash_bytes_with_seed(b"world", DEFAULT_GLOBAL_SEED);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_bytes_different_seeds() {
        let h1 = hash_bytes_with_seed(b"hello", 100);
        let h2 = hash_bytes_with_seed(b"hello", 200);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_bytes_empty() {
        let h1 = hash_bytes_with_seed(b"", DEFAULT_GLOBAL_SEED);
        let h2 = hash_bytes_with_seed(b"", DEFAULT_GLOBAL_SEED);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_value_deterministic() {
        let v1 = Value::String("test".into());
        let v2 = Value::String("test".into());
        assert_eq!(
            hash_value_with_seed(&v1, DEFAULT_GLOBAL_SEED),
            hash_value_with_seed(&v2, DEFAULT_GLOBAL_SEED)
        );
    }

    #[test]
    fn test_hash_value_different_values() {
        let v1 = Value::String("france".into());
        let v2 = Value::String("england".into());
        assert_ne!(
            hash_value_with_seed(&v1, DEFAULT_GLOBAL_SEED),
            hash_value_with_seed(&v2, DEFAULT_GLOBAL_SEED)
        );
    }

    #[test]
    fn test_hash_value_different_seeds() {
        let v = Value::String("test".into());
        let h1 = hash_value_with_seed(&v, 100);
        let h2 = hash_value_with_seed(&v, 200);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_value_type_discrimination() {
        // Different types with "equivalent" values should hash differently
        assert_ne!(
            hash_value_with_seed(&Value::Int(0), DEFAULT_GLOBAL_SEED),
            hash_value_with_seed(&Value::Bool(false), DEFAULT_GLOBAL_SEED)
        );
        assert_ne!(
            hash_value_with_seed(&Value::Int(1), DEFAULT_GLOBAL_SEED),
            hash_value_with_seed(&Value::Bool(true), DEFAULT_GLOBAL_SEED)
        );
        assert_ne!(
            hash_value_with_seed(&Value::Null, DEFAULT_GLOBAL_SEED),
            hash_value_with_seed(&Value::Int(0), DEFAULT_GLOBAL_SEED)
        );
    }

    #[test]
    fn test_hash_value_all_types() {
        use pulsive_core::{EntityId, IndexMap};

        // Test all value types are hashable and deterministic
        let mut map = IndexMap::new();
        map.insert("a".to_string(), Value::Int(1));
        map.insert("b".to_string(), Value::Int(2));

        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Bool(false),
            Value::Int(42),
            Value::Int(-1),
            Value::Float(3.14),
            Value::Float(0.0),
            Value::String("hello".into()),
            Value::String("".into()),
            Value::EntityRef(EntityId::new(123)),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
            Value::List(vec![]),
            Value::Map(map),
        ];

        for v in &values {
            let h1 = hash_value_with_seed(v, DEFAULT_GLOBAL_SEED);
            let h2 = hash_value_with_seed(v, DEFAULT_GLOBAL_SEED);
            assert_eq!(h1, h2, "Value {:?} should hash deterministically", v);
        }
    }

    #[test]
    fn test_hash_value_nested() {
        use pulsive_core::IndexMap;

        let mut map = IndexMap::new();
        map.insert("x".to_string(), Value::Float(1.0));
        map.insert("y".to_string(), Value::Float(2.0));

        let nested = Value::List(vec![Value::Map(map), Value::String("test".into())]);

        let h1 = hash_value_with_seed(&nested, DEFAULT_GLOBAL_SEED);
        let h2 = hash_value_with_seed(&nested, DEFAULT_GLOBAL_SEED);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_value_map_order_independent() {
        use pulsive_core::IndexMap;

        // Build two maps with the same key-value pairs but different insertion order
        let mut map1 = IndexMap::new();
        map1.insert("a".to_string(), Value::Int(1));
        map1.insert("b".to_string(), Value::Int(2));
        map1.insert("c".to_string(), Value::Int(3));

        let mut map2 = IndexMap::new();
        map2.insert("c".to_string(), Value::Int(3));
        map2.insert("a".to_string(), Value::Int(1));
        map2.insert("b".to_string(), Value::Int(2));

        // They should have the same hash because we sort keys before hashing
        let h1 = hash_value_with_seed(&Value::Map(map1), DEFAULT_GLOBAL_SEED);
        let h2 = hash_value_with_seed(&Value::Map(map2), DEFAULT_GLOBAL_SEED);
        assert_eq!(
            h1, h2,
            "Maps with same key-value pairs should hash identically regardless of insertion order"
        );
    }
}
