//! Static metadata for every byte of a PCM tone's **tail page**
//! (page `0x30` for Tone 1, `0x31` for Tone 2).
//!
//! FloorBoard `midi.xml` tags every byte at these addresses with
//! `customdesc="null"` — the XML has no information here. But the
//! complete byte-to-parameter mapping lives in FloorBoard's *C++
//! source*, specifically [`soundSource_synth_a.cpp`] (Tone 1) and
//! [`soundSource_synth_b.cpp`] (Tone 2), which wire each editor knob
//! to the appropriate `("30"/"31", "00", "XX")` address.
//!
//! Both tones share an identical tail layout — only the wire page byte
//! differs (0x30 vs 0x31). This table records the layout once and is
//! applied to either tone via [`crate::patch::Pcm::iter_tail_params`].
//!
//! Parameter names match the owner's manual (pages 25–27, "Parameter
//! List PCM TONE 1/PCM TONE 2"). The byte offsets are confirmed
//! against FloorBoard C++ source. Value-range / enum-variant metadata
//! is intentionally omitted from this table — it gets added by the
//! follow-up "rich table" pass with owner's manual help text.
//!
//! [`soundSource_synth_a.cpp`]: https://sourceforge.net/p/grfloorboard/code/
//! [`soundSource_synth_b.cpp`]: https://sourceforge.net/p/grfloorboard/code/

/// Group label for one tail-page parameter. The owner's manual organizes
/// PCM tone params into these 6 groups (plus the always-typed common
/// header on the *header* page). Useful for editor UI grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmTailGroup {
    /// FILTER group: filter type + cutoff + resonance + velocity / nuance
    /// sens + keyfollow.
    Filter,
    /// TVF (Time-Variant Filter) envelope: env depth + ADSR + sens.
    Tvf,
    /// Standalone velocity params (Level Velocity Sens, Velocity Curve
    /// Type) that the manual lumps with TONE but the device places
    /// adjacent to the TVA group.
    Velocity,
    /// TVA (Time-Variant Amplifier) envelope: ADSR + sens + level nuance.
    Tva,
    /// PITCH ENV: vel sens + depth + attack/decay time.
    PitchEnv,
    /// LFO1 + LFO2: rate + per-target depths.
    Lfo,
    /// Portamento Type (RATE / TIME) at offset 0x1B — pairs with
    /// portamento_sw + portamento_time on the header page.
    Portamento,
    /// FloorBoard's C++ source doesn't reference this offset — the byte
    /// exists on the wire but the editor doesn't expose it. Likely
    /// reserved.
    Reserved,
}

/// One byte of the tail page.
#[derive(Debug, Clone, Copy)]
pub struct PcmTailParamEntry {
    /// Offset within the tail page (`0x00..=0x7F`).
    pub offset: u8,
    /// Owning group for editor UI bucketing. `Reserved` for the
    /// FloorBoard-undocumented offsets `0x1D` and `0x23`.
    pub group: PcmTailGroup,
    /// Human-readable parameter name from the owner's manual / FloorBoard
    /// source comment. Empty for reserved bytes.
    pub name: &'static str,
}

/// Number of populated tail-page bytes (`0x00..=0x27` per FloorBoard's
/// `soundSource_synth_a.cpp`). Offsets `0x28..=0x7F` are not referenced
/// by the editor and round-trip unchanged.
pub const PCM_TAIL_PARAM_COUNT: usize = 0x28;

