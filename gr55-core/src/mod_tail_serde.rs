//! Custom serde for [`crate::patch::Mod::raw_tail`] that emits and
//! parses **named keys** like `"Distortion Drive"` or `"Wah Mode"`
//! instead of raw integer offsets. Drives the human-readable YAML
//! preset format.
//!
//! Names in [`crate::mod_params::MOD_PARAMS`] are unique by
//! construction (each effect type's params are prefixed with the
//! type name — `"Distortion Drive"`, `"Wah Mode"`, etc.), so the
//! lookup is unambiguous.
//!
//! ## Format
//!
//! - Bytes whose offset has a non-empty name in `MOD_PARAMS`
//!   serialize using that name.
//! - Bytes whose offset is unnamed (the `customdesc=""` / `"null"`
//!   placeholder slots inside MOD's reserved range) fall back to
//!   `"0xNN"` hex strings.
//! - Deserialization accepts EITHER form.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::mod_params::MOD_PARAMS;

pub fn serialize<S: Serializer>(map: &BTreeMap<u8, u8>, ser: S) -> Result<S::Ok, S::Error> {
    let named: BTreeMap<String, u8> = map
        .iter()
        .map(|(&off, &b)| {
            let key = match MOD_PARAMS.get(off as usize) {
                Some(entry) if !entry.name.is_empty() => entry.name.to_string(),
                _ => format!("0x{off:02X}"),
            };
            (key, b)
        })
        .collect();
    named.serialize(ser)
}

pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<BTreeMap<u8, u8>, D::Error> {
    let raw: BTreeMap<String, u8> = BTreeMap::deserialize(de)?;
    let mut out = BTreeMap::new();
    for (key, byte) in raw {
        let off = resolve_key(&key).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown MOD param key {key:?}: not a documented \
                 parameter name and not a parseable offset"
            ))
        })?;
        out.insert(off, byte);
    }
    Ok(out)
}

fn resolve_key(key: &str) -> Option<u8> {
    let trimmed = key.trim();
    // MOD_PARAMS is indexed by offset (0..=127), so a linear scan over
    // 128 entries is fine.
    for entry in MOD_PARAMS.iter() {
        if !entry.name.is_empty() && entry.name == trimmed {
            return Some(entry.offset);
        }
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if let Ok(v) = u8::from_str_radix(hex, 16) {
            return Some(v);
        }
    }
    trimmed.parse::<u8>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_key_known_names() {
        assert_eq!(resolve_key("Distortion Drive"), Some(0x19));
        assert_eq!(resolve_key("Distortion Tone"), Some(0x1A));
        assert_eq!(resolve_key("Wah Mode"), Some(0x1C));
        assert_eq!(resolve_key("Wah Sens"), Some(0x1F));
        // Hex fallback.
        assert_eq!(resolve_key("0x19"), Some(0x19));
        // Unknown returns None.
        assert!(resolve_key("Made-Up Param").is_none());
    }

    #[test]
    fn yaml_round_trip_uses_named_keys() {
        use crate::patch::Mod;
        let mut modu = Mod::default();
        modu.raw_tail.insert(0x19, 90); // Distortion Drive
        modu.raw_tail.insert(0x1A, 60); // Distortion Tone
        modu.raw_tail.insert(0x1C, 0);  // Wah Mode

        let yaml = serde_yaml::to_string(&modu).unwrap();
        eprintln!("{yaml}");
        assert!(yaml.contains("Distortion Drive: 90"));
        assert!(yaml.contains("Distortion Tone: 60"));
        assert!(yaml.contains("Wah Mode: 0"));

        let back: Mod = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.raw_tail.get(&0x19), Some(&90));
        assert_eq!(back.raw_tail.get(&0x1A), Some(&60));
        assert_eq!(back.raw_tail.get(&0x1C), Some(&0));
    }
}
