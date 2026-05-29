//! Per-parameter help text for the GR-55 protocol, looked up by the
//! parameter's canonical name (the `name` field on each ParamEntry-style
//! struct in [`crate::mfx_params`], [`crate::mod_params`],
//! [`crate::modeling_params`], [`crate::pcm_tail_params`], etc.).
//!
//! Source: Roland's GR-55 Owner's Manual, sections "Editing the Tones
//! (TONE)" (pp. 25-29) and "Effect Settings (EFFECT)" (pp. 41-53).
//! Descriptions are kept tooltip-friendly — one or two sentences.
//!
//! The table is **sorted by name** so [`help_for`] can binary-search
//! it. Adding a new entry: insert at the correct sorted position and
//! `cargo test help_table_is_sorted` will keep you honest.

/// Look up tooltip-friendly help text for a parameter by its canonical
/// name. Returns `None` if no entry is registered.
///
/// Names match the `name` field on the per-block param tables (e.g.
/// `mfx_params::MFX_PARAMS[i].name`), so callers can plumb help text
/// through the existing iterator APIs without renaming anything.
pub fn help_for(name: &str) -> Option<&'static str> {
    HELP_ENTRIES
        .binary_search_by_key(&name, |(k, _)| *k)
        .ok()
        .map(|i| HELP_ENTRIES[i].1)
}

/// Number of registered help entries. Useful for tests and bookkeeping.
pub fn entries() -> usize {
    HELP_ENTRIES.len()
}

