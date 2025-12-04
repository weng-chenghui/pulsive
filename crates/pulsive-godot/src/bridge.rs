//! Type conversion between Pulsive and Godot types

use godot::prelude::*;
use pulsive_core::{Value, ValueMap};

/// Convert a Pulsive Value to a Godot Variant
pub fn value_to_variant(value: &Value) -> Variant {
    match value {
        Value::Null => Variant::nil(),
        Value::Bool(b) => b.to_variant(),
        Value::Int(i) => i.to_variant(),
        Value::Float(f) => f.to_variant(),
        Value::String(s) => s.to_variant(),
        Value::EntityRef(id) => (id.raw() as i64).to_variant(),
        Value::List(list) => {
            let mut arr = Array::new();
            for item in list {
                arr.push(&value_to_variant(item));
            }
            arr.to_variant()
        }
        Value::Map(map) => value_map_to_dict(map).to_variant(),
    }
}

/// Convert a Godot Variant to a Pulsive Value
pub fn variant_to_value(variant: &Variant) -> Value {
    match variant.get_type() {
        VariantType::NIL => Value::Null,
        VariantType::BOOL => Value::Bool(variant.to::<bool>()),
        VariantType::INT => Value::Int(variant.to::<i64>()),
        VariantType::FLOAT => Value::Float(variant.to::<f64>()),
        VariantType::STRING => Value::String(variant.to::<GString>().to_string()),
        VariantType::ARRAY => {
            let arr = variant.to::<Array<Variant>>();
            let list: Vec<Value> = arr.iter_shared()
                .map(|v| variant_to_value(&v))
                .collect();
            Value::List(list)
        }
        VariantType::DICTIONARY => {
            let dict = variant.to::<Dictionary>();
            Value::Map(dict_to_value_map(&dict))
        }
        _ => {
            // Try to convert to string for unknown types
            Value::String(format!("{:?}", variant))
        }
    }
}

/// Convert a ValueMap to a Godot Dictionary
pub fn value_map_to_dict(map: &ValueMap) -> Dictionary {
    let mut dict = Dictionary::new();
    for (key, value) in map {
        dict.set(key.clone(), value_to_variant(value));
    }
    dict
}

/// Convert a Godot Dictionary to a ValueMap
pub fn dict_to_value_map(dict: &Dictionary) -> ValueMap {
    let mut map = ValueMap::new();
    for (key, value) in dict.iter_shared() {
        let key_str = key.to::<GString>().to_string();
        map.insert(key_str, variant_to_value(&value));
    }
    map
}
