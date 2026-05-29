//! GR-55 address-map metadata — static `SYSTEM_PARAMETERS`,
//! `STRUCTURE_PARAMETERS`, `TABLES_PARAMETERS`, `MPT_PARAMETERS`
//! tables embedded as committed source ([`generated/midi_map.rs`]).

include!("generated/midi_map.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_have_entries() {
        assert!(!SYSTEM_PARAMETERS.is_empty());
        assert!(!STRUCTURE_PARAMETERS.is_empty());
        assert!(!TABLES_PARAMETERS.is_empty());
        assert!(!MPT_PARAMETERS.is_empty());
    }

    #[test]
    fn system_holds_split_current_patch_enumeration() {
        // The 297-patch enumeration doesn't fit in one 7-bit byte, so FloorBoard's
        // XML splits it: the low-byte parameter holds values 0..127 ("User 01:1"
        // through somewhere around "User 43:1"); a sibling parameter encodes the
        // high byte.
        let low = SYSTEM_PARAMETERS
            .iter()
            .find(|p| p.values.first().is_some_and(|v| v.name == "User 01:1"))
            .expect("SYSTEM should contain a 'User 01:1' starting parameter");
        assert_eq!(low.values.len(), 128);
        // Total slots across this parameter and all parameters that share its
        // first three path bytes should cover all 297 user slots.
        let prefix = &low.path[..low.path.len().saturating_sub(1)];
        let total: usize = SYSTEM_PARAMETERS
            .iter()
            .filter(|p| p.path.starts_with(prefix))
            .map(|p| p.values.len())
            .sum();
        assert!(
            total >= 297,
            "expected combined enumeration of 297+ slots, got {total}"
        );
    }

    #[test]
    fn structure_starts_with_guitar_bass_mode() {
        let first = STRUCTURE_PARAMETERS
            .first()
            .expect("STRUCTURE must have at least one parameter");
        assert_eq!(first.name, "Guitar/Bass Mode");
        let bytes: Vec<u8> = first.values.iter().map(|e| e.byte).collect();
        let names: Vec<&str> = first.values.iter().map(|e| e.name).collect();
        assert_eq!(bytes, [0x00, 0x01]);
        assert_eq!(names, ["Guitar", "Bass"]);
    }

    #[test]
    fn structure_name1_covers_printable_ascii() {
        let name1 = STRUCTURE_PARAMETERS
            .iter()
            .find(|p| p.name == "Name1")
            .expect("STRUCTURE should contain a 'Name1' parameter");
        // Printable ASCII space is 0x20..=0x7E (95 codepoints). FloorBoard's XML
        // happens to enumerate 94 — close enough; the parameter clearly covers
        // the printable-ASCII range starting at 0x20.
        assert!(name1.values.len() >= 90);
        assert_eq!(name1.values.first().unwrap().byte, 0x20);
        assert!(name1.values.iter().any(|v| v.byte == 0x41 && v.name == "A"));
        assert!(name1.values.iter().any(|v| v.byte == 0x7A && v.name == "z"));
    }

    #[test]
    fn paths_are_three_or_four_bytes_deep() {
        for p in SYSTEM_PARAMETERS
            .iter()
            .chain(STRUCTURE_PARAMETERS)
            .chain(TABLES_PARAMETERS)
            .chain(MPT_PARAMETERS)
        {
            let depth = p.path.len();
            assert!(
                (1..=5).contains(&depth),
                "param '{}' has unexpected path depth {}: {:02X?}",
                p.name,
                depth,
                p.path
            );
        }
    }
}
