//! Custom serde for [`crate::patch::Pcm::raw_tail`] that emits and
//! parses **named keys** like `"Filter Type"` and `"Cutoff"` instead of
//! raw integer offsets like `0x00` and `0x01`. Drives the
//! human-readable YAML preset format.
//!
//! Wire map shape stays `BTreeMap<u8, u8>` keyed by tail-page offset.
//! Only the on-disk YAML representation changes.
//!
//! ## Format
//!
//! - Bytes whose offset is present in
//!   [`crate::pcm_tail_params::PCM_TAIL_PARAMS`] with a non-empty
//!   `name` serialize using that name (e.g. `"Filter Type": 1`).
//! - Bytes whose offset is unmapped — reserved slots (`0x1D`, `0x23`)
//!   or out-of-documented-range slots (`0x28..=0x7F`) — serialize as
//!   the hex string `"0xNN"` (e.g. `"0x30": 153`).
//! - Deserialization accepts EITHER form: a known param name is
//!   resolved back to its offset via the lookup table, and any
//!   `"0xNN"` (case-insensitive, also accepts plain decimal) parses as
//!   a raw offset.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::pcm_tail_params::{param_for, PCM_TAIL_PARAMS};

/// Emit the offset-keyed map as a string-keyed map using parameter
/// names (or `"0xNN"` for unmapped offsets).
pub fn serialize<S: Serializer>(map: &BTreeMap<u8, u8>, ser: S) -> Result<S::Ok, S::Error> {
    let named: BTreeMap<String, u8> = map
        .iter()
        .map(|(&off, &b)| {
            let key = match param_for(off) {
                Some(entry) if !entry.name.is_empty() => entry.name.to_string(),
                _ => format!("0x{off:02X}"),
            };
            (key, b)
        })
        .collect();
    named.serialize(ser)
}

/// Parse a string-keyed map back into offsets. Each key must resolve to
/// either a known parameter name (via the lookup table) or the literal
/// `"0xNN"` / decimal form.
pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<BTreeMap<u8, u8>, D::Error> {
    let raw: BTreeMap<String, u8> = BTreeMap::deserialize(de)?;
    let mut out = BTreeMap::new();
    for (key, byte) in raw {
        let off = resolve_key(&key).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown PCM tail param key {key:?}: not a documented \
                 parameter name and not a parseable offset (use \"0xNN\" \
                 hex or a decimal byte 0..=127)"
            ))
        })?;
        out.insert(off, byte);
    }
    Ok(out)
}

/// Map a user-supplied YAML key to a tail-page offset. Accepts:
/// - The canonical `name` field of any [`PCM_TAIL_PARAMS`] entry.
/// - `"0xNN"` / `"0XNN"` (case-insensitive hex).
/// - A plain decimal like `"40"` (0..=127).
fn resolve_key(key: &str) -> Option<u8> {
    let trimmed = key.trim();
    // Try param name match first (cheap linear scan over 40 entries).
    for entry in PCM_TAIL_PARAMS.iter() {
        if !entry.name.is_empty() && entry.name == trimmed {
            return Some(entry.offset);
        }
    }
    // Hex form.
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if let Ok(v) = u8::from_str_radix(hex, 16) {
            return Some(v);
        }
    }
    // Decimal fallback.
    trimmed.parse::<u8>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_key_handles_named_and_hex() {
        // Named params from the table.
        assert_eq!(resolve_key("Filter Type"), Some(0x00));
        assert_eq!(resolve_key("Portamento Type"), Some(0x1B));
        assert_eq!(resolve_key("LFO2 Pan Depth"), Some(0x27));
        // Whitespace tolerance.
        assert_eq!(resolve_key("  Cutoff  "), Some(0x01));
        // Hex form.
        assert_eq!(resolve_key("0x1D"), Some(0x1D));
        assert_eq!(resolve_key("0X1d"), Some(0x1D));
        // Decimal form.
        assert_eq!(resolve_key("40"), Some(40));
        // Unknown returns None.
        assert!(resolve_key("Definitely Not A Real Param").is_none());
        assert!(resolve_key("0xZZ").is_none());
    }

    #[test]
    fn yaml_emits_named_keys_in_expected_format() {
        // Build a minimal Pcm with three known params + one unmapped byte
        // and check the YAML literally contains the named keys (so any
        // future regression in the key encoder surfaces here).
        use crate::patch::Pcm;
        let mut pcm = Pcm::default();
        pcm.raw_tail.insert(0x00, 1);  // Filter Type = LPF
        pcm.raw_tail.insert(0x07, 35); // TVF Env Depth
        pcm.raw_tail.insert(0x1B, 1);  // Portamento Type = TIME
        pcm.raw_tail.insert(0x30, 153); // unmapped
        let yaml = serde_yaml::to_string(&pcm).unwrap();
        // Print under `cargo test -- --nocapture` for manual inspection.
        eprintln!("{yaml}");

        // Spot-check the rendered keys are present as substrings.
        assert!(yaml.contains("Filter Type: 1"));
        assert!(yaml.contains("Portamento Type: 1"));
        assert!(yaml.contains("TVF Env Depth: 35"));
        // Unmapped byte rendered as quoted hex string.
        assert!(yaml.contains("0x30") && yaml.contains(": 153"));
    }

    #[test]
    fn yaml_round_trip_uses_named_keys() {
        // A small Pcm with known params + one out-of-range byte.
        use crate::patch::Pcm;
        let mut pcm = Pcm::default();
        pcm.raw_tail.insert(0x00, 2);  // "Filter Type"
        pcm.raw_tail.insert(0x01, 64); // "Cutoff"
        pcm.raw_tail.insert(0x1B, 1);  // "Portamento Type"
        pcm.raw_tail.insert(0x30, 0x99); // unmapped → "0x30"

        let yaml = serde_yaml::to_string(&pcm).expect("serialize");
        // Named keys present.
        assert!(yaml.contains("Filter Type"));
        assert!(yaml.contains("Cutoff"));
        assert!(yaml.contains("Portamento Type"));
        // Unmapped byte uses hex string.
        assert!(yaml.contains("0x30") || yaml.contains("'0x30'"));

        let back: Pcm = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(back.raw_tail.get(&0x00), Some(&2));
        assert_eq!(back.raw_tail.get(&0x01), Some(&64));
        assert_eq!(back.raw_tail.get(&0x1B), Some(&1));
        assert_eq!(back.raw_tail.get(&0x30), Some(&0x99));
    }
}
