//! Static metadata for every byte of a PCM tone's **tail page**
//! (page `0x30` for Tone 1, `0x31` for Tone 2).
//!
//! Parameter names match Roland's GR-55 Owner's Manual (pages 25-27,
//! "Parameter List PCM TONE 1/PCM TONE 2"). Byte offsets are facts
//! about Roland's MIDI protocol (Feist v. Rural Telephone, 1991) —
//! not user-facing in the OM but discoverable from any GR-55 editor.
//!
//! Both tones share an identical tail layout — only the wire page byte
//! differs (0x30 vs 0x31). This table records the layout once and is
//! applied to either tone via [`crate::patch::Pcm::iter_tail_params`].

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

impl PcmTailGroup {
    /// Snake_case identifier suitable for YAML keys.
    pub fn as_snake(&self) -> &'static str {
        match self {
            PcmTailGroup::Filter => "filter",
            PcmTailGroup::Tvf => "tvf",
            PcmTailGroup::Velocity => "velocity",
            PcmTailGroup::Tva => "tva",
            PcmTailGroup::PitchEnv => "pitch_env",
            PcmTailGroup::Lfo => "lfo",
            PcmTailGroup::Portamento => "portamento",
            PcmTailGroup::Reserved => "reserved",
        }
    }

    pub fn from_snake(s: &str) -> Option<Self> {
        Some(match s {
            "filter" => PcmTailGroup::Filter,
            "tvf" => PcmTailGroup::Tvf,
            "velocity" => PcmTailGroup::Velocity,
            "tva" => PcmTailGroup::Tva,
            "pitch_env" => PcmTailGroup::PitchEnv,
            "lfo" => PcmTailGroup::Lfo,
            "portamento" => PcmTailGroup::Portamento,
            "reserved" => PcmTailGroup::Reserved,
            _ => return None,
        })
    }
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
    /// Named byte values (empty for purely-numeric parameters).
    ///
    /// The wire byte order is **inferred** from the owner's manual's
    /// parameter-value listing order — Roland's convention is that the
    /// first listed value is byte 0x00, the second is 0x01, etc. Not
    /// confirmed against hardware.
    pub values: &'static [(u8, &'static str)],
    /// Raw wire-byte range (min, max). `None` for purely enumerated
    /// parameters. Inferred from the owner's manual's value spec —
    /// e.g. "-50..+50" implies a 101-byte range; we assume the wire
    /// uses the standard Roland convention of byte 0 = -50, byte 100 =
    /// +50.
    pub range: Option<(u8, u8)>,
    /// Display range (min, max) for numeric parameters whose display
    /// values differ from their wire bytes. Direct from the owner's
    /// manual.
    pub display_range: Option<(i32, i32)>,
    /// Tooltip-friendly description from the owner's manual. Empty if
    /// the manual didn't dedicate a description sentence to this param.
    pub help: &'static str,
}

/// Number of populated tail-page bytes (`0x00..=0x27` per FloorBoard's
/// `soundSource_synth_a.cpp`). Offsets `0x28..=0x7F` are not referenced
/// by the editor and round-trip unchanged.
pub const PCM_TAIL_PARAM_COUNT: usize = 0x28;

// Named-value tables shared by multiple parameters. Static refs keep
// each PCM_TAIL_PARAMS entry small and dedupe these enum-variant lists.
const FILTER_TYPE_VALUES: &[(u8, &str)] = &[
    (0x00, "OFF"), (0x01, "LPF"), (0x02, "BPF"), (0x03, "HPF"),
    (0x04, "PKG"), (0x05, "LPF2"), (0x06, "LPF3"), (0x07, "TONE"),
];
const VELOCITY_CURVE_VALUES: &[(u8, &str)] = &[
    (0x00, "FIX"),
    (0x01, "1"), (0x02, "2"), (0x03, "3"), (0x04, "4"),
    (0x05, "5"), (0x06, "6"), (0x07, "7"),
    (0x08, "TONE"),
];
const PORTAMENTO_TYPE_VALUES: &[(u8, &str)] = &[(0x00, "RATE"), (0x01, "TIME")];
const RELEASE_MODE_VALUES: &[(u8, &str)] = &[(0x00, "1"), (0x01, "2")];

