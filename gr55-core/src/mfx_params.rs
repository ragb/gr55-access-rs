//! Build-time-generated table of every byte in the GR-55's MFX
//! parameter block.
//!
//! The MFX block spans 256 bytes laid across pages `0x03` and `0x04`:
//! page `0x03` offset `0x00..=0x7F` maps to linear `0..=127`, and page
//! `0x04` offset `0x00..=0x7F` maps to linear `128..=255`. Despite
//! FloorBoard `midi.xml`'s misleading "MFX 2" label on page `0x04`,
//! there is only **one** MFX slot — page `0x04` is the continuation of
//! MFX 1's type-specific parameter region.
//!
//! Each entry tells us:
//! - which wire byte (`page`, `offset`) it represents,
//! - which MFX effect type "owns" the byte (`None` for the 6 common
//!   header bytes and the 15 FloorBoard-undocumented padding bytes),
//! - the human-readable parameter name from FloorBoard.
//!
//! **The table's mere existence empirically validates the
//! disjoint-type-ranges hypothesis** — `build.rs` panics if any two
//! effect types lay claim to the same offset. So we know every byte
//! belongs to at most one effect type, and the layout fits inside the
//! 256-byte block exactly (235 type-specific + 6 common + 15 blank =
//! 256).
//!
//! Practical consequence: the type-specific parameters do **not** need
//! a Rust sum type. A flat byte buffer with this table for labelling +
//! the type-byte at offset `0x05` for active-range selection captures
//! the wire model exactly.

include!(concat!(env!("OUT_DIR"), "/mfx_params.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_every_offset_in_the_256_byte_block() {
        assert_eq!(MFX_PARAMS.len(), MFX_BLOCK_SIZE);
        assert_eq!(MFX_BLOCK_SIZE, 256);
        for (i, entry) in MFX_PARAMS.iter().enumerate() {
            let expected_page = if i < 128 { 0x03 } else { 0x04 };
            let expected_offset = (i & 0x7F) as u8;
            assert_eq!(
                entry.page, expected_page,
                "MFX_PARAMS[{i}] should be on page 0x{expected_page:02X}"
            );
            assert_eq!(
                entry.offset, expected_offset,
                "MFX_PARAMS[{i}] should be at offset 0x{expected_offset:02X}"
            );
        }
    }

    #[test]
    fn per_type_byte_counts_sum_to_documented_size() {
        let type_total: usize = MFX_TYPE_BYTE_COUNTS.iter().map(|(_, n)| *n).sum();
        // 235 type-specific + 6 common + 15 blank = 256
        assert_eq!(type_total + MFX_COMMON_BYTES + MFX_BLANK_BYTES, 256);
        assert_eq!(
            MFX_TYPE_BYTE_COUNTS.len(),
            20,
            "should have a count entry for each of the 20 effect types"
        );
    }

    #[test]
    fn spot_check_known_type_owners() {
        // page 0x03 offset 0x05 = MFX Type byte (common, no owning type).
        let type_byte = &MFX_PARAMS[0x05];
        assert!(type_byte.owning_type.is_none());

        // page 0x03 offset 0x07 = Equalizer Low Freq.
        let eq_low_freq = &MFX_PARAMS[0x07];
        assert_eq!(eq_low_freq.owning_type, Some(MfxTypeOwner::Equalizer));
        assert!(
            eq_low_freq.name.contains("Low Freq"),
            "got name {:?}",
            eq_low_freq.name
        );

        // page 0x03 offset 0x12 = Super Filter Type.
        let sf_type = &MFX_PARAMS[0x12];
        assert_eq!(sf_type.owning_type, Some(MfxTypeOwner::SuperFilter));

        // page 0x04 offset 0x00 (linear 128) = Flanger CutOff Freq.
        let flanger_first = &MFX_PARAMS[128];
        assert_eq!(flanger_first.owning_type, Some(MfxTypeOwner::Flanger));
        assert_eq!(flanger_first.page, 0x04);
        assert_eq!(flanger_first.offset, 0x00);
    }
}
