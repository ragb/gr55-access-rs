//! Custom serde for [`crate::patch::Mfx::raw_tail`] that emits and
//! parses the type-specific MFX bytes as a **two-level map** grouped by
//! owning effect type (the active one *and* every dormant one whose
//! bytes the device still holds in TEMP RAM).
//!
//! The device's MFX block stores every effect type's parameters
//! simultaneously, so switching `mfx_type` preserves the dormant
//! types' state. The flat-map representation we used before exposed
//! all ~200 bytes side-by-side; this grouped form separates them
//! visually:
//!
//! ```yaml
//! mfx:
//!   mfx_type: super_filter
//!   raw_tail:
//!     super_filter:                    # ACTIVE — these bytes are live
//!       Super Filter Type: 1
//!       Super Filter Cutoff: 64
//!     phaser:                          # dormant but persisted
//!       Phaser Rate: 0
//!       Phaser Depth: 0
//!     ...
//!     unmapped:                        # offsets w/o a named param
//!       "0x4F": 0
//! ```
//!
//! All bytes round-trip — wire layout is identical to the flat form.

use std::collections::BTreeMap;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::mfx_params::{MfxTypeOwner, MFX_PARAMS};

/// Key used for bytes whose `MfxParamEntry` has no `owning_type`
/// (placeholder slots / FB-undocumented offsets that ended up in
/// `raw_tail`). The bytes inside are keyed by `"0xNN"` hex strings.
const UNMAPPED_KEY: &str = "unmapped";

pub fn serialize<S: Serializer>(map: &BTreeMap<u16, u8>, ser: S) -> Result<S::Ok, S::Error> {
    // Outer: owning-type snake key → inner map. Inner: param-name (or
    // `0xNN` hex for unmapped offsets) → byte value.
    let mut grouped: BTreeMap<String, BTreeMap<String, u8>> = BTreeMap::new();
    for (&lin, &b) in map {
        let (group_key, inner_key) = classify(lin);
        grouped.entry(group_key).or_default().insert(inner_key, b);
    }
    grouped.serialize(ser)
}

pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<BTreeMap<u16, u8>, D::Error> {
    let grouped: BTreeMap<String, BTreeMap<String, u8>> = BTreeMap::deserialize(de)?;
    let mut out = BTreeMap::new();
    for (group, inner) in &grouped {
        for (inner_key, &byte) in inner {
            let lin = resolve_inner_key(group, inner_key).ok_or_else(|| {
                D::Error::custom(format!(
                    "unknown MFX param key {inner_key:?} under group {group:?}: \
                     not a documented parameter name and not a parseable hex offset"
                ))
            })?;
            out.insert(lin, byte);
        }
    }
    Ok(out)
}

fn classify(lin: u16) -> (String, String) {
    match MFX_PARAMS.get(lin as usize) {
        Some(entry) if entry.owning_type.is_some() && !entry.name.is_empty() => (
            entry.owning_type.unwrap().as_snake().to_string(),
            entry.name.to_string(),
        ),
        Some(entry) if !entry.name.is_empty() => {
            // Named but no owning type — common-header bytes that ended
            // up in raw_tail (rare; they're typed fields).
            (UNMAPPED_KEY.to_string(), entry.name.to_string())
        }
        _ => (UNMAPPED_KEY.to_string(), format!("0x{lin:02X}")),
    }
}

fn resolve_inner_key(group: &str, inner: &str) -> Option<u16> {
    let trimmed = inner.trim();
    if group == UNMAPPED_KEY {
        // Hex first; fall back to a name-only scan as a safety net for
        // hand-written YAML that puts a named param under `unmapped`.
        if let Some(lin) = parse_hex_u16(trimmed) {
            return Some(lin);
        }
        return find_name(None, trimmed);
    }
    let owner = MfxTypeOwner::from_snake(group)?;
    if let Some(lin) = find_name(Some(owner), trimmed) {
        return Some(lin);
    }
    // Same hex fallback inside a typed group, so users can drop in raw
    // bytes if they need to.
    parse_hex_u16(trimmed)
}

fn parse_hex_u16(s: &str) -> Option<u16> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    let v = u16::from_str_radix(hex, 16).ok()?;
    (v < 256).then_some(v)
}

fn find_name(owner: Option<MfxTypeOwner>, name: &str) -> Option<u16> {
    for (idx, entry) in MFX_PARAMS.iter().enumerate() {
        if entry.name.is_empty() {
            continue;
        }
        if entry.owning_type != owner {
            continue;
        }
        if entry.name == name {
            return Some(idx as u16);
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
    use crate::patch::Mfx;

    #[test]
    fn classify_picks_owning_type_for_known_bytes() {
        // Super Filter Type at linear 0x12.
        let (g, n) = classify(0x12);
        assert_eq!(g, "super_filter");
        assert_eq!(n, "Super Filter Type");
        // Flanger CutOff Freq at linear 0x80 (page 0x04 offset 0x00).
        let (g, n) = classify(0x80);
        assert_eq!(g, "flanger");
        assert_eq!(n, "Flanger CutOff Freq");
    }

    #[test]
    fn classify_routes_unmapped_to_hex() {
        // Offset with no name → unmapped/hex.
        let unnamed = MFX_PARAMS
            .iter()
            .enumerate()
            .find(|(_, e)| e.name.is_empty())
            .map(|(i, _)| i)
            .expect("at least one unnamed slot exists");
        let (g, n) = classify(unnamed as u16);
        assert_eq!(g, UNMAPPED_KEY);
        assert!(n.starts_with("0x"));
    }

    #[test]
    fn yaml_round_trip_groups_by_active_and_dormant_types() {
        let mut mfx = Mfx::default();
        mfx.raw_tail.insert(0x12, 1); // Super Filter Type
        mfx.raw_tail.insert(0x14, 64); // Super Filter Cutoff
        mfx.raw_tail.insert(0x80, 6); // Flanger CutOff Freq (page 0x04)
        let yaml = serde_yaml::to_string(&mfx).unwrap();
        eprintln!("{yaml}");
        assert!(yaml.contains("super_filter:"));
        assert!(yaml.contains("flanger:"));
        assert!(yaml.contains("Super Filter Type: 1"));
        assert!(yaml.contains("Flanger CutOff Freq: 6"));

        let back: Mfx = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.raw_tail.get(&0x12), Some(&1));
        assert_eq!(back.raw_tail.get(&0x14), Some(&64));
        assert_eq!(back.raw_tail.get(&0x80), Some(&6));
    }
}