// Standard Roland symmetric range for -50..=+50 display params.
// Wire bytes 0..=100 with byte 50 = display 0.
const SYM_50_RANGE: Option<(u8, u8)> = Some((0x00, 0x64));
const SYM_50_DISPLAY: Option<(i32, i32)> = Some((-50, 50));

/// Per-offset metadata for the 40 documented tail-page bytes.
///
/// Wire-byte ranges and named-value byte mappings are INFERRED from the
/// owner's manual's parameter-list listing order (Roland convention:
/// listed values map to bytes 0x00, 0x01, ... in order, and symmetric
/// display ranges like -50..=+50 use wire bytes 0..=100 with byte 50 =
/// display 0). Not confirmed against hardware. The `help` field is
/// transcribed from the owner's manual's "Description" column on
/// pages 25–27.
pub static PCM_TAIL_PARAMS: [PcmTailParamEntry; PCM_TAIL_PARAM_COUNT] = [
    // ---- FILTER group (0x00..=0x06) ----
    PcmTailParamEntry { offset: 0x00, group: PcmTailGroup::Filter, name: "Filter Type",
        values: FILTER_TYPE_VALUES, range: None, display_range: None,
        help: "OFF: no filter. LPF: low-pass (cuts highs). BPF: band-pass. HPF: high-pass. PKG: peaking. LPF2: half-strength LPF (acoustic piano). LPF3: cutoff-dependent LPF (acoustic). TONE: optimal for the selected tone." },
    PcmTailParamEntry { offset: 0x01, group: PcmTailGroup::Filter, name: "Cutoff",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Specifies the frequency at which the filter will begin to be applied." },
    PcmTailParamEntry { offset: 0x02, group: PcmTailGroup::Filter, name: "Resonance",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Boosts the region near the cutoff frequency, giving the sound a distinctive character. Raising this value excessively may cause oscillation and distortion." },
    PcmTailParamEntry { offset: 0x03, group: PcmTailGroup::Filter, name: "Cutoff Velocity Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Specifies the amount by which your playing strength will vary the cutoff frequency. With positive values, stronger playing will raise the cutoff frequency." },
    PcmTailParamEntry { offset: 0x04, group: PcmTailGroup::Filter, name: "Cutoff Velocity Curve",
        values: VELOCITY_CURVE_VALUES, range: None, display_range: None,
        help: "The curve by which your playing strength will affect the cutoff frequency. Normally choose TONE (the optimal curve for the selected tone). Choose FIX if you don't want the cutoff to be affected." },
    PcmTailParamEntry { offset: 0x05, group: PcmTailGroup::Filter, name: "Cutoff Keyfollow",
        values: &[], range: None, display_range: Some((-200, 200)),
        help: "Specifies how the pitch of the note you play will affect the cutoff frequency. With positive values, the cutoff will rise as you play higher notes." },
    PcmTailParamEntry { offset: 0x06, group: PcmTailGroup::Filter, name: "Cutoff Nuance Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Specifies how nuances of your performance will affect the filter cutoff frequency." },
    // ---- TVF group (0x07..=0x0D) ----
    PcmTailParamEntry { offset: 0x07, group: PcmTailGroup::Tvf, name: "TVF Env Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Depth of the TVF envelope. Higher values increase the change produced by the TVF envelope." },
    PcmTailParamEntry { offset: 0x08, group: PcmTailGroup::Tvf, name: "TVF Attack Time",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Attack time of the filter envelope." },
    PcmTailParamEntry { offset: 0x09, group: PcmTailGroup::Tvf, name: "TVF Decay Time",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Decay time of the filter envelope." },
    PcmTailParamEntry { offset: 0x0A, group: PcmTailGroup::Tvf, name: "TVF Sustain Level",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Sustain level of the filter envelope." },
    PcmTailParamEntry { offset: 0x0B, group: PcmTailGroup::Tvf, name: "TVF Release Time",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Release time of the filter envelope." },
    PcmTailParamEntry { offset: 0x0C, group: PcmTailGroup::Tvf, name: "TVF Attack Velocity Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How your playing strength affects the filter attack time. Positive values shorten the attack on stronger playing." },
    PcmTailParamEntry { offset: 0x0D, group: PcmTailGroup::Tvf, name: "TVF Attack Nuance Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How nuances of your performance will affect the filter attack time." },
    // ---- Velocity group (0x0E..=0x0F) ----
    PcmTailParamEntry { offset: 0x0E, group: PcmTailGroup::Velocity, name: "Level Velocity Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Amount by which the tone's volume is affected by your playing strength. Positive values: volume increases on stronger playing." },
    PcmTailParamEntry { offset: 0x0F, group: PcmTailGroup::Velocity, name: "Velocity Curve Type",
        values: VELOCITY_CURVE_VALUES, range: None, display_range: None,
        help: "Curve by which your playing strength affects the tone's volume. TONE chooses the optimal curve for the selected tone; FIX disables velocity-to-volume." },
    // ---- TVA group (0x10..=0x16) ----
    PcmTailParamEntry { offset: 0x10, group: PcmTailGroup::Tva, name: "TVA Attack Time",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Attack time of the amp envelope." },
    PcmTailParamEntry { offset: 0x11, group: PcmTailGroup::Tva, name: "TVA Decay Time",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Decay time of the amp envelope." },
    PcmTailParamEntry { offset: 0x12, group: PcmTailGroup::Tva, name: "TVA Sustain Level",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Sustain level of the amp envelope." },
    PcmTailParamEntry { offset: 0x13, group: PcmTailGroup::Tva, name: "TVA Release Time",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Release time of the amp envelope." },
    PcmTailParamEntry { offset: 0x14, group: PcmTailGroup::Tva, name: "TVA Attack Velocity Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How your playing strength affects the amp attack time. Positive values shorten the attack on stronger playing." },
    PcmTailParamEntry { offset: 0x15, group: PcmTailGroup::Tva, name: "TVA Attack Nuance Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How nuances of your performance will affect the amp attack time." },
    PcmTailParamEntry { offset: 0x16, group: PcmTailGroup::Tva, name: "TVA Level Nuance Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How nuances of your performance will affect the volume." },
    // ---- PITCH ENV group (0x17..=0x1A) ----
    PcmTailParamEntry { offset: 0x17, group: PcmTailGroup::PitchEnv, name: "Pitch Env Velocity Sens",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How your playing strength affects the depth of the pitch envelope. Positive values: stronger playing increases pitch-envelope change." },
    PcmTailParamEntry { offset: 0x18, group: PcmTailGroup::PitchEnv, name: "Pitch Env Depth",
        values: &[], range: Some((0x00, 0x18)), display_range: Some((-12, 12)),
        help: "Depth of the pitch envelope. Higher values increase the change produced by the pitch envelope (in semitones)." },
    PcmTailParamEntry { offset: 0x19, group: PcmTailGroup::PitchEnv, name: "Pitch Attack Time",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Attack time of the pitch envelope." },
    PcmTailParamEntry { offset: 0x1A, group: PcmTailGroup::PitchEnv, name: "Pitch Decay Time",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "Decay time of the pitch envelope." },
    // ---- Portamento Type (0x1B) ----
    PcmTailParamEntry { offset: 0x1B, group: PcmTailGroup::Portamento, name: "Portamento Type",
        values: PORTAMENTO_TYPE_VALUES, range: None, display_range: None,
        help: "RATE: pitch change time is proportional to the interval. TIME: pitch change takes the same time regardless of interval." },
    // ---- LFO1 group (0x1C..=0x21) ----
    PcmTailParamEntry { offset: 0x1C, group: PcmTailGroup::Lfo, name: "LFO1 Rate",
        values: &[], range: None, display_range: None,
        help: "LFO rate. 0–100 sets a fixed speed; the manual also documents BPM note-value sentinels (whole/half/quarter/triplet) and TONE — encoding not yet captured here." },
    PcmTailParamEntry { offset: 0x1D, group: PcmTailGroup::Reserved, name: "",
        values: &[], range: None, display_range: None, help: "" },
    PcmTailParamEntry { offset: 0x1E, group: PcmTailGroup::Lfo, name: "LFO1 Pitch Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How the LFO affects the pitch. OFF disables LFO-to-pitch routing." },
    PcmTailParamEntry { offset: 0x1F, group: PcmTailGroup::Lfo, name: "LFO1 TVF Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How the LFO affects the filter cutoff frequency." },
    PcmTailParamEntry { offset: 0x20, group: PcmTailGroup::Lfo, name: "LFO1 TVA Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How the LFO affects the volume." },
    PcmTailParamEntry { offset: 0x21, group: PcmTailGroup::Lfo, name: "LFO1 Pan Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How the LFO affects pan (stereo position)." },
    // ---- LFO2 group (0x22..=0x27) ----
    PcmTailParamEntry { offset: 0x22, group: PcmTailGroup::Lfo, name: "LFO2 Rate",
        values: &[], range: None, display_range: None,
        help: "Second LFO rate. Same value space as LFO1 Rate." },
    PcmTailParamEntry { offset: 0x23, group: PcmTailGroup::Reserved, name: "",
        values: &[], range: None, display_range: None, help: "" },
    PcmTailParamEntry { offset: 0x24, group: PcmTailGroup::Lfo, name: "LFO2 Pitch Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How LFO2 affects the pitch." },
    PcmTailParamEntry { offset: 0x25, group: PcmTailGroup::Lfo, name: "LFO2 TVF Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How LFO2 affects the filter cutoff." },
    PcmTailParamEntry { offset: 0x26, group: PcmTailGroup::Lfo, name: "LFO2 TVA Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How LFO2 affects the volume." },
    PcmTailParamEntry { offset: 0x27, group: PcmTailGroup::Lfo, name: "LFO2 Pan Depth",
        values: &[], range: SYM_50_RANGE, display_range: SYM_50_DISPLAY,
        help: "How LFO2 affects pan (stereo position)." },
];

// Suppress dead-code lint for constants not referenced from outside the
// PCM_TAIL_PARAMS array (the named-value tables and the symmetric range
// helpers — they're all used in the array literal).
#[allow(dead_code)]
fn _silence_dead_code_lint() {
    let _ = (
        FILTER_TYPE_VALUES,
        VELOCITY_CURVE_VALUES,
        PORTAMENTO_TYPE_VALUES,
        RELEASE_MODE_VALUES,
        SYM_50_RANGE,
        SYM_50_DISPLAY,
    );
}

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
    fn enriched_metadata_covers_known_params() {
        // Filter Type is enumerated, not range-bound.
        let ft = &PCM_TAIL_PARAMS[0x00];
        assert_eq!(ft.values.len(), 8);
        assert_eq!(ft.values[0], (0x00, "OFF"));
        assert_eq!(ft.values[7], (0x07, "TONE"));
        assert!(ft.range.is_none());
        assert!(ft.help.contains("LPF: low-pass"));

        // Cutoff is symmetric -50..=+50 numeric.
        let cutoff = &PCM_TAIL_PARAMS[0x01];
        assert!(cutoff.values.is_empty());
        assert_eq!(cutoff.range, Some((0x00, 0x64)));
        assert_eq!(cutoff.display_range, Some((-50, 50)));
        assert!(cutoff.help.starts_with("Specifies the frequency"));

        // Pitch Env Depth uses a different range (-12..=+12 semitones).
        let pe_depth = &PCM_TAIL_PARAMS[0x18];
        assert_eq!(pe_depth.range, Some((0x00, 0x18)));
        assert_eq!(pe_depth.display_range, Some((-12, 12)));

        // Portamento Type is a 2-value enum.
        let pt = &PCM_TAIL_PARAMS[0x1B];
        assert_eq!(pt.values, &[(0x00, "RATE"), (0x01, "TIME")]);
        assert!(pt.help.contains("RATE") && pt.help.contains("TIME"));

        // Reserved bytes carry empty metadata.
        let reserved = &PCM_TAIL_PARAMS[0x1D];
        assert!(reserved.values.is_empty());
        assert!(reserved.range.is_none());
        assert_eq!(reserved.help, "");
    }

    #[test]
    fn param_for_returns_none_beyond_documented_range() {
        assert!(param_for(0x28).is_none());
        assert!(param_for(0x7F).is_none());
        assert_eq!(param_for(0x00).map(|p| p.name), Some("Filter Type"));
        assert_eq!(param_for(0x27).map(|p| p.name), Some("LFO2 Pan Depth"));
    }
}
