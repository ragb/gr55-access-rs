//! Custom serde for [`crate::patch::Modeling::raw_tail`] — grouped by
//! the **category** axis from [`crate::modeling_params::MODELING_PARAMS`]
//! (`E.GTR`, `E.Guitar`, `Acoustic`, `Bass`, `Synth`, `Modeling`, `NS`).
//! Inner keys retain the `(types)` qualifier so per-instrument
//! variations (Strat vs Telecaster Tone, etc.) stay unambiguous within
//! a category group.
//!
//! ```yaml
//! modeling:
//!   raw_tail:
//!     acoustic:
//!       "Buzz (Sitar)": 7
//!       "Top Type (steel)": 0
//!     bass:
//!       "Treble (M-Man)": 64
//!     e_gtr:
//!       "PU select (01-02)": 1
//!       "Tone (01-02)": 50
//!       "Tone (03-04)": 50
//!     ...
//!     unmapped:
//!       "0x73": 50
//! ```

use std::collections::BTreeMap;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::modeling_params::MODELING_PARAMS;

const UNMAPPED_KEY: &str = "unmapped";

pub fn serialize<S: Serializer>(map: &BTreeMap<u16, u8>, ser: S) -> Result<S::Ok, S::Error> {
    let mut grouped: BTreeMap<String, BTreeMap<String, u8>> = BTreeMap::new();
    for (&lin, &b) in map {
        let (group, inner) = classify(lin);
        grouped.entry(group).or_default().insert(inner, b);
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
                    "unknown Modeling param key {inner_key:?} under group {group:?}: \
                     not a documented parameter and not a parseable hex offset"
                ))
            })?;
            out.insert(lin, byte);
        }
    }
    Ok(out)
}

fn classify(lin: u16) -> (String, String) {
    let entry = MODELING_PARAMS.get(lin as usize);
    match entry {
        Some(e) if !e.name.is_empty() && !e.category.is_empty() => {
            let inner = if e.types.is_empty() {
                e.name.to_string()
            } else {
                format!("{} ({})", e.name, e.types)
            };
            (category_slug(e.category), inner)
        }
        Some(e) if !e.name.is_empty() => (UNMAPPED_KEY.to_string(), e.name.to_string()),
        _ => (UNMAPPED_KEY.to_string(), format!("0x{lin:02X}")),
    }
}

fn category_slug(cat: &str) -> String {
    // Lowercase the category, replace `.` with `_`, leave the rest as-is.
    cat.chars()
        .map(|c| match c {
            '.' => '_',
            c => c.to_ascii_lowercase(),
        })
        .collect::<String>()
}

fn slug_for_category(s: &str) -> String {
    category_slug(s)
}

fn resolve_inner_key(group: &str, inner: &str) -> Option<u16> {
    let trimmed = inner.trim();
    if group == UNMAPPED_KEY {
        if let Some(lin) = parse_hex_u16(trimmed) {
            return Some(lin);
        }
        // Bare-name fallback for hand-written YAML.
        return find_bare(trimmed);
    }
    // Split "Name (types)" → ("Name", "types").
    let (name, types) = split_inner(trimmed);
    for (idx, entry) in MODELING_PARAMS.iter().enumerate() {
        if entry.name.is_empty() {
            continue;
        }
        if slug_for_category(entry.category) != group {
            continue;
        }
        if entry.name != name {
            continue;
        }
        if entry.types != types {
            continue;
        }
        return Some(idx as u16);
    }
    parse_hex_u16(trimmed)
}

fn split_inner(s: &str) -> (&str, &str) {
    if let Some(open) = s.rfind(" (") {
        if let Some(close) = s.rfind(')') {
            if close == s.len() - 1 && close > open + 2 {
                return (s[..open].trim(), s[open + 2..close].trim());
            }
        }
    }
    (s, "")
}

fn parse_hex_u16(s: &str) -> Option<u16> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    let v = u16::from_str_radix(hex, 16).ok()?;
    (v < 256).then_some(v)
}

fn find_bare(name: &str) -> Option<u16> {
    let mut hit = None;
    for (idx, entry) in MODELING_PARAMS.iter().enumerate() {
        if !entry.name.is_empty() && entry.name == name {
            if hit.is_some() {
                return None; // ambiguous
            }
            hit = Some(idx as u16);
        }
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Modeling;

    #[test]
    fn classify_groups_by_category() {
        // E.GTR Tone (01-02) at linear 0x32.
        let (g, n) = classify(0x32);
        assert_eq!(g, "e_gtr");
        assert_eq!(n, "Tone (01-02)");
        // Acoustic Buzz (Sitar) at linear 0x3E.
        let (g, n) = classify(0x3E);
        assert_eq!(g, "acoustic");
        assert_eq!(n, "Buzz (Sitar)");
        // Bass M-Man Treble at linear 0x98 (page 0x11 offset 0x18).
        let (g, n) = classify(0x98);
        assert_eq!(g, "bass");
        assert_eq!(n, "Treble (M-Man)");
    }

    #[test]
    fn yaml_round_trip_groups_by_category() {
        let mut m = Modeling::default();
        m.raw_tail.insert(0x32, 50); // Strat Tone
        m.raw_tail.insert(0x3E, 7); // Sitar Buzz
        m.raw_tail.insert(0x98, 64); // Bass M-Man Treble
        let yaml = serde_yaml::to_string(&m).unwrap();
        eprintln!("{yaml}");
        assert!(yaml.contains("e_gtr:"));
        assert!(yaml.contains("acoustic:"));
        assert!(yaml.contains("bass:"));
        assert!(yaml.contains("Tone (01-02): 50"));
        assert!(yaml.contains("Buzz (Sitar): 7"));
        assert!(yaml.contains("Treble (M-Man): 64"));

        let back: Modeling = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.raw_tail.get(&0x32), Some(&50));
        assert_eq!(back.raw_tail.get(&0x3E), Some(&7));
        assert_eq!(back.raw_tail.get(&0x98), Some(&64));
    }
}
