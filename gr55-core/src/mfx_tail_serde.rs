//! Custom serde for [`crate::patch::Mfx::raw_tail`] that emits and
//! parses **named keys** like `"Super Filter Cutoff"` or `"Phaser
//! Resonance"` instead of raw linear offsets. Drives the
//! human-readable YAML preset format.
//!
//! Mfx::raw_tail keys are u16 linear offsets `0..=255` that span
//! pages 0x03 + 0x04 (the GR-55 has a single MFX slot whose data
//! straddles both pages). [`crate::mfx_params::MFX_PARAMS`] is
//! indexed by linear offset, so the lookup is direct.
//!
//! MFX param names are unique by construction — each effect type
//! prefixes its params with the type name ("Super Filter Cutoff",
//! "Phaser Resonance", "Equalizer Low Gain"). No tiebreakers needed.
//!
//! ## Format
//!
//! - Bytes whose linear offset has a non-empty name in `MFX_PARAMS`
//!   serialize using that name.
//! - Bytes whose offset is unnamed (FloorBoard `customdesc=""`
//!   placeholder slots) fall back to `"0xNN"` hex strings on the wire,
//!   widened to `0xNN`/`0xNNN` as needed for the u16 keyspace.
//! - Deserialization accepts EITHER form. Hex parses as `u16` so it
//!   supports both `"0x05"` and `"0x85"` (page-0x04 region).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::mfx_params::MFX_PARAMS;

pub fn serialize<S: Serializer>(map: &BTreeMap<u16, u8>, ser: S) -> Result<S::Ok, S::Error> {
    let named: BTreeMap<String, u8> = map
        .iter()
        .map(|(&lin, &b)| {
            let key = match MFX_PARAMS.get(lin as usize) {
                Some(entry) if !entry.name.is_empty() => entry.name.to_string(),
                _ => format!("0x{lin:02X}"),
            };
            (key, b)
        })
        .collect();
    named.serialize(ser)
}

pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<BTreeMap<u16, u8>, D::Error> {
    let raw: BTreeMap<String, u8> = BTreeMap::deserialize(de)?;
    let mut out = BTreeMap::new();
    for (key, byte) in raw {
        let lin = resolve_key(&key).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown MFX param key {key:?}: not a documented \
                 parameter name and not a parseable offset"
            ))
        })?;
        out.insert(lin, byte);
    }
    Ok(out)
}

fn resolve_key(key: &str) -> Option<u16> {
    let trimmed = key.trim();
    // MFX_PARAMS has 256 entries; linear scan is fine for an editor.
    for (idx, entry) in MFX_PARAMS.iter().enumerate() {
        if !entry.name.is_empty() && entry.name == trimmed {
            return Some(idx as u16);
        }
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if let Ok(v) = u16::from_str_radix(hex, 16) {
            if v < 256 {
                return Some(v);
            }
        }
    }
    if let Ok(v) = trimmed.parse::<u16>() {
        if v < 256 {
            return Some(v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_key_known_names() {
        // Equalizer Low Freq on page 0x03 offset 0x07 = linear 0x07.
        assert_eq!(resolve_key("Equalizer Low Freq"), Some(0x07));
        // Super Filter Type on page 0x03 offset 0x12 = linear 0x12.
        assert_eq!(resolve_key("Super Filter Type"), Some(0x12));
        // Flanger CutOff Freq on page 0x04 offset 0x00 = linear 0x80.
        assert_eq!(resolve_key("Flanger CutOff Freq"), Some(0x80));
        // Hex form, both pages.
        assert_eq!(resolve_key("0x07"), Some(0x07));
        assert_eq!(resolve_key("0x80"), Some(0x80));
        // Out-of-range hex rejected.
        assert!(resolve_key("0x100").is_none());
        // Unknown name.
        assert!(resolve_key("Bogus Param Name").is_none());
    }

    #[test]
    fn yaml_round_trip_uses_named_keys() {
        use crate::patch::Mfx;
        let mut mfx = Mfx::default();
        mfx.raw_tail.insert(0x12, 1);  // Super Filter Type
        mfx.raw_tail.insert(0x14, 64); // Super Filter Cutoff
        mfx.raw_tail.insert(0x80, 6);  // Flanger CutOff Freq (page 0x04)

        let yaml = serde_yaml::to_string(&mfx).unwrap();
        eprintln!("{yaml}");
        assert!(yaml.contains("Super Filter Type: 1"));
        assert!(yaml.contains("Super Filter Cutoff: 64"));
        assert!(yaml.contains("Flanger CutOff Freq: 6"));

        let back: Mfx = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.raw_tail.get(&0x12), Some(&1));
        assert_eq!(back.raw_tail.get(&0x14), Some(&64));
        assert_eq!(back.raw_tail.get(&0x80), Some(&6));
    }
}
