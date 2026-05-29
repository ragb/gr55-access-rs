//! Custom serde for [`crate::patch::Modeling::raw_tail`] that emits and
//! parses **compound named keys** like `"Tone (E.GTR/01-02)"` or
//! `"PU select (E.GTR/03-04)"` instead of raw linear offsets.
//!
//! Modeling differs from MFX/MOD/PCM in that param **names alone are
//! not unique** — `"Tone"`, `"PU select"`, `"Volume"` etc. appear at
//! multiple offsets across instrument categories (Strat Tone vs Tele
//! Tone vs Sitar Tone). To round-trip safely we always qualify the
//! name with its `(category, types)` axes from
//! [`crate::modeling_params::MODELING_PARAMS`].
//!
//! ## Format
//!
//! Each entry's YAML key is:
//!
//! - `"{name} ({category}/{types})"` — both qualifiers present
//!   (the common case for type-specific bytes — e.g.
//!   `"Tone (E.GTR/01-02)"`, `"Buzz (Acoustic/Sitar)"`,
//!   `"Treble (Bass/M-Man)"`).
//! - `"{name} ({category})"` — category-only (common-header rows where
//!   FloorBoard left `desc` empty, e.g. `"Tone Sw (Modeling)"`).
//! - `"{name}"` — bare name (only used if a populated entry has neither
//!   category nor types, which shouldn't normally happen).
//! - `"0xNN"` / `"0xNNN"` — hex fallback for unmapped offsets
//!   (FloorBoard placeholder rows where `name` is empty too). u16
//!   parsing supports both pages.
//!
//! Deserialization accepts EITHER form: the compound key is split back
//! into `(name, category, types)` and looked up; bare names fall back
//! to a name-only scan (rejecting ambiguous matches); hex offsets
//! parse directly.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::modeling_params::MODELING_PARAMS;

pub fn serialize<S: Serializer>(map: &BTreeMap<u16, u8>, ser: S) -> Result<S::Ok, S::Error> {
    let named: BTreeMap<String, u8> = map
        .iter()
        .map(|(&lin, &b)| (compose_key(lin), b))
        .collect();
    named.serialize(ser)
}

pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<BTreeMap<u16, u8>, D::Error> {
    let raw: BTreeMap<String, u8> = BTreeMap::deserialize(de)?;
    let mut out = BTreeMap::new();
    for (key, byte) in raw {
        let lin = resolve_key(&key).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown Modeling param key {key:?}: not a documented \
                 parameter and not a parseable offset"
            ))
        })?;
        out.insert(lin, byte);
    }
    Ok(out)
}

fn compose_key(linear: u16) -> String {
    let entry = match MODELING_PARAMS.get(linear as usize) {
        Some(e) if !e.name.is_empty() => e,
        _ => return format!("0x{linear:02X}"),
    };
    match (entry.category.is_empty(), entry.types.is_empty()) {
        (false, false) => format!("{} ({}/{})", entry.name, entry.category, entry.types),
        (false, true) => format!("{} ({})", entry.name, entry.category),
        _ => entry.name.to_string(),
    }
}

