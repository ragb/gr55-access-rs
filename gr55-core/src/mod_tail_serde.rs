//! Custom serde for [`crate::patch::Mod::raw_tail`] — same grouping
//! pattern as [`crate::mfx_tail_serde`], applied to the MOD block on
//! page 0x07. Bytes are emitted as a two-level map keyed by owning
//! effect type ([`ModTypeOwner`]) → parameter name.
//!
//! ```yaml
//! modulation:
//!   mod_type: distortion
//!   raw_tail:
//!     distortion:                # ACTIVE
//!       Distortion Drive: 90
//!       Distortion Tone: 60
//!     wah:                       # dormant but persisted
//!       Wah Mode: 0
//!       Wah Sens: 50
//!     ...
//!     unmapped:                  # offsets w/o a named param
//!       "0x14": 0
//! ```

use std::collections::BTreeMap;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::mod_params::{ModTypeOwner, MOD_PARAMS};

const UNMAPPED_KEY: &str = "unmapped";

pub fn serialize<S: Serializer>(map: &BTreeMap<u8, u8>, ser: S) -> Result<S::Ok, S::Error> {
    let mut grouped: BTreeMap<String, BTreeMap<String, u8>> = BTreeMap::new();
    for (&off, &b) in map {
        let (group_key, inner_key) = classify(off);
        grouped.entry(group_key).or_default().insert(inner_key, b);
    }
    grouped.serialize(ser)
}

pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<BTreeMap<u8, u8>, D::Error> {
    let grouped: BTreeMap<String, BTreeMap<String, u8>> = BTreeMap::deserialize(de)?;
    let mut out = BTreeMap::new();
    for (group, inner) in &grouped {
        for (inner_key, &byte) in inner {
            let off = resolve_inner_key(group, inner_key).ok_or_else(|| {
                D::Error::custom(format!(
                    "unknown MOD param key {inner_key:?} under group {group:?}: \
                     not a documented parameter name and not a parseable hex offset"
                ))
            })?;
            out.insert(off, byte);
        }
    }
    Ok(out)
}

fn classify(off: u8) -> (String, String) {
    let entry = MOD_PARAMS.iter().find(|e| e.offset == off);
    match entry {
        Some(e) if e.owning_type.is_some() && !e.name.is_empty() => (
            e.owning_type.unwrap().as_snake().to_string(),
            e.name.to_string(),
        ),
        Some(e) if !e.name.is_empty() => (UNMAPPED_KEY.to_string(), e.name.to_string()),
        _ => (UNMAPPED_KEY.to_string(), format!("0x{off:02X}")),
    }
}

fn resolve_inner_key(group: &str, inner: &str) -> Option<u8> {
    let trimmed = inner.trim();
    if group == UNMAPPED_KEY {
        if let Some(off) = parse_hex_u8(trimmed) {
            return Some(off);
        }
        return find_name(None, trimmed);
    }
    let owner = ModTypeOwner::from_snake(group)?;
    if let Some(off) = find_name(Some(owner), trimmed) {
        return Some(off);
    }
    parse_hex_u8(trimmed)
}

fn parse_hex_u8(s: &str) -> Option<u8> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    u8::from_str_radix(hex, 16).ok()
}

fn find_name(owner: Option<ModTypeOwner>, name: &str) -> Option<u8> {
    for entry in MOD_PARAMS.iter() {
        if entry.name.is_empty() {
            continue;
        }
        if entry.owning_type != owner {
            continue;
        }
        if entry.name == name {
            return Some(entry.offset);
        }
    }
    None
}

/// Custom schema describing the actual YAML shape:
/// `Map<EffectTypeOrCategory, Map<ParamName, byte>>` rather than the
/// raw `BTreeMap<u16, u8>` wire form.
#[cfg(feature = "schema")]
pub fn schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    use std::collections::BTreeMap;
    <BTreeMap<String, BTreeMap<String, u8>> as schemars::JsonSchema>::json_schema(gen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Mod;

    #[test]
    fn classify_picks_owning_type_for_known_bytes() {
        // Distortion Drive at offset 0x19 (per the previous
        // resolve_key_known_names test).
        let (g, n) = classify(0x19);
        assert_eq!(g, "distortion");
        assert_eq!(n, "Distortion Drive");
        // Wah Mode at 0x1C.
        let (g, n) = classify(0x1C);
        assert_eq!(g, "wah");
        assert_eq!(n, "Wah Mode");
    }

    #[test]
    fn yaml_round_trip_groups_by_effect_type() {
        let mut modu = Mod::default();
        modu.raw_tail.insert(0x19, 90); // Distortion Drive
        modu.raw_tail.insert(0x1A, 60); // Distortion Tone
        modu.raw_tail.insert(0x1C, 0); // Wah Mode
        let yaml = serde_yaml::to_string(&modu).unwrap();
        eprintln!("{yaml}");
        assert!(yaml.contains("distortion:"));
        assert!(yaml.contains("Distortion Drive: 90"));
        assert!(yaml.contains("wah:"));
        assert!(yaml.contains("Wah Mode: 0"));

        let back: Mod = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.raw_tail.get(&0x19), Some(&90));
        assert_eq!(back.raw_tail.get(&0x1A), Some(&60));
        assert_eq!(back.raw_tail.get(&0x1C), Some(&0));
    }
}