/// Per-offset metadata for the 40 documented tail-page bytes.
pub static PCM_TAIL_PARAMS: [PcmTailParamEntry; PCM_TAIL_PARAM_COUNT] = [
    // FILTER group (0x00..=0x06).
    PcmTailParamEntry { offset: 0x00, group: PcmTailGroup::Filter, name: "Filter Type" },
    PcmTailParamEntry { offset: 0x01, group: PcmTailGroup::Filter, name: "Cutoff" },
    PcmTailParamEntry { offset: 0x02, group: PcmTailGroup::Filter, name: "Resonance" },
    PcmTailParamEntry { offset: 0x03, group: PcmTailGroup::Filter, name: "Cutoff Velocity Sens" },
    PcmTailParamEntry { offset: 0x04, group: PcmTailGroup::Filter, name: "Cutoff Velocity Curve" },
    PcmTailParamEntry { offset: 0x05, group: PcmTailGroup::Filter, name: "Cutoff Keyfollow" },
    PcmTailParamEntry { offset: 0x06, group: PcmTailGroup::Filter, name: "Cutoff Nuance Sens" },
    // TVF group (0x07..=0x0D).
    PcmTailParamEntry { offset: 0x07, group: PcmTailGroup::Tvf, name: "TVF Env Depth" },
    PcmTailParamEntry { offset: 0x08, group: PcmTailGroup::Tvf, name: "TVF Attack Time" },
    PcmTailParamEntry { offset: 0x09, group: PcmTailGroup::Tvf, name: "TVF Decay Time" },
    PcmTailParamEntry { offset: 0x0A, group: PcmTailGroup::Tvf, name: "TVF Sustain Level" },
    PcmTailParamEntry { offset: 0x0B, group: PcmTailGroup::Tvf, name: "TVF Release Time" },
    PcmTailParamEntry { offset: 0x0C, group: PcmTailGroup::Tvf, name: "TVF Attack Velocity Sens" },
    PcmTailParamEntry { offset: 0x0D, group: PcmTailGroup::Tvf, name: "TVF Attack Nuance Sens" },
    // Velocity group (0x0E..=0x0F).
    PcmTailParamEntry { offset: 0x0E, group: PcmTailGroup::Velocity, name: "Level Velocity Sens" },
    PcmTailParamEntry { offset: 0x0F, group: PcmTailGroup::Velocity, name: "Velocity Curve Type" },
    // TVA group (0x10..=0x16).
    PcmTailParamEntry { offset: 0x10, group: PcmTailGroup::Tva, name: "TVA Attack Time" },
    PcmTailParamEntry { offset: 0x11, group: PcmTailGroup::Tva, name: "TVA Decay Time" },
    PcmTailParamEntry { offset: 0x12, group: PcmTailGroup::Tva, name: "TVA Sustain Level" },
    PcmTailParamEntry { offset: 0x13, group: PcmTailGroup::Tva, name: "TVA Release Time" },
    PcmTailParamEntry { offset: 0x14, group: PcmTailGroup::Tva, name: "TVA Attack Velocity Sens" },
    PcmTailParamEntry { offset: 0x15, group: PcmTailGroup::Tva, name: "TVA Attack Nuance Sens" },
    PcmTailParamEntry { offset: 0x16, group: PcmTailGroup::Tva, name: "TVA Level Nuance Sens" },
    // PITCH ENV group (0x17..=0x1A).
    PcmTailParamEntry { offset: 0x17, group: PcmTailGroup::PitchEnv, name: "Pitch Env Velocity Sens" },
    PcmTailParamEntry { offset: 0x18, group: PcmTailGroup::PitchEnv, name: "Pitch Env Depth" },
    PcmTailParamEntry { offset: 0x19, group: PcmTailGroup::PitchEnv, name: "Pitch Attack Time" },
    PcmTailParamEntry { offset: 0x1A, group: PcmTailGroup::PitchEnv, name: "Pitch Decay Time" },
    // PORTAMENTO group (0x1B).
    PcmTailParamEntry { offset: 0x1B, group: PcmTailGroup::Portamento, name: "Portamento Type" },
    // LFO1 group (0x1C..=0x21). Offset 0x1D is reserved/unreferenced.
    PcmTailParamEntry { offset: 0x1C, group: PcmTailGroup::Lfo, name: "LFO1 Rate" },
    PcmTailParamEntry { offset: 0x1D, group: PcmTailGroup::Reserved, name: "" },
    PcmTailParamEntry { offset: 0x1E, group: PcmTailGroup::Lfo, name: "LFO1 Pitch Depth" },
    PcmTailParamEntry { offset: 0x1F, group: PcmTailGroup::Lfo, name: "LFO1 TVF Depth" },
    PcmTailParamEntry { offset: 0x20, group: PcmTailGroup::Lfo, name: "LFO1 TVA Depth" },
    PcmTailParamEntry { offset: 0x21, group: PcmTailGroup::Lfo, name: "LFO1 Pan Depth" },
    // LFO2 group (0x22..=0x27). Offset 0x23 is reserved/unreferenced.
    PcmTailParamEntry { offset: 0x22, group: PcmTailGroup::Lfo, name: "LFO2 Rate" },
    PcmTailParamEntry { offset: 0x23, group: PcmTailGroup::Reserved, name: "" },
    PcmTailParamEntry { offset: 0x24, group: PcmTailGroup::Lfo, name: "LFO2 Pitch Depth" },
    PcmTailParamEntry { offset: 0x25, group: PcmTailGroup::Lfo, name: "LFO2 TVF Depth" },
    PcmTailParamEntry { offset: 0x26, group: PcmTailGroup::Lfo, name: "LFO2 TVA Depth" },
    PcmTailParamEntry { offset: 0x27, group: PcmTailGroup::Lfo, name: "LFO2 Pan Depth" },
];

/// Look up a tail-page param entry by offset. Returns `None` for
/// offsets beyond `0x27` (the last documented byte).
pub fn param_for(offset: u8) -> Option<&'static PcmTailParamEntry> {
    PCM_TAIL_PARAMS.get(offset as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_indexed_by_offset() {
        for (i, entry) in PCM_TAIL_PARAMS.iter().enumerate() {
            assert_eq!(entry.offset as usize, i);
        }
    }

    #[test]
    fn spot_check_known_params() {
        assert_eq!(PCM_TAIL_PARAMS[0x00].name, "Filter Type");
        assert_eq!(PCM_TAIL_PARAMS[0x00].group, PcmTailGroup::Filter);

        assert_eq!(PCM_TAIL_PARAMS[0x14].name, "TVA Attack Velocity Sens");
        assert_eq!(PCM_TAIL_PARAMS[0x14].group, PcmTailGroup::Tva);

        assert_eq!(PCM_TAIL_PARAMS[0x1B].name, "Portamento Type");
        assert_eq!(PCM_TAIL_PARAMS[0x1B].group, PcmTailGroup::Portamento);

        // 0x1D and 0x23 are reserved/unreferenced per FloorBoard source.
        assert_eq!(PCM_TAIL_PARAMS[0x1D].group, PcmTailGroup::Reserved);
        assert_eq!(PCM_TAIL_PARAMS[0x1D].name, "");
        assert_eq!(PCM_TAIL_PARAMS[0x23].group, PcmTailGroup::Reserved);

        // LFO Rate at 0x1C (LFO1) and 0x22 (LFO2).
        assert_eq!(PCM_TAIL_PARAMS[0x1C].name, "LFO1 Rate");
        assert_eq!(PCM_TAIL_PARAMS[0x22].name, "LFO2 Rate");
    }

    #[test]
    fn param_for_returns_none_beyond_documented_range() {
        assert!(param_for(0x28).is_none());
        assert!(param_for(0x7F).is_none());
        assert_eq!(param_for(0x00).map(|p| p.name), Some("Filter Type"));
        assert_eq!(param_for(0x27).map(|p| p.name), Some("LFO2 Pan Depth"));
    }
}
