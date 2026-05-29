//! Build-time-generated table of every byte in the GR-55's Modeling
//! parameter block.
//!
//! The Modeling block spans 256 bytes laid across pages `0x10` (linear
//! `0..=127`) and `0x11` (linear `128..=255`). Unlike MFX and MOD, the
//! ownership taxonomy here is **multi-axis**:
//!
//! - **Mode** comes from the page byte for type-specific bytes: page
//!   `0x10` offsets `0x1E..=0x7F` are Guitar Mode; page `0x11` is Bass
//!   Mode. The common header bytes at page `0x10` `0x00..=0x1D`
//!   ([`ModelingMode::Both`]) apply regardless of mode.
//! - **Category** comes from FloorBoard's `abbr` attribute — `E.GTR`,
//!   `E.Guitar`, `Acoustic`, `Bass`, `Synth`, `Modeling` (common
//!   header), `NS` (noise suppressor).
//! - **Types** come from FloorBoard's `desc` attribute, encoded either
//!   as a dash-separated list of numeric IDs (`"01-02"`, `"03-04-05-07"`),
//!   sub-type names (`"steel"`, `"Nylon"`, `"Jazz-PB"`), or descriptive
//!   phrases (`"Analog GR Envelope modulation"`). Empty string means
//!   the byte applies to every type in its category.
//!
//! **Wire-level disjointness holds**: `build.rs` asserts that no two
//! DATA elements claim the same `(page, offset)`, and the build
//! succeeded. Multiple TYPES per byte (via dash-separated `desc`) is
//! expected — it represents physical-instrument families that share a
//! control surface (e.g. Strat Classic and Strat Modern share the same
//! Volume knob).
//!
//! Per-category byte allocations (post-extraction):
//! Synth 80, Modeling-common 43, Acoustic 20, Bass 18, E.GTR 5,
//! E.Guitar 4, NS 3, Bass-Model-List 1, Guitar-Model-List 1, and 39
//! bytes with empty `abbr` (placeholders FloorBoard didn't fully
//! document). 14 bytes are shared across multiple types, 113 are owned
//! by a single type, 87 are common (mode-bridging / NS / placeholder).

include!(concat!(env!("OUT_DIR"), "/modeling_params.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_every_offset_in_the_256_byte_block() {
        assert_eq!(MODELING_PARAMS.len(), MODELING_BLOCK_SIZE);
        assert_eq!(MODELING_BLOCK_SIZE, 256);
        for (i, entry) in MODELING_PARAMS.iter().enumerate() {
            let expected_page = if i < 128 { 0x10 } else { 0x11 };
            let expected_offset = (i & 0x7F) as u8;
            // Note: placeholder rows (no DATA in XML) keep page=0x00,
            // offset=0x00 in the build output. We only check populated
            // ones — populated rows always have a category attribute set.
            if !entry.category.is_empty() || !entry.name.is_empty() {
                assert_eq!(
                    entry.page, expected_page,
                    "MODELING_PARAMS[{i}] page should be 0x{expected_page:02X}"
                );
                assert_eq!(
                    entry.offset, expected_offset,
                    "MODELING_PARAMS[{i}] offset should be 0x{expected_offset:02X}"
                );
            }
        }
    }

    #[test]
    fn mode_assignment_matches_page_for_type_specific_bytes() {
        for (i, entry) in MODELING_PARAMS.iter().enumerate() {
            if i < 0x1E {
                // Common header bytes (page 0x10 offsets 0x00..=0x1D).
                assert_eq!(entry.mode, ModelingMode::Both);
            } else if entry.category.is_empty() {
                continue;
            } else if i < 0x80 {
                assert_eq!(
                    entry.mode,
                    ModelingMode::Guitar,
                    "MODELING_PARAMS[0x{i:02X}] should be Guitar mode"
                );
            } else {
                assert_eq!(
                    entry.mode,
                    ModelingMode::Bass,
                    "MODELING_PARAMS[0x{i:02X}] should be Bass mode"
                );
            }
        }
    }

    #[test]
    fn spot_check_known_owners() {
        // Common header — Modeling Tone Sw at page 0x10 offset 0x0A.
        let tone_sw = &MODELING_PARAMS[0x0A];
        assert_eq!(tone_sw.mode, ModelingMode::Both);
        assert_eq!(tone_sw.category, "Modeling");
        assert_eq!(tone_sw.name, "Tone Sw");

        // E.GTR shared by types 01-02 (the two Strats) — page 0x10 offset 0x32.
        let strat_tone = &MODELING_PARAMS[0x32];
        assert_eq!(strat_tone.mode, ModelingMode::Guitar);
        assert_eq!(strat_tone.category, "E.GTR");
        assert_eq!(strat_tone.types, "01-02");
        assert_eq!(strat_tone.name, "Tone");

        // Bass mode Music Man Treble at page 0x11 offset 0x18 = linear 0x98.
        let mm_treble = &MODELING_PARAMS[0x98];
        assert_eq!(mm_treble.mode, ModelingMode::Bass);
        assert_eq!(mm_treble.category, "Bass");
        assert_eq!(mm_treble.types, "M-Man");
        assert_eq!(mm_treble.name, "Treble");

        // Acoustic Sitar Buzz at page 0x10 offset 0x3E.
        let sitar_buzz = &MODELING_PARAMS[0x3E];
        assert_eq!(sitar_buzz.category, "Acoustic");
        assert_eq!(sitar_buzz.types, "Sitar");
        assert_eq!(sitar_buzz.name, "Buzz");
    }

    #[test]
    fn category_byte_counts_present_for_known_categories() {
        let cats: std::collections::HashMap<&str, usize> =
            MODELING_CATEGORY_BYTE_COUNTS.iter().copied().collect();
        // 80 bytes for Synth (shared between Guitar and Bass mode synth).
        assert_eq!(cats.get("Synth"), Some(&80));
        // 43 common (always-present) bytes labelled "Modeling".
        assert_eq!(cats.get("Modeling"), Some(&43));
        // 20 Acoustic bytes — all on page 0x10 (Guitar Mode has Acoustic, Bass Mode doesn't).
        assert_eq!(cats.get("Acoustic"), Some(&20));
    }

    #[test]
    fn shared_plus_single_plus_common_sum_to_populated_total() {
        let populated: usize = MODELING_CATEGORY_BYTE_COUNTS.iter().map(|(_, n)| *n).sum();
        assert_eq!(
            MODELING_SHARED_TYPE_BYTES + MODELING_SINGLE_TYPE_BYTES + MODELING_COMMON_BYTES,
            populated,
            "shared + single + common should sum to total populated byte count"
        );
    }
}