fn resolve_key(key: &str) -> Option<u16> {
    let trimmed = key.trim();

    // 1. Compound form: "name (category/types)" or "name (category)".
    if let Some((name, qual)) = split_compound(trimmed) {
        let (cat, types) = match qual.split_once('/') {
            Some((c, t)) => (c, t),
            None => (qual, ""),
        };
        for (idx, entry) in MODELING_PARAMS.iter().enumerate() {
            if entry.name == name && entry.category == cat && entry.types == types {
                return Some(idx as u16);
            }
        }
        // Compound form didn't match — fall through to bare-name and
        // hex attempts using the raw `trimmed` key.
    }

    // 2. Bare name: accept only if uniquely-named (no collision).
    let mut hit: Option<u16> = None;
    for (idx, entry) in MODELING_PARAMS.iter().enumerate() {
        if !entry.name.is_empty() && entry.name == trimmed {
            if hit.is_some() {
                hit = None;
                break;
            }
            hit = Some(idx as u16);
        }
    }
    if let Some(idx) = hit {
        return Some(idx);
    }

    // 3. Hex / decimal fallback.
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

fn split_compound(key: &str) -> Option<(&str, &str)> {
    let open = key.rfind(" (")?;
    let close = key.rfind(')')?;
    if close != key.len() - 1 || close <= open + 2 {
        return None;
    }
    let name = key[..open].trim();
    let qual = key[open + 2..close].trim();
    if name.is_empty() || qual.is_empty() {
        return None;
    }
    Some((name, qual))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_key_uses_compound_form_for_known_entries() {
        // E.GTR Strat Tone at linear 0x32: "Tone (E.GTR/01-02)".
        assert_eq!(compose_key(0x32), "Tone (E.GTR/01-02)");
        // Acoustic Sitar Buzz at linear 0x3E: "Buzz (Acoustic/Sitar)".
        assert_eq!(compose_key(0x3E), "Buzz (Acoustic/Sitar)");
        // Bass M-Man Treble at linear 0x98 (page 0x11 offset 0x18).
        assert_eq!(compose_key(0x98), "Treble (Bass/M-Man)");
        // Modeling common Tone Sw at linear 0x0A. FloorBoard tags it
        // category="Modeling" types="Tone" → full compound form.
        assert_eq!(compose_key(0x0A), "Tone Sw (Modeling/Tone)");
    }

    #[test]
    fn split_compound_extracts_name_and_qualifier() {
        assert_eq!(
            split_compound("Tone (E.GTR/01-02)"),
            Some(("Tone", "E.GTR/01-02"))
        );
        assert_eq!(
            split_compound("Tone Sw (Modeling/Tone)"),
            Some(("Tone Sw", "Modeling/Tone"))
        );
        // Trailing space / no close paren → no match.
        assert_eq!(split_compound("Tone (E.GTR/01-02"), None);
        assert_eq!(split_compound("Tone"), None);
    }

    #[test]
    fn resolve_key_round_trips_compound_forms() {
        assert_eq!(resolve_key("Tone (E.GTR/01-02)"), Some(0x32));
        assert_eq!(resolve_key("Buzz (Acoustic/Sitar)"), Some(0x3E));
        assert_eq!(resolve_key("Treble (Bass/M-Man)"), Some(0x98));
        assert_eq!(resolve_key("Tone Sw (Modeling/Tone)"), Some(0x0A));
        // Whitespace tolerance.
        assert_eq!(resolve_key("  Tone Sw (Modeling/Tone)  "), Some(0x0A));
    }

    #[test]
    fn resolve_key_rejects_ambiguous_bare_names() {
        // "Tone" matches multiple categories — bare form must fail.
        assert!(resolve_key("Tone").is_none());
        // Wrong category in compound → fail.
        assert!(resolve_key("Tone (E.GTR/99-99)").is_none());
    }

    #[test]
    fn resolve_key_hex_fallback_supports_both_pages() {
        // Page 0x10 hex.
        assert_eq!(resolve_key("0x00"), Some(0x00));
        // Page 0x11 hex (linear >= 0x80).
        assert_eq!(resolve_key("0x80"), Some(0x80));
        assert_eq!(resolve_key("0xFF"), Some(0xFF));
        // Out-of-range rejected.
        assert!(resolve_key("0x100").is_none());
    }

    #[test]
    fn yaml_round_trip_uses_compound_keys() {
        use crate::patch::Modeling;
        let mut m = Modeling::default();
        m.raw_tail.insert(0x32, 50); // Strat Tone
        m.raw_tail.insert(0x3E, 7);  // Sitar Buzz
        m.raw_tail.insert(0x98, 64); // Bass M-Man Treble

        let yaml = serde_yaml::to_string(&m).unwrap();
        eprintln!("{yaml}");
        assert!(yaml.contains("Tone (E.GTR/01-02): 50"));
        assert!(yaml.contains("Buzz (Acoustic/Sitar): 7"));
        assert!(yaml.contains("Treble (Bass/M-Man): 64"));

        let back: Modeling = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.raw_tail.get(&0x32), Some(&50));
        assert_eq!(back.raw_tail.get(&0x3E), Some(&7));
        assert_eq!(back.raw_tail.get(&0x98), Some(&64));
    }
}
