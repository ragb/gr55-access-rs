//! Build-time-generated table of every byte in the GR-55's MOD
//! (modulation) effect parameter block.
//!
//! The MOD slot lives entirely on page `0x07` (128 bytes). Like MFX,
//! its 14 effect types own **disjoint byte ranges**: switching the type
//! byte at offset `0x16` doesn't reinterpret existing bytes — each
//! effect type has its own dedicated sub-range that persists on disk.
//!
//! The build verifies this empirically: if any two MOD effect types
//! claimed the same byte offset, `build.rs` would panic. The build
//! succeeded, so the disjoint hypothesis holds for MOD.
//!
//! Practical consequence: the MOD type-specific parameters do not need
//! a Rust sum type. A flat byte buffer plus this table for labelling +
//! the type byte at offset `0x16` for active-range selection captures
//! the wire model exactly. Same as MFX.

include!("generated/mod_params.rs");

impl ModTypeOwner {
    /// Snake_case identifier suitable for YAML keys.
    pub fn as_snake(&self) -> &'static str {
        match self {
            ModTypeOwner::Distortion => "distortion",
            ModTypeOwner::Wah => "wah",
            ModTypeOwner::Compressor => "compressor",
            ModTypeOwner::Limiter => "limiter",
            ModTypeOwner::Octave => "octave",
            ModTypeOwner::Phaser => "phaser",
            ModTypeOwner::Flanger => "flanger",
            ModTypeOwner::Tremolo => "tremolo",
            ModTypeOwner::Rotary => "rotary",
            ModTypeOwner::UniVibe => "uni_vibe",
            ModTypeOwner::Panner => "panner",
            ModTypeOwner::Delay => "delay",
            ModTypeOwner::Chorus => "chorus",
            ModTypeOwner::Equalizer => "equalizer",
        }
    }

    pub fn from_snake(s: &str) -> Option<Self> {
        Some(match s {
            "distortion" => ModTypeOwner::Distortion,
            "wah" => ModTypeOwner::Wah,
            "compressor" => ModTypeOwner::Compressor,
            "limiter" => ModTypeOwner::Limiter,
            "octave" => ModTypeOwner::Octave,
            "phaser" => ModTypeOwner::Phaser,
            "flanger" => ModTypeOwner::Flanger,
            "tremolo" => ModTypeOwner::Tremolo,
            "rotary" => ModTypeOwner::Rotary,
            "uni_vibe" => ModTypeOwner::UniVibe,
            "panner" => ModTypeOwner::Panner,
            "delay" => ModTypeOwner::Delay,
            "chorus" => ModTypeOwner::Chorus,
            "equalizer" => ModTypeOwner::Equalizer,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_every_offset_in_the_128_byte_block() {
        assert_eq!(MOD_PARAMS.len(), MOD_BLOCK_SIZE);
        assert_eq!(MOD_BLOCK_SIZE, 128);
        for (i, entry) in MOD_PARAMS.iter().enumerate() {
            assert_eq!(
                entry.page, 0x07,
                "MOD_PARAMS[{i}] should be on page 0x07"
            );
            assert_eq!(
                entry.offset as usize, i,
                "MOD_PARAMS[{i}] offset should match index"
            );
        }
    }

    #[test]
    fn per_type_byte_counts_sum_to_block_size() {
        let type_total: usize = MOD_TYPE_BYTE_COUNTS.iter().map(|(_, n)| *n).sum();
        // 66 type-specific + 59 common + 3 blank = 128
        assert_eq!(type_total + COMMON_BYTES + BLANK_BYTES, 128);
        assert_eq!(
            MOD_TYPE_BYTE_COUNTS.len(),
            14,
            "MOD has 14 effect types per FloorBoard midi.xml"
        );
    }

    #[test]
    fn enriched_metadata_covers_known_entries() {
        // Distortion Drive at 0x19: range 0..=120, display 0..=120, has help.
        let dist_drive = &MOD_PARAMS[0x19];
        assert_eq!(dist_drive.range, Some((0x00, 0x78)));
        assert_eq!(dist_drive.display_range, Some((0, 120)));
        assert!(dist_drive.values.is_empty());
        assert!(dist_drive.help.contains("distortion"));

        // Distortion Tone at 0x1A: range 0..=100, display -50..=+50, has help.
        let dist_tone = &MOD_PARAMS[0x1A];
        assert_eq!(dist_tone.display_range, Some((-50, 50)));
        assert!(dist_tone.help.contains("Tone"));

        // MOD type byte at 0x16: 14 named effect types.
        let mod_type = &MOD_PARAMS[0x16];
        assert_eq!(mod_type.values.len(), 14);
        assert_eq!(mod_type.values[0], (0x00, "Distortion"));
    }

    #[test]
    fn spot_check_known_type_owners() {
        // MOD type byte at 0x16 is common (no owning type — it's the
        // selector itself).
        let type_byte = &MOD_PARAMS[0x16];
        assert!(type_byte.owning_type.is_none());

        // 0x18 is the first type-specific byte: Distortion Type.
        let dist_first = &MOD_PARAMS[0x18];
        assert_eq!(dist_first.owning_type, Some(ModTypeOwner::Distortion));

        // 0x1C is Wah Mode (first Wah byte after Distortion's range).
        let wah_first = &MOD_PARAMS[0x1C];
        assert_eq!(wah_first.owning_type, Some(ModTypeOwner::Wah));

        // 0x23 is Compressor Sustain — confirms the trim_end match
        // works (FloorBoard's XML has "Compressor       " with
        // trailing whitespace).
        let comp_first = &MOD_PARAMS[0x23];
        assert_eq!(comp_first.owning_type, Some(ModTypeOwner::Compressor));
    }
}
