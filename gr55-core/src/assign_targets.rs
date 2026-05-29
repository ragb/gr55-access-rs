//! Static labels for the GR-55's Assign modulation-destination table.
//!
//! Each [`crate::patch::Assign`] points at a parameter identified by a
//! `(target, target_b)` wire-byte pair plus the patch's mode (Guitar /
//! Bass). The lookup table here resolves that pair to a human-readable
//! name like `"PCM1 Tone Level"` or `"Modeling Tone String 3 Level"`.
//!
//! Extracted from FloorBoard's `midi.xml`; see
//! `tools/extract_assign_targets.py`. The PARAM `value` attribute in
//! the XML runs past `0x7F` (up to `0xFD`); on the wire the low 7 bits
//! land in `target_b` and the high bit ends up in `target_c`'s LSB.
//! Editors typically prefer a single `u8` in `[0, 255]` for display
//! and let the codec split it at encode time.

include!("generated/assign_targets.rs");

/// Find the entry that matches `(mode, list, value)`. Returns `None`
/// when the triple isn't in the table.
pub fn lookup(mode: AssignTargetMode, list: u8, value: u8) -> Option<&'static AssignTargetEntry> {
    ASSIGN_TARGETS
        .iter()
        .find(|e| e.mode == mode && e.list == list && e.value == value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_entries_for_both_modes() {
        let guitar_count = ASSIGN_TARGETS
            .iter()
            .filter(|e| e.mode == AssignTargetMode::Guitar)
            .count();
        let bass_count = ASSIGN_TARGETS
            .iter()
            .filter(|e| e.mode == AssignTargetMode::Bass)
            .count();
        assert!(guitar_count > 0);
        assert!(bass_count > 0);
        // Per the extraction script's output, ~500 entries per mode.
        assert!(guitar_count >= 500);
        assert!(bass_count >= 500);
    }

    #[test]
    fn pcm1_tone_level_in_guitar_list_0() {
        // First sub-list, value 0x02 = "PCM1 Tone Level" per midi.xml.
        let entry = lookup(AssignTargetMode::Guitar, 0x00, 0x02).expect("PCM1 Tone Level present");
        assert!(
            entry.name.contains("PCM1 Tone Level"),
            "unexpected name {:?}",
            entry.name
        );
    }

    #[test]
    fn lookup_returns_none_for_missing_triple() {
        // List 0x07 doesn't exist (only 0..=2 are populated).
        assert!(lookup(AssignTargetMode::Guitar, 0x07, 0x00).is_none());
    }
}