/// Sorted (name, help) pairs. Kept alphabetical for binary search; see
/// [`help_for`].
const HELP_ENTRIES: &[(&str, &str)] = &[
    // -------- A --------
    // -------- B --------
    // -------- C --------
    ("Chorus Send Amp/Mod", "Amount of MOD output sent to the chorus effect (0\u{2013}100)."),
    ("Chorus Send MFX", "Amount of MFX output sent to the chorus effect (0\u{2013}100)."),
    ("Chromatic", "When ON, the tone's pitch will change only in semitone steps even if you bend a string."),
    ("Compressor Attack", "Compressor attack time (how quickly compression engages)."),
    ("Compressor Level", "Output level after compression."),
    ("Compressor Sustain", "Compression amount. Higher values increase sustain."),
    ("Cutoff", "Frequency at which the filter begins to act."),
    ("Cutoff Keyfollow", "How the played note's pitch affects the filter cutoff. Positive values raise the cutoff for higher notes."),
    ("Cutoff Nuance Sens", "How playing nuance (soft-touch detection) modulates the filter cutoff."),
    ("Cutoff Velocity Curve", "Velocity-to-cutoff response curve. FIX = no velocity tracking; 1\u{2013}7 = preset curves; TONE = optimal curve for the selected tone."),
    ("Cutoff Velocity Sens", "How playing strength affects the filter cutoff. Positive values raise the cutoff with stronger picking."),
    // -------- D --------
    ("Delay Send Amp/Mod", "Amount of MOD output sent to the delay effect (0\u{2013}100)."),
    ("Delay Send MFX", "Amount of MFX output sent to the delay effect (0\u{2013}100)."),
    ("Distortion Drive", "Amount of distortion. Higher values produce more saturation."),
    ("Distortion Level", "Output level of the distortion stage."),
    ("Distortion Tone", "Tone control after distortion. Negative = warmer, positive = brighter."),
    ("Distortion Type", "Distortion algorithm (clean booster, classic overdrive, hi-gain, etc.)."),
    // -------- E --------
    ("Equalizer High Freq", "Frequency at which the High band acts."),
    ("Equalizer High Gain", "Boost or cut for the High band."),
    ("Equalizer Level", "Output level of the EQ section."),
    ("Equalizer Low Freq", "Frequency at which the Low band acts (200Hz or 400Hz)."),
    ("Equalizer Low Gain", "Boost or cut for the Low band (\u{2212}15 to +15 dB)."),
    ("Equalizer Mid 1 Freq", "Centre frequency for the Mid 1 peaking band."),
    ("Equalizer Mid 1 Gain", "Boost or cut for the Mid 1 band."),
    ("Equalizer Mid 1 Q", "Width of the Mid 1 peaking band. Higher Q = narrower band."),
    ("Equalizer Mid 2 Freq", "Centre frequency for the Mid 2 peaking band."),
    ("Equalizer Mid 2 Gain", "Boost or cut for the Mid 2 band."),
    ("Equalizer Mid 2 Q", "Width of the Mid 2 peaking band. Higher Q = narrower band."),
    // -------- F --------
    ("Filter Type", "Filter mode: OFF, LPF (lows pass), BPF (band around cutoff), HPF (highs pass), PKG (peaking emphasises cutoff), LPF2/LPF3 (gentler-slope low-pass variants), or TONE (best curve for the selected tone)."),
    // -------- L --------
    ("LFO1 Pan Depth", "How much LFO1 modulates the tone's stereo position."),
    ("LFO1 Pitch Depth", "How much LFO1 modulates pitch (vibrato amount)."),
    ("LFO1 Rate", "LFO1 speed. Can be a free-run value (0\u{2013}100), a BPM-synced note value, or TONE (the rate baked into the selected tone)."),
    ("LFO1 TVA Depth", "How much LFO1 modulates volume (tremolo amount)."),
    ("LFO1 TVF Depth", "How much LFO1 modulates the filter cutoff (wah-like sweeping)."),
    ("LFO2 Pan Depth", "How much LFO2 modulates the tone's stereo position."),
    ("LFO2 Pitch Depth", "How much LFO2 modulates pitch."),
    ("LFO2 Rate", "LFO2 speed. Free-run, BPM-synced, or TONE."),
    ("LFO2 TVA Depth", "How much LFO2 modulates volume."),
    ("LFO2 TVF Depth", "How much LFO2 modulates the filter cutoff."),
    ("Legato", "When ON, hammer-ons and pull-offs change only pitch \u{2014} the attack of the next note isn't re-triggered. Requires CHROMATIC ON."),
    ("Level Velocity Sens", "How playing strength affects the tone's volume. Positive = louder with stronger picking."),
    ("Limiter Level", "Output level of the limiter."),
    ("Limiter Release", "How quickly the limiter releases after clamping."),
    ("Limiter Threshold", "Level above which the limiter clamps the signal."),
    // -------- M --------
    ("MOD: Type", "Selects the MOD effect type."),
    ("Modeling 12 String", "Adds a second virtual string an octave above each main string."),
    ("Modeling Tone Level", "Output level of the modelling engine."),
    ("Modeling Tone Sw", "Enables or bypasses the modelling engine."),
    // -------- N --------
    ("Nuance Sw", "Master switch for nuance detection. When OFF, the various Nuance Sens parameters have no effect."),
    // -------- O --------
    ("Octave", "Shifts the tone's pitch in 1-octave steps (\u{2212}3 to +3)."),
    // -------- P --------
    ("Pan", "Stereo position. L50 = full left, center = centered, R50 = full right."),
    ("Pitch Attack Time", "Attack time of the pitch envelope (how quickly the pitch reaches its target)."),
    ("Pitch Decay Time", "Decay time of the pitch envelope (how quickly the pitch returns to nominal)."),
    ("Pitch Env Depth", "Depth of the pitch envelope (\u{2212}12 to +12 semitones)."),
    ("Pitch Env Velocity Sens", "How playing strength affects the pitch envelope's depth. Positive values produce more pitch movement with stronger picking."),
    ("Pitch Fine", "Fine pitch adjustment in 1-cent steps (\u{2212}50 to +50, i.e. up to half a semitone)."),
    ("Pitch Shift", "Tone pitch in semitones (\u{2212}24 to +24, up to two octaves)."),
    ("Portamento Type", "RATE = pitch-change duration is proportional to interval. TIME = each pitch change takes the same wall-clock time regardless of interval size."),
    // -------- R --------
    ("Release Mode", "1: the next note's release continues a previously-played note on the same string. 2: any previously-played note on the same string is forcibly decayed before the next note sounds."),
    ("Resonance", "Cutoff-frequency emphasis. Higher values may produce self-oscillation."),
    ("Reverb Send Amp/Mod", "Amount of MOD output sent to the reverb effect (0\u{2013}100)."),
    ("Reverb Send MFX", "Amount of MFX output sent to the reverb effect (0\u{2013}100)."),
    // -------- S --------
    ("Super Filter Cutoff", "Frequency at which the filter takes effect."),
    ("Super Filter Resonance", "Emphasises the cutoff frequency. Raising too high may cause oscillation."),
    ("Super Filter Slope", "Filter steepness. Higher slope = sharper cutoff."),
    ("Super Filter Type", "Filter mode: LPF (lows pass), BPF (band pass), HPF (highs pass), or Notch."),
    ("Switch Effect", "Enables or bypasses the MFX effect."),
    // -------- T --------
    ("TVA Attack Nuance Sens", "How playing nuance affects the amp envelope's attack time."),
    ("TVA Attack Time", "Attack time of the amp envelope (\u{2212}50 to +50 relative to the tone's default)."),
    ("TVA Attack Velocity Sens", "How playing strength affects attack time. Positive values shorten the attack with stronger picking."),
    ("TVA Decay Time", "Decay time of the amp envelope."),
    ("TVA Level Nuance Sens", "How playing nuance affects the tone's overall volume."),
    ("TVA Release Time", "Release time of the amp envelope."),
    ("TVA Sustain Level", "Sustain level of the amp envelope."),
    ("TVF Attack Nuance Sens", "How playing nuance affects the filter envelope's attack time."),
    ("TVF Attack Time", "Attack time of the filter envelope."),
    ("TVF Attack Velocity Sens", "How playing strength affects the filter envelope's attack time."),
    ("TVF Decay Time", "Decay time of the filter envelope."),
    ("TVF Env Depth", "Depth of the filter envelope. Higher values produce a more pronounced envelope-driven sweep."),
    ("TVF Release Time", "Release time of the filter envelope."),
    ("TVF Sustain Level", "Sustain level of the filter envelope."),
    ("Type", "Selects the MFX effect type. Each type produces a different effect; switching types preserves the other types' parameters."),
    // -------- V --------
    ("Velocity Curve Type", "Velocity-to-volume response curve. FIX = no velocity tracking; 1\u{2013}7 = preset curves; TONE = optimal curve for the selected tone."),
    // -------- W --------
    ("Wah Frequency", "Centre frequency of the wah filter."),
    ("Wah Level", "Output level of the wah."),
    ("Wah Mode", "How the wah is controlled (manual position, auto, or pedal-controlled)."),
    ("Wah Peak", "Resonance / Q of the wah filter."),
    ("Wah Pedal Position", "Manual wah filter position when not pedal-controlled."),
    ("Wah Sens", "Sensitivity to input level for auto-wah modes."),
    ("Wah Type", "Wah voicing \u{2014} different filter responses simulating classic wah pedals."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_table_is_sorted() {
        for w in HELP_ENTRIES.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "HELP_ENTRIES must stay sorted by name for binary_search; \
                 found {:?} before {:?}",
                w[0].0,
                w[1].0,
            );
        }
    }

    #[test]
    fn help_lookups_hit_and_miss() {
        // Known hits across MFX/MOD/PCM-tail families.
        assert!(help_for("Filter Type").unwrap().contains("LPF"));
        assert!(help_for("LFO1 Rate").unwrap().contains("BPM"));
        assert!(help_for("Distortion Drive").unwrap().contains("saturation"));
        assert!(help_for("Equalizer Low Gain").unwrap().contains("Low band"));
        // Miss returns None cleanly.
        assert!(help_for("Definitely Not A Param").is_none());
    }

    #[test]
    fn no_duplicate_names() {
        let mut names: Vec<&str> = HELP_ENTRIES.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), HELP_ENTRIES.len(), "duplicate names in HELP_ENTRIES");
    }
}
