//! Custom serde for [`crate::patch::Pcm::raw_tail`] — grouped by
//! [`PcmTailGroup`] (Filter / TVF / TVA / PitchEnv / LFO / Portamento /
//! Velocity / Reserved). Each group's parameters land in a nested map
//! keyed by the canonical parameter name from `PCM_TAIL_PARAMS`.
//!
//! ```yaml
//! pcm:
//! - raw_tail:
//!     filter:
//!       Filter Type: 1
//!       Cutoff: 64
//!     tvf:
//!       TVF Env Depth: 35
//!     ...
//!     unmapped:                # offsets w/o a named entry
//!       "0x28": 0
//! ```

use std::collections::BTreeMap;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::pcm_tail_params::{param_for, PcmTailGroup, PCM_TAIL_PARAMS};

const UNMAPPED_KEY: &str = "unmapped";

pub fn serialize<S: Serializer>(map: &BTreeMap<u8, u8>, ser: S) -> Result<S::Ok, S::Error> {
    let mut grouped: BTreeMap<String, BTreeMap<String, u8>> = BTreeMap::new();
    for (&off, &b) in map {
        let (group_key, inner_key) = classify(off);
        grouped
            .entry(group_key)
            .or_default()
            .insert(inner_key, b);
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
                    "unknown PCM tail param key {inner_key:?} under group {group:?}: \
                     not a documented parameter name and not a parseable hex offset \
                     (use `unmapped:` + `0xNN` for raw bytes)"
                ))
            })?;
            out.insert(off, byte);
        }
    }
    Ok(out)
}

fn classify(off: u8) -> (String, String) {
    match param_for(off) {
        Some(entry) if !entry.name.is_empty() => (
            entry.group.as_snake().to_string(),
            entry.name.to_string(),
        ),
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
    let target = PcmTailGroup::from_snake(group)?;
    if let Some(off) = find_name(Some(target), trimmed) {
        return Some(off);
    }
    parse_hex_u8(trimmed)
}

fn parse_hex_u8(s: &str) -> Option<u8> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    u8::from_str_radix(hex, 16).ok()
}

fn find_name(group: Option<PcmTailGroup>, name: &str) -> Option<u8> {
    for entry in PCM_TAIL_PARAMS.iter() {
        if entry.name.is_empty() {
            continue;
        }
        if let Some(g) = group {
            if entry.group != g {
                continue;
            }
        }
        if entry.name == name {
            return Some(entry.offset);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Pcm;

    #[test]
    fn classify_picks_group_for_known_offsets() {
        // Filter Type at 0x00.
        let (g, n) = classify(0x00);
        assert_eq!(g, "filter");
        assert_eq!(n, "Filter Type");
        // Portamento Type at 0x1B.
        let (g, n) = classify(0x1B);
        assert_eq!(g, "portamento");
        assert_eq!(n, "Portamento Type");
        // LFO2 Pan Depth at 0x27.
        let (g, n) = classify(0x27);
        assert_eq!(g, "lfo");
        assert_eq!(n, "LFO2 Pan Depth");
    }

    #[test]
    fn yaml_round_trip_groups_by_pcm_tail_group() {
        let mut pcm = Pcm::default();
        pcm.raw_tail.insert(0x00, 1); // Filter Type
        pcm.raw_tail.insert(0x01, 64); // Cutoff
        pcm.raw_tail.insert(0x1B, 1); // Portamento Type
        pcm.raw_tail.insert(0x30, 0x99); // Unmapped
        let yaml = serde_yaml::to_string(&pcm).unwrap();
        eprintln!("{yaml}");
        assert!(yaml.contains("filter:"));
        assert!(yaml.contains("Filter Type: 1"));
        assert!(yaml.contains("portamento:"));
        assert!(yaml.contains("unmapped:"));

        let back: Pcm = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.raw_tail.get(&0x00), Some(&1));
        assert_eq!(back.raw_tail.get(&0x01), Some(&64));
        assert_eq!(back.raw_tail.get(&0x1B), Some(&1));
        assert_eq!(back.raw_tail.get(&0x30), Some(&0x99));
    }
}
