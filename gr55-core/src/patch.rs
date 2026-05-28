//! Typed model for a single GR-55 patch payload.
//!
//! The Patch model is a parallel of [`crate::system::SystemArea`] for the
//! address range described by FloorBoard `midi.xml` under `<Structure>`
//! (lines 37143-55307). A single patch is laid out across 14 LSB "pages":
//!
//! | LSB  | Meaning                            |
//! | ---- | ---------------------------------- |
//! | 0x00 | Names and Pedal (patch name, CTL/EXP/GK pedal assignments) |
//! | 0x01 | Master Assign 1-6                  |
//! | 0x02 | Master Assign 7-8                  |
//! | 0x03 | MFX                                |
//! | 0x04 | MFX 2                              |
//! | 0x05 | reserved (blank)                   |
//! | 0x06 | Chorus / Delay / Reverb / EQ       |
//! | 0x07 | Preamp / NS / MOD                  |
//! | 0x10 | Guitar Modeling                    |
//! | 0x11 | Bass Mode Modeling                 |
//! | 0x20 | PCM-1-A                            |
//! | 0x21 | PCM-2-A                            |
//! | 0x30 | PCM-1-B                            |
//! | 0x31 | PCM-2-B                            |
//!
//! The same byte layout is reachable at three different address MSBs:
//! `0x60` (temporary edit buffer), `0x20..=0x2C` and `0x30..=0x3B` (USER /
//! PRESET patch slots — see [`crate::address::PatchSlot`]), and `0x18`
//! (FloorBoard's file-format canonical address). `PatchArea` is MSB-agnostic
//! — callers supply the base MSB to [`PatchArea::from_frames_at`] and
//! [`PatchArea::to_frames`].
//!
//! Initial typing scope (this commit):
//!
//! - `mode` — the Guitar/Bass mode discriminator at page `0x00` offset `0x00`.
//! - `name` — the 16-char patch name spread across page `0x00` offsets
//!   `0x01..=0x10`.
//!
//! Everything else lands in `unknown_bytes`, keyed by `"page:hi:lo"`. The
//! decoder is non-lossy: round-tripping a frame stream through
//! `from_frames_at`/`to_frames` reproduces the input addresses byte-for-byte.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::codec::CodecError;
use crate::sysex::Frame;
use crate::system::{HoldType, OnOff, PitchBendDepth, SwitchMode};

/// 16-char patch name (ASCII 0x20..=0x7D, the printable subset FloorBoard
/// allows). Stored as raw bytes so that round-trip preserves any byte the
/// device happens to emit, including pad spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatchName(#[serde(with = "patch_name_serde")] pub [u8; 16]);

impl Default for PatchName {
    fn default() -> Self {
        Self([0x20; 16])
    }
}

impl PatchName {
    /// True if every byte is the pad space `0x20`.
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|b| *b == 0x20)
    }

    /// Render the name as a `String`, replacing any non-ASCII byte with `?`.
    pub fn as_string(&self) -> String {
        self.0
            .iter()
            .map(|b| {
                if (0x20..=0x7D).contains(b) {
                    *b as char
                } else {
                    '?'
                }
            })
            .collect()
    }
}

impl std::str::FromStr for PatchName {
    type Err = PatchNameError;
    fn from_str(s: &str) -> Result<Self, PatchNameError> {
        let bytes = s.as_bytes();
        if bytes.len() > 16 {
            return Err(PatchNameError::TooLong(bytes.len()));
        }
        for (i, b) in bytes.iter().enumerate() {
            if !(0x20..=0x7D).contains(b) {
                return Err(PatchNameError::NotPrintable {
                    index: i,
                    byte: *b,
                });
            }
        }
        let mut out = [0x20_u8; 16];
        out[..bytes.len()].copy_from_slice(bytes);
        Ok(PatchName(out))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PatchNameError {
    #[error("patch name is {0} bytes; max is 16")]
    TooLong(usize),
    #[error("byte 0x{byte:02X} at index {index} is not in the printable range 0x20..=0x7D")]
    NotPrintable { index: usize, byte: u8 },
}

mod patch_name_serde {
    use super::PatchName;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::str::FromStr;

    pub fn serialize<S: Serializer>(arr: &[u8; 16], s: S) -> Result<S::Ok, S::Error> {
        let view = PatchName(*arr);
        s.serialize_str(view.as_string().trim_end())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 16], D::Error> {
        let s = String::deserialize(d)?;
        let parsed = PatchName::from_str(&s).map_err(serde::de::Error::custom)?;
        Ok(parsed.0)
    }
}

/// Guitar/Bass mode discriminator at page `0x00` offset `0x00`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchMode {
    Guitar,
    Bass,
}

impl PatchMode {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Guitar),
            0x01 => Some(Self::Bass),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Guitar => 0x00,
            Self::Bass => 0x01,
        }
    }
}

/// Patch CTL pedal function (page `0x00` offset `0x12`).
///
/// Distinct from the System-area `CtlPedalFunction` — the patch enum has 17
/// variants instead of 22 and includes `LedMoment` / `LedToggle` (LED
/// behaviour for the CTL switch) while omitting the System-area navigation
/// functions (Sound Style / Bank Number / Patch Number Inc/Dec) and the
/// `Patch Setting` variant. Mined from FloorBoard `midi.xml:38690-38708`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CtlFunction {
    Off,
    Hold,
    TapTempo,
    ToneSw,
    AmpSw,
    ModSw,
    MfxSw,
    DelaySw,
    ReverbSw,
    ChorusSw,
    AudioPlayerPlayStop,
    AudioPlayerSongInc,
    AudioPlayerSongDec,
    AudioPlayerSw,
    VLinkSw,
    LedMoment,
    LedToggle,
}

impl CtlFunction {
    pub fn from_byte(b: u8) -> Option<Self> {
        use CtlFunction::*;
        Some(match b {
            0x00 => Off,
            0x01 => Hold,
            0x02 => TapTempo,
            0x03 => ToneSw,
            0x04 => AmpSw,
            0x05 => ModSw,
            0x06 => MfxSw,
            0x07 => DelaySw,
            0x08 => ReverbSw,
            0x09 => ChorusSw,
            0x0A => AudioPlayerPlayStop,
            0x0B => AudioPlayerSongInc,
            0x0C => AudioPlayerSongDec,
            0x0D => AudioPlayerSw,
            0x0E => VLinkSw,
            0x0F => LedMoment,
            0x10 => LedToggle,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use CtlFunction::*;
        match self {
            Off => 0x00,
            Hold => 0x01,
            TapTempo => 0x02,
            ToneSw => 0x03,
            AmpSw => 0x04,
            ModSw => 0x05,
            MfxSw => 0x06,
            DelaySw => 0x07,
            ReverbSw => 0x08,
            ChorusSw => 0x09,
            AudioPlayerPlayStop => 0x0A,
            AudioPlayerSongInc => 0x0B,
            AudioPlayerSongDec => 0x0C,
            AudioPlayerSw => 0x0D,
            VLinkSw => 0x0E,
            LedMoment => 0x0F,
            LedToggle => 0x10,
        }
    }
}

/// Patch EXP pedal function (page `0x00` offset `0x1F`).
///
/// 10 variants, mined from FloorBoard `midi.xml:38759-38770`. Differs from
/// the System-area `ExpPedalFunction` (which has 11 variants) by omitting
/// the `PatchSetting` option at byte 0x01 — at the patch level a "patch
/// setting" assignment would be self-referential. All other variants are
/// shifted down by one byte relative to the System enum:
///   patch 0x01 PatchVolume   == system 0x02 PatchVolume
///   patch 0x09 ModControl    == system 0x0A ModControl
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpFunction {
    Off,
    PatchVolume,
    ToneVolume,
    PitchBend,
    Modulation,
    CrossFader,
    DelayLevel,
    ReverbLevel,
    ChorusLevel,
    ModControl,
}

impl ExpFunction {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ExpFunction::*;
        Some(match b {
            0x00 => Off,
            0x01 => PatchVolume,
            0x02 => ToneVolume,
            0x03 => PitchBend,
            0x04 => Modulation,
            0x05 => CrossFader,
            0x06 => DelayLevel,
            0x07 => ReverbLevel,
            0x08 => ChorusLevel,
            0x09 => ModControl,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use ExpFunction::*;
        match self {
            Off => 0x00,
            PatchVolume => 0x01,
            ToneVolume => 0x02,
            PitchBend => 0x03,
            Modulation => 0x04,
            CrossFader => 0x05,
            DelayLevel => 0x06,
            ReverbLevel => 0x07,
            ChorusLevel => 0x08,
            ModControl => 0x09,
        }
    }
}

/// Direction an EXP pedal's cross-fader assignment sweeps. Per-output:
/// PCM 1 / PCM 2 / Modeling / Normal PU each get their own setting at
/// `0x2C..=0x2F`. Mined from FloorBoard `midi.xml:38818-38835`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossFaderMode {
    Off,
    Toe,
    Heel,
}

impl CrossFaderMode {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Off),
            0x01 => Some(Self::Toe),
            0x02 => Some(Self::Heel),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Off => 0x00,
            Self::Toe => 0x01,
            Self::Heel => 0x02,
        }
    }
}

/// Patch EXP pedal switch function (page `0x00` offset `0x4E`).
///
/// 14 variants. A strict subset of [`CtlFunction`] — it omits `Hold` and
/// the two LED behaviours, and a strict subset of System
/// `ExpPedalSwitchFunction` — it omits `PatchSetting`, the four navigation
/// variants (Sound Style / Bank Number / Patch Number Inc/Dec). Mined from
/// FloorBoard `midi.xml:38953-38968`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpSwFunction {
    Off,
    TapTempo,
    ToneSw,
    AmpSw,
    ModSw,
    MfxSw,
    DelaySw,
    ReverbSw,
    ChorusSw,
    AudioPlayerPlayStop,
    AudioPlayerSongInc,
    AudioPlayerSongDec,
    AudioPlayerSw,
    VLinkSw,
}

impl ExpSwFunction {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ExpSwFunction::*;
        Some(match b {
            0x00 => Off,
            0x01 => TapTempo,
            0x02 => ToneSw,
            0x03 => AmpSw,
            0x04 => ModSw,
            0x05 => MfxSw,
            0x06 => DelaySw,
            0x07 => ReverbSw,
            0x08 => ChorusSw,
            0x09 => AudioPlayerPlayStop,
            0x0A => AudioPlayerSongInc,
            0x0B => AudioPlayerSongDec,
            0x0C => AudioPlayerSw,
            0x0D => VLinkSw,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use ExpSwFunction::*;
        match self {
            Off => 0x00,
            TapTempo => 0x01,
            ToneSw => 0x02,
            AmpSw => 0x03,
            ModSw => 0x04,
            MfxSw => 0x05,
            DelaySw => 0x06,
            ReverbSw => 0x07,
            ChorusSw => 0x08,
            AudioPlayerPlayStop => 0x09,
            AudioPlayerSongInc => 0x0A,
            AudioPlayerSongDec => 0x0B,
            AudioPlayerSw => 0x0C,
            VLinkSw => 0x0D,
        }
    }
}

/// Typed view of a single GR-55 patch payload. MSB-agnostic — the caller
/// supplies the base MSB when decoding or encoding.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchArea {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<PatchMode>,
    #[serde(default, skip_serializing_if = "PatchName::is_empty")]
    pub name: PatchName,

    // ---- CTL pedal block (page 0x00 offsets 0x11..=0x1E) ----
    /// CTL Status at `0x11`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_status: Option<OnOff>,
    /// CTL Function at `0x12`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_function: Option<CtlFunction>,
    /// CTL Hold Type at `0x13` (Type 1..=4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_hold_type: Option<HoldType>,
    /// CTL Hold Switch Mode at `0x14` (Latch / Moment).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_switch_mode: Option<SwitchMode>,
    /// CTL Hold PCM 1 at `0x15`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_hold_pcm_1: Option<OnOff>,
    /// CTL Hold PCM 2 at `0x16`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_hold_pcm_2: Option<OnOff>,
    /// CTL Tone Sw OFF: PCM 1 at `0x17`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_off_pcm_1: Option<OnOff>,
    /// CTL Tone Sw OFF: PCM 2 at `0x18`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_off_pcm_2: Option<OnOff>,
    /// CTL Tone Sw OFF: Modeling at `0x19`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_off_modeling: Option<OnOff>,
    /// CTL Tone Sw OFF: Normal PU at `0x1A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_off_normal_pu: Option<OnOff>,
    /// CTL Tone Sw ON: PCM 1 at `0x1B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_on_pcm_1: Option<OnOff>,
    /// CTL Tone Sw ON: PCM 2 at `0x1C`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_on_pcm_2: Option<OnOff>,
    /// CTL Tone Sw ON: Modeling at `0x1D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_on_modeling: Option<OnOff>,
    /// CTL Tone Sw ON: Normal PU at `0x1E`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_on_normal_pu: Option<OnOff>,

    // ---- EXP pedal block (page 0x00 offsets 0x1F..=0x35) ----
    /// EXP Function at `0x1F`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_function: Option<ExpFunction>,
    /// EXP Tone Volume: PCM 1 at `0x20`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_tone_vol_pcm_1: Option<OnOff>,
    /// EXP Tone Volume: PCM 2 at `0x21`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_tone_vol_pcm_2: Option<OnOff>,
    /// EXP Tone Volume: Modeling at `0x22`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_tone_vol_modeling: Option<OnOff>,
    /// EXP Tone Volume: Normal PU at `0x23`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_tone_vol_normal_pu: Option<OnOff>,
    /// EXP Pitch Bend Depth at `0x24` (-12..=+12 semitones).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pitch_bend_depth: Option<PitchBendDepth>,
    /// EXP Pitch Bend: PCM 1 at `0x25`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pitch_bend_pcm_1: Option<OnOff>,
    /// EXP Pitch Bend: PCM 2 at `0x26`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pitch_bend_pcm_2: Option<OnOff>,
    /// EXP Pitch Bend: Modeling at `0x27`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pitch_bend_modeling: Option<OnOff>,
    /// EXP Modulation MIN at `0x28` (raw 0..=127, display 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_mod_min: Option<u8>,
    /// EXP Modulation MAX at `0x29`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_mod_max: Option<u8>,
    /// EXP Modulation: PCM 1 at `0x2A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_mod_pcm_1: Option<OnOff>,
    /// EXP Modulation: PCM 2 at `0x2B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_mod_pcm_2: Option<OnOff>,
    /// EXP Cross Fader: PCM 1 at `0x2C` (Off / Toe / Heel).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_cross_fader_pcm_1: Option<CrossFaderMode>,
    /// EXP Cross Fader: PCM 2 at `0x2D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_cross_fader_pcm_2: Option<CrossFaderMode>,
    /// EXP Cross Fader: Modeling at `0x2E`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_cross_fader_modeling: Option<CrossFaderMode>,
    /// EXP Cross Fader: Normal PU at `0x2F`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_cross_fader_normal_pu: Option<CrossFaderMode>,
    /// EXP Delay Level MIN at `0x30` (raw 0..=120).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_delay_min: Option<u8>,
    /// EXP Delay Level MAX at `0x31`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_delay_max: Option<u8>,
    /// EXP Reverb Level MIN at `0x32` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_reverb_min: Option<u8>,
    /// EXP Reverb Level MAX at `0x33`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_reverb_max: Option<u8>,
    /// EXP Chorus Level MIN at `0x34` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_chorus_min: Option<u8>,
    /// EXP Chorus Level MAX at `0x35`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_chorus_max: Option<u8>,

    // ---- EXP ON block (page 0x00 offsets 0x36..=0x4C) ----
    // Mirrors the EXP block field-for-field; same enums and ranges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_function: Option<ExpFunction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_tone_vol_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_tone_vol_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_tone_vol_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_tone_vol_normal_pu: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_pitch_bend_depth: Option<PitchBendDepth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_pitch_bend_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_pitch_bend_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_pitch_bend_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_mod_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_mod_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_mod_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_mod_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_cross_fader_pcm_1: Option<CrossFaderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_cross_fader_pcm_2: Option<CrossFaderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_cross_fader_modeling: Option<CrossFaderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_cross_fader_normal_pu: Option<CrossFaderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_delay_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_delay_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_reverb_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_reverb_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_chorus_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_chorus_max: Option<u8>,

    // ---- EXP SW block (page 0x00 offsets 0x4D..=0x5A) ----
    // Offsets 0x4F..=0x52 are FloorBoard placeholders with no PARAM body —
    // they fall through to unknown_bytes.
    /// EXP SW Status at `0x4D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_status: Option<OnOff>,
    /// EXP SW Function at `0x4E` (14 variants, no Hold).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_function: Option<ExpSwFunction>,
    /// EXP SW Tone Sw OFF: PCM 1 at `0x53`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_off_pcm_1: Option<OnOff>,
    /// EXP SW Tone Sw OFF: PCM 2 at `0x54`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_off_pcm_2: Option<OnOff>,
    /// EXP SW Tone Sw OFF: Modeling at `0x55`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_off_modeling: Option<OnOff>,
    /// EXP SW Tone Sw OFF: Normal PU at `0x56`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_off_normal_pu: Option<OnOff>,
    /// EXP SW Tone Sw ON: PCM 1 at `0x57`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_on_pcm_1: Option<OnOff>,
    /// EXP SW Tone Sw ON: PCM 2 at `0x58`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_on_pcm_2: Option<OnOff>,
    /// EXP SW Tone Sw ON: Modeling at `0x59`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_on_modeling: Option<OnOff>,
    /// EXP SW Tone Sw ON: Normal PU at `0x5A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_on_normal_pu: Option<OnOff>,

    // ---- GK VOLUME block (page 0x00 offsets 0x5B..=0x71) ----
    // Same shape as the EXP / EXP ON blocks; GK VOLUME's function enum is
    // identical to ExpFunction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_function: Option<ExpFunction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_tone_vol_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_tone_vol_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_tone_vol_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_tone_vol_normal_pu: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_pitch_bend_depth: Option<PitchBendDepth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_pitch_bend_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_pitch_bend_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_pitch_bend_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_mod_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_mod_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_mod_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_mod_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_cross_fader_pcm_1: Option<CrossFaderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_cross_fader_pcm_2: Option<CrossFaderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_cross_fader_modeling: Option<CrossFaderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_cross_fader_normal_pu: Option<CrossFaderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_delay_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_delay_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_reverb_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_reverb_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_chorus_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_chorus_max: Option<u8>,

    // ---- GK S1 block (page 0x00 offsets 0x72..=0x7E) ----
    // 0x73..=0x76 are FloorBoard placeholders; they fall through.
    /// GK S1 Function at `0x72`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_function: Option<ExpSwFunction>,
    /// GK S1 Tone Sw OFF: PCM 1 at `0x77`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_off_pcm_1: Option<OnOff>,
    /// GK S1 Tone Sw OFF: PCM 2 at `0x78`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_off_pcm_2: Option<OnOff>,
    /// GK S1 Tone Sw OFF: Modeling at `0x79`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_off_modeling: Option<OnOff>,
    /// GK S1 Tone Sw OFF: Normal PU at `0x7A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_off_normal_pu: Option<OnOff>,
    /// GK S1 Tone Sw ON: PCM 1 at `0x7B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_on_pcm_1: Option<OnOff>,
    /// GK S1 Tone Sw ON: PCM 2 at `0x7C`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_on_pcm_2: Option<OnOff>,
    /// GK S1 Tone Sw ON: Modeling at `0x7D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_on_modeling: Option<OnOff>,
    /// GK S1 Tone Sw ON: Normal PU at `0x7E`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_on_normal_pu: Option<OnOff>,

    // ---- GK S2 block (page 0x00 offset 0x7F + page 0x01 offsets 0x00..=0x0B) ----
    // The Function byte lives at the last offset of page 0x00; the remainder
    // wraps to page 0x01. Offsets 0x01:00..=0x01:03 are FloorBoard placeholders.
    /// GK S2 Function at page `0x00` offset `0x7F`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_function: Option<ExpSwFunction>,
    /// GK S2 Tone Sw OFF: PCM 1 at page `0x01` offset `0x04`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_off_pcm_1: Option<OnOff>,
    /// GK S2 Tone Sw OFF: PCM 2 at page `0x01` offset `0x05`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_off_pcm_2: Option<OnOff>,
    /// GK S2 Tone Sw OFF: Modeling at page `0x01` offset `0x06`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_off_modeling: Option<OnOff>,
    /// GK S2 Tone Sw OFF: Normal PU at page `0x01` offset `0x07`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_off_normal_pu: Option<OnOff>,
    /// GK S2 Tone Sw ON: PCM 1 at page `0x01` offset `0x08`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_on_pcm_1: Option<OnOff>,
    /// GK S2 Tone Sw ON: PCM 2 at page `0x01` offset `0x09`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_on_pcm_2: Option<OnOff>,
    /// GK S2 Tone Sw ON: Modeling at page `0x01` offset `0x0A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_on_modeling: Option<OnOff>,
    /// GK S2 Tone Sw ON: Normal PU at page `0x01` offset `0x0B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_on_normal_pu: Option<OnOff>,

    /// Everything inside the patch payload that the typed model doesn't yet
    /// cover. Keys are formatted `"PP:HH:LL"` — page byte, then the two
    /// in-page offset bytes.
    #[serde(default)]
    pub unknown_bytes: BTreeMap<String, u8>,
}

impl PatchArea {
    /// Decode a slice of DT1 frames into a `PatchArea`. Frames whose
    /// `address[0]` does not equal `base_msb` are ignored.
    pub fn from_frames_at(frames: &[Frame<'_>], base_msb: u8) -> Self {
        let mut area = PatchArea::default();
        for frame in frames {
            let Frame::Dt1 { address, data, .. } = frame else {
                continue;
            };
            if address[0] != base_msb {
                continue;
            }
            let mut page = address[1];
            let mut hi = address[2];
            let mut lo = address[3];
            for &b in data.iter() {
                area.store(page, hi, lo, b);
                // Roland addresses are 7-bit per byte (0x00..=0x7F).
                // Advance lo; on overflow past 0x7F, reset to 0 and carry
                // into hi; on overflow there, into page.
                lo += 1;
                if lo > 0x7F {
                    lo = 0;
                    hi += 1;
                    if hi > 0x7F {
                        hi = 0;
                        page += 1;
                    }
                }
            }
        }
        area
    }

    /// Encode this `PatchArea` into DT1 frames at the given MSB. One frame
    /// per byte for now — small and obviously correct; the [`SystemArea`]
    /// pattern is the same. The CLI can coalesce adjacent addresses later
    /// if it becomes worth doing.
    ///
    /// [`SystemArea`]: crate::system::SystemArea
    pub fn to_frames(
        &self,
        device_id: u8,
        base_msb: u8,
    ) -> Result<Vec<Frame<'static>>, CodecError> {
        let bytes = self.collect_bytes(base_msb)?;
        Ok(bytes
            .into_iter()
            .map(|(addr, b)| Frame::Dt1 {
                device_id,
                address: addr,
                data: std::borrow::Cow::Owned(vec![b]),
            })
            .collect())
    }

    fn store(&mut self, page: u8, hi: u8, lo: u8, b: u8) {
        match (page, hi, lo) {
            (0x00, 0x00, 0x00) => self.mode = PatchMode::from_byte(b),
            (0x00, 0x00, off @ 0x01..=0x10) => self.name.0[(off - 0x01) as usize] = b,
            (0x00, 0x00, 0x11) => self.ctl_status = OnOff::from_byte(b),
            (0x00, 0x00, 0x12) => self.ctl_function = CtlFunction::from_byte(b),
            (0x00, 0x00, 0x13) => self.ctl_hold_type = HoldType::from_byte(b),
            (0x00, 0x00, 0x14) => self.ctl_switch_mode = SwitchMode::from_byte(b),
            (0x00, 0x00, 0x15) => self.ctl_hold_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x16) => self.ctl_hold_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x17) => self.ctl_tone_sw_off_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x18) => self.ctl_tone_sw_off_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x19) => self.ctl_tone_sw_off_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x1A) => self.ctl_tone_sw_off_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x1B) => self.ctl_tone_sw_on_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x1C) => self.ctl_tone_sw_on_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x1D) => self.ctl_tone_sw_on_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x1E) => self.ctl_tone_sw_on_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x1F) => self.exp_function = ExpFunction::from_byte(b),
            (0x00, 0x00, 0x20) => self.exp_tone_vol_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x21) => self.exp_tone_vol_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x22) => self.exp_tone_vol_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x23) => self.exp_tone_vol_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x24) => self.exp_pitch_bend_depth = PitchBendDepth::from_byte(b),
            (0x00, 0x00, 0x25) => self.exp_pitch_bend_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x26) => self.exp_pitch_bend_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x27) => self.exp_pitch_bend_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x28) if b <= 127 => self.exp_mod_min = Some(b),
            (0x00, 0x00, 0x29) if b <= 127 => self.exp_mod_max = Some(b),
            (0x00, 0x00, 0x2A) => self.exp_mod_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x2B) => self.exp_mod_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x2C) => self.exp_cross_fader_pcm_1 = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x2D) => self.exp_cross_fader_pcm_2 = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x2E) => self.exp_cross_fader_modeling = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x2F) => self.exp_cross_fader_normal_pu = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x30) if b <= 120 => self.exp_delay_min = Some(b),
            (0x00, 0x00, 0x31) if b <= 120 => self.exp_delay_max = Some(b),
            (0x00, 0x00, 0x32) if b <= 100 => self.exp_reverb_min = Some(b),
            (0x00, 0x00, 0x33) if b <= 100 => self.exp_reverb_max = Some(b),
            (0x00, 0x00, 0x34) if b <= 100 => self.exp_chorus_min = Some(b),
            (0x00, 0x00, 0x35) if b <= 100 => self.exp_chorus_max = Some(b),
            (0x00, 0x00, 0x36) => self.exp_on_function = ExpFunction::from_byte(b),
            (0x00, 0x00, 0x37) => self.exp_on_tone_vol_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x38) => self.exp_on_tone_vol_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x39) => self.exp_on_tone_vol_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x3A) => self.exp_on_tone_vol_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x3B) => self.exp_on_pitch_bend_depth = PitchBendDepth::from_byte(b),
            (0x00, 0x00, 0x3C) => self.exp_on_pitch_bend_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x3D) => self.exp_on_pitch_bend_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x3E) => self.exp_on_pitch_bend_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x3F) if b <= 127 => self.exp_on_mod_min = Some(b),
            (0x00, 0x00, 0x40) if b <= 127 => self.exp_on_mod_max = Some(b),
            (0x00, 0x00, 0x41) => self.exp_on_mod_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x42) => self.exp_on_mod_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x43) => self.exp_on_cross_fader_pcm_1 = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x44) => self.exp_on_cross_fader_pcm_2 = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x45) => self.exp_on_cross_fader_modeling = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x46) => self.exp_on_cross_fader_normal_pu = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x47) if b <= 120 => self.exp_on_delay_min = Some(b),
            (0x00, 0x00, 0x48) if b <= 120 => self.exp_on_delay_max = Some(b),
            (0x00, 0x00, 0x49) if b <= 100 => self.exp_on_reverb_min = Some(b),
            (0x00, 0x00, 0x4A) if b <= 100 => self.exp_on_reverb_max = Some(b),
            (0x00, 0x00, 0x4B) if b <= 100 => self.exp_on_chorus_min = Some(b),
            (0x00, 0x00, 0x4C) if b <= 100 => self.exp_on_chorus_max = Some(b),
            (0x00, 0x00, 0x4D) => self.exp_sw_status = OnOff::from_byte(b),
            (0x00, 0x00, 0x4E) => self.exp_sw_function = ExpSwFunction::from_byte(b),
            (0x00, 0x00, 0x53) => self.exp_sw_tone_sw_off_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x54) => self.exp_sw_tone_sw_off_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x55) => self.exp_sw_tone_sw_off_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x56) => self.exp_sw_tone_sw_off_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x57) => self.exp_sw_tone_sw_on_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x58) => self.exp_sw_tone_sw_on_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x59) => self.exp_sw_tone_sw_on_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x5A) => self.exp_sw_tone_sw_on_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x5B) => self.gk_vol_function = ExpFunction::from_byte(b),
            (0x00, 0x00, 0x5C) => self.gk_vol_tone_vol_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x5D) => self.gk_vol_tone_vol_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x5E) => self.gk_vol_tone_vol_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x5F) => self.gk_vol_tone_vol_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x60) => self.gk_vol_pitch_bend_depth = PitchBendDepth::from_byte(b),
            (0x00, 0x00, 0x61) => self.gk_vol_pitch_bend_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x62) => self.gk_vol_pitch_bend_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x63) => self.gk_vol_pitch_bend_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x64) if b <= 127 => self.gk_vol_mod_min = Some(b),
            (0x00, 0x00, 0x65) if b <= 127 => self.gk_vol_mod_max = Some(b),
            (0x00, 0x00, 0x66) => self.gk_vol_mod_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x67) => self.gk_vol_mod_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x68) => self.gk_vol_cross_fader_pcm_1 = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x69) => self.gk_vol_cross_fader_pcm_2 = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x6A) => self.gk_vol_cross_fader_modeling = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x6B) => self.gk_vol_cross_fader_normal_pu = CrossFaderMode::from_byte(b),
            (0x00, 0x00, 0x6C) if b <= 120 => self.gk_vol_delay_min = Some(b),
            (0x00, 0x00, 0x6D) if b <= 120 => self.gk_vol_delay_max = Some(b),
            (0x00, 0x00, 0x6E) if b <= 100 => self.gk_vol_reverb_min = Some(b),
            (0x00, 0x00, 0x6F) if b <= 100 => self.gk_vol_reverb_max = Some(b),
            (0x00, 0x00, 0x70) if b <= 100 => self.gk_vol_chorus_min = Some(b),
            (0x00, 0x00, 0x71) if b <= 100 => self.gk_vol_chorus_max = Some(b),
            (0x00, 0x00, 0x72) => self.gk_s1_function = ExpSwFunction::from_byte(b),
            (0x00, 0x00, 0x77) => self.gk_s1_tone_sw_off_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x78) => self.gk_s1_tone_sw_off_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x79) => self.gk_s1_tone_sw_off_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x7A) => self.gk_s1_tone_sw_off_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x7B) => self.gk_s1_tone_sw_on_pcm_1 = OnOff::from_byte(b),
            (0x00, 0x00, 0x7C) => self.gk_s1_tone_sw_on_pcm_2 = OnOff::from_byte(b),
            (0x00, 0x00, 0x7D) => self.gk_s1_tone_sw_on_modeling = OnOff::from_byte(b),
            (0x00, 0x00, 0x7E) => self.gk_s1_tone_sw_on_normal_pu = OnOff::from_byte(b),
            (0x00, 0x00, 0x7F) => self.gk_s2_function = ExpSwFunction::from_byte(b),
            (0x01, 0x00, 0x04) => self.gk_s2_tone_sw_off_pcm_1 = OnOff::from_byte(b),
            (0x01, 0x00, 0x05) => self.gk_s2_tone_sw_off_pcm_2 = OnOff::from_byte(b),
            (0x01, 0x00, 0x06) => self.gk_s2_tone_sw_off_modeling = OnOff::from_byte(b),
            (0x01, 0x00, 0x07) => self.gk_s2_tone_sw_off_normal_pu = OnOff::from_byte(b),
            (0x01, 0x00, 0x08) => self.gk_s2_tone_sw_on_pcm_1 = OnOff::from_byte(b),
            (0x01, 0x00, 0x09) => self.gk_s2_tone_sw_on_pcm_2 = OnOff::from_byte(b),
            (0x01, 0x00, 0x0A) => self.gk_s2_tone_sw_on_modeling = OnOff::from_byte(b),
            (0x01, 0x00, 0x0B) => self.gk_s2_tone_sw_on_normal_pu = OnOff::from_byte(b),
            _ => {
                self.unknown_bytes.insert(format_key(page, hi, lo), b);
            }
        }
    }

    fn collect_bytes(&self, base_msb: u8) -> Result<BTreeMap<[u8; 4], u8>, CodecError> {
        let mut bytes: BTreeMap<[u8; 4], u8> = BTreeMap::new();
        if let Some(mode) = self.mode {
            bytes.insert([base_msb, 0x00, 0x00, 0x00], mode.to_byte());
        }
        for (i, b) in self.name.0.iter().enumerate() {
            if *b != 0x20 {
                bytes.insert([base_msb, 0x00, 0x00, 0x01 + i as u8], *b);
            }
        }
        // CTL block
        if let Some(v) = self.ctl_status {
            bytes.insert([base_msb, 0x00, 0x00, 0x11], v.to_byte());
        }
        if let Some(v) = self.ctl_function {
            bytes.insert([base_msb, 0x00, 0x00, 0x12], v.to_byte());
        }
        if let Some(v) = self.ctl_hold_type {
            bytes.insert([base_msb, 0x00, 0x00, 0x13], v.to_byte());
        }
        if let Some(v) = self.ctl_switch_mode {
            bytes.insert([base_msb, 0x00, 0x00, 0x14], v.to_byte());
        }
        if let Some(v) = self.ctl_hold_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x15], v.to_byte());
        }
        if let Some(v) = self.ctl_hold_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x16], v.to_byte());
        }
        if let Some(v) = self.ctl_tone_sw_off_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x17], v.to_byte());
        }
        if let Some(v) = self.ctl_tone_sw_off_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x18], v.to_byte());
        }
        if let Some(v) = self.ctl_tone_sw_off_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x19], v.to_byte());
        }
        if let Some(v) = self.ctl_tone_sw_off_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x1A], v.to_byte());
        }
        if let Some(v) = self.ctl_tone_sw_on_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x1B], v.to_byte());
        }
        if let Some(v) = self.ctl_tone_sw_on_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x1C], v.to_byte());
        }
        if let Some(v) = self.ctl_tone_sw_on_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x1D], v.to_byte());
        }
        if let Some(v) = self.ctl_tone_sw_on_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x1E], v.to_byte());
        }
        // EXP block
        if let Some(v) = self.exp_function {
            bytes.insert([base_msb, 0x00, 0x00, 0x1F], v.to_byte());
        }
        if let Some(v) = self.exp_tone_vol_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x20], v.to_byte());
        }
        if let Some(v) = self.exp_tone_vol_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x21], v.to_byte());
        }
        if let Some(v) = self.exp_tone_vol_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x22], v.to_byte());
        }
        if let Some(v) = self.exp_tone_vol_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x23], v.to_byte());
        }
        if let Some(v) = self.exp_pitch_bend_depth {
            bytes.insert([base_msb, 0x00, 0x00, 0x24], v.to_byte());
        }
        if let Some(v) = self.exp_pitch_bend_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x25], v.to_byte());
        }
        if let Some(v) = self.exp_pitch_bend_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x26], v.to_byte());
        }
        if let Some(v) = self.exp_pitch_bend_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x27], v.to_byte());
        }
        if let Some(v) = self.exp_mod_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x28], v);
        }
        if let Some(v) = self.exp_mod_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x29], v);
        }
        if let Some(v) = self.exp_mod_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x2A], v.to_byte());
        }
        if let Some(v) = self.exp_mod_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x2B], v.to_byte());
        }
        if let Some(v) = self.exp_cross_fader_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x2C], v.to_byte());
        }
        if let Some(v) = self.exp_cross_fader_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x2D], v.to_byte());
        }
        if let Some(v) = self.exp_cross_fader_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x2E], v.to_byte());
        }
        if let Some(v) = self.exp_cross_fader_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x2F], v.to_byte());
        }
        if let Some(v) = self.exp_delay_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x30], v);
        }
        if let Some(v) = self.exp_delay_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x31], v);
        }
        if let Some(v) = self.exp_reverb_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x32], v);
        }
        if let Some(v) = self.exp_reverb_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x33], v);
        }
        if let Some(v) = self.exp_chorus_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x34], v);
        }
        if let Some(v) = self.exp_chorus_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x35], v);
        }
        // EXP ON block
        if let Some(v) = self.exp_on_function {
            bytes.insert([base_msb, 0x00, 0x00, 0x36], v.to_byte());
        }
        if let Some(v) = self.exp_on_tone_vol_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x37], v.to_byte());
        }
        if let Some(v) = self.exp_on_tone_vol_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x38], v.to_byte());
        }
        if let Some(v) = self.exp_on_tone_vol_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x39], v.to_byte());
        }
        if let Some(v) = self.exp_on_tone_vol_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x3A], v.to_byte());
        }
        if let Some(v) = self.exp_on_pitch_bend_depth {
            bytes.insert([base_msb, 0x00, 0x00, 0x3B], v.to_byte());
        }
        if let Some(v) = self.exp_on_pitch_bend_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x3C], v.to_byte());
        }
        if let Some(v) = self.exp_on_pitch_bend_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x3D], v.to_byte());
        }
        if let Some(v) = self.exp_on_pitch_bend_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x3E], v.to_byte());
        }
        if let Some(v) = self.exp_on_mod_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x3F], v);
        }
        if let Some(v) = self.exp_on_mod_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x40], v);
        }
        if let Some(v) = self.exp_on_mod_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x41], v.to_byte());
        }
        if let Some(v) = self.exp_on_mod_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x42], v.to_byte());
        }
        if let Some(v) = self.exp_on_cross_fader_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x43], v.to_byte());
        }
        if let Some(v) = self.exp_on_cross_fader_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x44], v.to_byte());
        }
        if let Some(v) = self.exp_on_cross_fader_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x45], v.to_byte());
        }
        if let Some(v) = self.exp_on_cross_fader_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x46], v.to_byte());
        }
        if let Some(v) = self.exp_on_delay_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x47], v);
        }
        if let Some(v) = self.exp_on_delay_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x48], v);
        }
        if let Some(v) = self.exp_on_reverb_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x49], v);
        }
        if let Some(v) = self.exp_on_reverb_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x4A], v);
        }
        if let Some(v) = self.exp_on_chorus_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x4B], v);
        }
        if let Some(v) = self.exp_on_chorus_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x4C], v);
        }
        // EXP SW block
        if let Some(v) = self.exp_sw_status {
            bytes.insert([base_msb, 0x00, 0x00, 0x4D], v.to_byte());
        }
        if let Some(v) = self.exp_sw_function {
            bytes.insert([base_msb, 0x00, 0x00, 0x4E], v.to_byte());
        }
        if let Some(v) = self.exp_sw_tone_sw_off_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x53], v.to_byte());
        }
        if let Some(v) = self.exp_sw_tone_sw_off_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x54], v.to_byte());
        }
        if let Some(v) = self.exp_sw_tone_sw_off_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x55], v.to_byte());
        }
        if let Some(v) = self.exp_sw_tone_sw_off_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x56], v.to_byte());
        }
        if let Some(v) = self.exp_sw_tone_sw_on_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x57], v.to_byte());
        }
        if let Some(v) = self.exp_sw_tone_sw_on_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x58], v.to_byte());
        }
        if let Some(v) = self.exp_sw_tone_sw_on_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x59], v.to_byte());
        }
        if let Some(v) = self.exp_sw_tone_sw_on_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x5A], v.to_byte());
        }
        // GK VOLUME block
        if let Some(v) = self.gk_vol_function {
            bytes.insert([base_msb, 0x00, 0x00, 0x5B], v.to_byte());
        }
        if let Some(v) = self.gk_vol_tone_vol_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x5C], v.to_byte());
        }
        if let Some(v) = self.gk_vol_tone_vol_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x5D], v.to_byte());
        }
        if let Some(v) = self.gk_vol_tone_vol_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x5E], v.to_byte());
        }
        if let Some(v) = self.gk_vol_tone_vol_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x5F], v.to_byte());
        }
        if let Some(v) = self.gk_vol_pitch_bend_depth {
            bytes.insert([base_msb, 0x00, 0x00, 0x60], v.to_byte());
        }
        if let Some(v) = self.gk_vol_pitch_bend_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x61], v.to_byte());
        }
        if let Some(v) = self.gk_vol_pitch_bend_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x62], v.to_byte());
        }
        if let Some(v) = self.gk_vol_pitch_bend_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x63], v.to_byte());
        }
        if let Some(v) = self.gk_vol_mod_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x64], v);
        }
        if let Some(v) = self.gk_vol_mod_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x65], v);
        }
        if let Some(v) = self.gk_vol_mod_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x66], v.to_byte());
        }
        if let Some(v) = self.gk_vol_mod_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x67], v.to_byte());
        }
        if let Some(v) = self.gk_vol_cross_fader_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x68], v.to_byte());
        }
        if let Some(v) = self.gk_vol_cross_fader_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x69], v.to_byte());
        }
        if let Some(v) = self.gk_vol_cross_fader_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x6A], v.to_byte());
        }
        if let Some(v) = self.gk_vol_cross_fader_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x6B], v.to_byte());
        }
        if let Some(v) = self.gk_vol_delay_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x6C], v);
        }
        if let Some(v) = self.gk_vol_delay_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x6D], v);
        }
        if let Some(v) = self.gk_vol_reverb_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x6E], v);
        }
        if let Some(v) = self.gk_vol_reverb_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x6F], v);
        }
        if let Some(v) = self.gk_vol_chorus_min {
            bytes.insert([base_msb, 0x00, 0x00, 0x70], v);
        }
        if let Some(v) = self.gk_vol_chorus_max {
            bytes.insert([base_msb, 0x00, 0x00, 0x71], v);
        }
        // GK S1 block
        if let Some(v) = self.gk_s1_function {
            bytes.insert([base_msb, 0x00, 0x00, 0x72], v.to_byte());
        }
        if let Some(v) = self.gk_s1_tone_sw_off_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x77], v.to_byte());
        }
        if let Some(v) = self.gk_s1_tone_sw_off_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x78], v.to_byte());
        }
        if let Some(v) = self.gk_s1_tone_sw_off_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x79], v.to_byte());
        }
        if let Some(v) = self.gk_s1_tone_sw_off_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x7A], v.to_byte());
        }
        if let Some(v) = self.gk_s1_tone_sw_on_pcm_1 {
            bytes.insert([base_msb, 0x00, 0x00, 0x7B], v.to_byte());
        }
        if let Some(v) = self.gk_s1_tone_sw_on_pcm_2 {
            bytes.insert([base_msb, 0x00, 0x00, 0x7C], v.to_byte());
        }
        if let Some(v) = self.gk_s1_tone_sw_on_modeling {
            bytes.insert([base_msb, 0x00, 0x00, 0x7D], v.to_byte());
        }
        if let Some(v) = self.gk_s1_tone_sw_on_normal_pu {
            bytes.insert([base_msb, 0x00, 0x00, 0x7E], v.to_byte());
        }
        // GK S2 block (page 0x00 + page 0x01)
        if let Some(v) = self.gk_s2_function {
            bytes.insert([base_msb, 0x00, 0x00, 0x7F], v.to_byte());
        }
        if let Some(v) = self.gk_s2_tone_sw_off_pcm_1 {
            bytes.insert([base_msb, 0x01, 0x00, 0x04], v.to_byte());
        }
        if let Some(v) = self.gk_s2_tone_sw_off_pcm_2 {
            bytes.insert([base_msb, 0x01, 0x00, 0x05], v.to_byte());
        }
        if let Some(v) = self.gk_s2_tone_sw_off_modeling {
            bytes.insert([base_msb, 0x01, 0x00, 0x06], v.to_byte());
        }
        if let Some(v) = self.gk_s2_tone_sw_off_normal_pu {
            bytes.insert([base_msb, 0x01, 0x00, 0x07], v.to_byte());
        }
        if let Some(v) = self.gk_s2_tone_sw_on_pcm_1 {
            bytes.insert([base_msb, 0x01, 0x00, 0x08], v.to_byte());
        }
        if let Some(v) = self.gk_s2_tone_sw_on_pcm_2 {
            bytes.insert([base_msb, 0x01, 0x00, 0x09], v.to_byte());
        }
        if let Some(v) = self.gk_s2_tone_sw_on_modeling {
            bytes.insert([base_msb, 0x01, 0x00, 0x0A], v.to_byte());
        }
        if let Some(v) = self.gk_s2_tone_sw_on_normal_pu {
            bytes.insert([base_msb, 0x01, 0x00, 0x0B], v.to_byte());
        }
        for (k, b) in &self.unknown_bytes {
            let (page, hi, lo) =
                parse_key(k).ok_or_else(|| CodecError::BadStoredAddress(k.clone()))?;
            bytes.insert([base_msb, page, hi, lo], *b);
        }
        Ok(bytes)
    }
}

fn format_key(page: u8, hi: u8, lo: u8) -> String {
    format!("{:02X}:{:02X}:{:02X}", page, hi, lo)
}

fn parse_key(s: &str) -> Option<(u8, u8, u8)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let page = u8::from_str_radix(parts[0], 16).ok()?;
    let hi = u8::from_str_radix(parts[1], 16).ok()?;
    let lo = u8::from_str_radix(parts[2], 16).ok()?;
    Some((page, hi, lo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    const TEMP_MSB: u8 = 0x60;

    #[test]
    fn decodes_mode_and_name_from_temp_buffer() {
        // One DT1 spanning offsets 0x00..=0x10 of page 0x00 at MSB 0x60.
        let mut data = vec![PatchMode::Bass.to_byte()];
        data.extend(b"My Patch        "); // 16 chars
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x00, 0x00, 0x00],
            data: Cow::Owned(data),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.mode, Some(PatchMode::Bass));
        assert_eq!(area.name.as_string().trim_end(), "My Patch");
        assert!(area.unknown_bytes.is_empty());
    }

    #[test]
    fn unrecognised_offsets_round_trip_via_unknown_bytes() {
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x03, 0x00, 0x05], // MFX page, some offset
            data: Cow::Owned(vec![0xAB, 0xCD]),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.unknown_bytes.get("03:00:05"), Some(&0xAB));
        assert_eq!(area.unknown_bytes.get("03:00:06"), Some(&0xCD));

        let back = area.to_frames(0x10, TEMP_MSB).unwrap();
        let round = PatchArea::from_frames_at(&back, TEMP_MSB);
        assert_eq!(round, area);
    }

    #[test]
    fn ignores_frames_addressed_to_other_msbs() {
        let frames = vec![
            Frame::Dt1 {
                device_id: 0x10,
                address: [0x18, 0x00, 0x00, 0x00], // file-format MSB, not ours
                data: Cow::Owned(vec![PatchMode::Bass.to_byte()]),
            },
            Frame::Dt1 {
                device_id: 0x10,
                address: [TEMP_MSB, 0x00, 0x00, 0x00],
                data: Cow::Owned(vec![PatchMode::Guitar.to_byte()]),
            },
        ];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.mode, Some(PatchMode::Guitar));
    }

    #[test]
    fn ctl_block_decodes_and_round_trips() {
        // Build a payload covering 0x00..=0x1E in one DT1 frame.
        let mut data = vec![PatchMode::Guitar.to_byte()];
        data.extend(b"Crunch          "); // 16-char name at 0x01..=0x10
        data.push(OnOff::On.to_byte()); // 0x11 ctl_status
        data.push(CtlFunction::TapTempo.to_byte()); // 0x12
        data.push(HoldType::Type2.to_byte()); // 0x13
        data.push(SwitchMode::Moment.to_byte()); // 0x14
        data.push(OnOff::On.to_byte()); // 0x15 hold_pcm_1
        data.push(OnOff::Off.to_byte()); // 0x16 hold_pcm_2
        data.push(OnOff::On.to_byte()); // 0x17 off_pcm_1
        data.push(OnOff::Off.to_byte()); // 0x18 off_pcm_2
        data.push(OnOff::On.to_byte()); // 0x19 off_modeling
        data.push(OnOff::Off.to_byte()); // 0x1A off_normal_pu
        data.push(OnOff::Off.to_byte()); // 0x1B on_pcm_1
        data.push(OnOff::On.to_byte()); // 0x1C on_pcm_2
        data.push(OnOff::Off.to_byte()); // 0x1D on_modeling
        data.push(OnOff::On.to_byte()); // 0x1E on_normal_pu
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x00, 0x00, 0x00],
            data: Cow::Owned(data),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);

        assert_eq!(area.mode, Some(PatchMode::Guitar));
        assert_eq!(area.name.as_string().trim_end(), "Crunch");
        assert_eq!(area.ctl_status, Some(OnOff::On));
        assert_eq!(area.ctl_function, Some(CtlFunction::TapTempo));
        assert_eq!(area.ctl_hold_type, Some(HoldType::Type2));
        assert_eq!(area.ctl_switch_mode, Some(SwitchMode::Moment));
        assert_eq!(area.ctl_hold_pcm_1, Some(OnOff::On));
        assert_eq!(area.ctl_hold_pcm_2, Some(OnOff::Off));
        assert_eq!(area.ctl_tone_sw_off_pcm_1, Some(OnOff::On));
        assert_eq!(area.ctl_tone_sw_off_pcm_2, Some(OnOff::Off));
        assert_eq!(area.ctl_tone_sw_off_modeling, Some(OnOff::On));
        assert_eq!(area.ctl_tone_sw_off_normal_pu, Some(OnOff::Off));
        assert_eq!(area.ctl_tone_sw_on_pcm_1, Some(OnOff::Off));
        assert_eq!(area.ctl_tone_sw_on_pcm_2, Some(OnOff::On));
        assert_eq!(area.ctl_tone_sw_on_modeling, Some(OnOff::Off));
        assert_eq!(area.ctl_tone_sw_on_normal_pu, Some(OnOff::On));
        assert!(area.unknown_bytes.is_empty());

        // Round-trip
        let back_frames = area.to_frames(0x10, TEMP_MSB).unwrap();
        let round = PatchArea::from_frames_at(&back_frames, TEMP_MSB);
        assert_eq!(round, area);
    }

    #[test]
    fn exp_block_decodes_and_round_trips() {
        // 0x1F..=0x35 = 23 bytes.
        let payload: Vec<u8> = vec![
            ExpFunction::PitchBend.to_byte(), // 0x1F
            OnOff::On.to_byte(),              // 0x20 tone_vol_pcm_1
            OnOff::Off.to_byte(),             // 0x21 tone_vol_pcm_2
            OnOff::On.to_byte(),              // 0x22 tone_vol_modeling
            OnOff::Off.to_byte(),             // 0x23 tone_vol_normal_pu
            PitchBendDepth::new(-5).unwrap().to_byte(), // 0x24
            OnOff::On.to_byte(),              // 0x25 pitch_bend_pcm_1
            OnOff::Off.to_byte(),             // 0x26 pitch_bend_pcm_2
            OnOff::On.to_byte(),              // 0x27 pitch_bend_modeling
            0x40,                             // 0x28 mod_min
            0x60,                             // 0x29 mod_max
            OnOff::On.to_byte(),              // 0x2A mod_pcm_1
            OnOff::Off.to_byte(),             // 0x2B mod_pcm_2
            CrossFaderMode::Toe.to_byte(),    // 0x2C cf_pcm_1
            CrossFaderMode::Heel.to_byte(),   // 0x2D cf_pcm_2
            CrossFaderMode::Off.to_byte(),    // 0x2E cf_modeling
            CrossFaderMode::Heel.to_byte(),   // 0x2F cf_normal_pu
            0x10,                             // 0x30 delay_min
            0x70,                             // 0x31 delay_max
            0x20,                             // 0x32 reverb_min
            0x50,                             // 0x33 reverb_max
            0x15,                             // 0x34 chorus_min
            0x45,                             // 0x35 chorus_max
        ];
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x00, 0x00, 0x1F],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);

        assert_eq!(area.exp_function, Some(ExpFunction::PitchBend));
        assert_eq!(area.exp_tone_vol_pcm_1, Some(OnOff::On));
        assert_eq!(area.exp_tone_vol_normal_pu, Some(OnOff::Off));
        assert_eq!(area.exp_pitch_bend_depth.unwrap().get(), -5);
        assert_eq!(area.exp_pitch_bend_modeling, Some(OnOff::On));
        assert_eq!(area.exp_mod_min, Some(0x40));
        assert_eq!(area.exp_mod_max, Some(0x60));
        assert_eq!(area.exp_mod_pcm_2, Some(OnOff::Off));
        assert_eq!(area.exp_cross_fader_pcm_1, Some(CrossFaderMode::Toe));
        assert_eq!(area.exp_cross_fader_pcm_2, Some(CrossFaderMode::Heel));
        assert_eq!(area.exp_cross_fader_modeling, Some(CrossFaderMode::Off));
        assert_eq!(area.exp_cross_fader_normal_pu, Some(CrossFaderMode::Heel));
        assert_eq!(area.exp_delay_min, Some(0x10));
        assert_eq!(area.exp_delay_max, Some(0x70));
        assert_eq!(area.exp_reverb_max, Some(0x50));
        assert_eq!(area.exp_chorus_min, Some(0x15));
        assert_eq!(area.exp_chorus_max, Some(0x45));
        assert!(area.unknown_bytes.is_empty());

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn exp_on_block_decodes_and_round_trips() {
        // Distinct values from the EXP test so a swap between the two
        // blocks would show up as a mismatch.
        let payload: Vec<u8> = vec![
            ExpFunction::ModControl.to_byte(), // 0x36
            OnOff::Off.to_byte(),              // 0x37
            OnOff::On.to_byte(),               // 0x38
            OnOff::Off.to_byte(),              // 0x39
            OnOff::On.to_byte(),               // 0x3A
            PitchBendDepth::new(7).unwrap().to_byte(), // 0x3B
            OnOff::Off.to_byte(),              // 0x3C
            OnOff::On.to_byte(),               // 0x3D
            OnOff::Off.to_byte(),              // 0x3E
            0x05,                              // 0x3F mod_min
            0x65,                              // 0x40 mod_max
            OnOff::Off.to_byte(),              // 0x41
            OnOff::On.to_byte(),               // 0x42
            CrossFaderMode::Heel.to_byte(),    // 0x43
            CrossFaderMode::Toe.to_byte(),     // 0x44
            CrossFaderMode::Heel.to_byte(),    // 0x45
            CrossFaderMode::Off.to_byte(),     // 0x46
            0x05,                              // 0x47 delay_min
            0x55,                              // 0x48 delay_max
            0x15,                              // 0x49 reverb_min
            0x64,                              // 0x4A reverb_max (max raw = 100)
            0x05,                              // 0x4B chorus_min
            0x45,                              // 0x4C chorus_max
        ];
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x00, 0x00, 0x36],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.exp_on_function, Some(ExpFunction::ModControl));
        assert_eq!(area.exp_on_pitch_bend_depth.unwrap().get(), 7);
        assert_eq!(area.exp_on_mod_min, Some(0x05));
        assert_eq!(area.exp_on_mod_max, Some(0x65));
        assert_eq!(area.exp_on_cross_fader_pcm_1, Some(CrossFaderMode::Heel));
        assert_eq!(area.exp_on_cross_fader_normal_pu, Some(CrossFaderMode::Off));
        assert_eq!(area.exp_on_reverb_max, Some(0x64));
        assert_eq!(area.exp_on_chorus_max, Some(0x45));
        assert!(area.unknown_bytes.is_empty());

        // None of the EXP-OFF fields should have been touched.
        assert!(area.exp_function.is_none());
        assert!(area.exp_pitch_bend_depth.is_none());
        assert!(area.exp_chorus_max.is_none());

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn exp_sw_block_decodes_and_round_trips() {
        // Cover 0x4D..=0x5A. Offsets 0x4F..=0x52 are FloorBoard placeholders
        // — supply a byte at one of them and assert it falls through to
        // unknown_bytes rather than getting absorbed.
        let payload: Vec<u8> = vec![
            OnOff::On.to_byte(),                 // 0x4D status
            ExpSwFunction::DelaySw.to_byte(),    // 0x4E function
            0xAA,                                // 0x4F placeholder
            0xBB,                                // 0x50 placeholder
            0xCC,                                // 0x51 placeholder
            0xDD,                                // 0x52 placeholder
            OnOff::Off.to_byte(),                // 0x53 off_pcm_1
            OnOff::On.to_byte(),                 // 0x54 off_pcm_2
            OnOff::Off.to_byte(),                // 0x55 off_modeling
            OnOff::On.to_byte(),                 // 0x56 off_normal_pu
            OnOff::On.to_byte(),                 // 0x57 on_pcm_1
            OnOff::Off.to_byte(),                // 0x58 on_pcm_2
            OnOff::On.to_byte(),                 // 0x59 on_modeling
            OnOff::Off.to_byte(),                // 0x5A on_normal_pu
        ];
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x00, 0x00, 0x4D],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.exp_sw_status, Some(OnOff::On));
        assert_eq!(area.exp_sw_function, Some(ExpSwFunction::DelaySw));
        assert_eq!(area.exp_sw_tone_sw_off_pcm_2, Some(OnOff::On));
        assert_eq!(area.exp_sw_tone_sw_off_normal_pu, Some(OnOff::On));
        assert_eq!(area.exp_sw_tone_sw_on_pcm_1, Some(OnOff::On));
        assert_eq!(area.exp_sw_tone_sw_on_normal_pu, Some(OnOff::Off));
        // The 4 placeholder bytes survived round-trip via unknown_bytes.
        assert_eq!(area.unknown_bytes.get("00:00:4F"), Some(&0xAA));
        assert_eq!(area.unknown_bytes.get("00:00:50"), Some(&0xBB));
        assert_eq!(area.unknown_bytes.get("00:00:51"), Some(&0xCC));
        assert_eq!(area.unknown_bytes.get("00:00:52"), Some(&0xDD));

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn gk_volume_block_decodes_and_round_trips() {
        let payload: Vec<u8> = vec![
            ExpFunction::ToneVolume.to_byte(),         // 0x5B
            OnOff::On.to_byte(),                       // 0x5C
            OnOff::Off.to_byte(),                      // 0x5D
            OnOff::On.to_byte(),                       // 0x5E
            OnOff::Off.to_byte(),                      // 0x5F
            PitchBendDepth::new(-1).unwrap().to_byte(), // 0x60
            OnOff::On.to_byte(),                       // 0x61
            OnOff::Off.to_byte(),                      // 0x62
            OnOff::On.to_byte(),                       // 0x63
            0x30,                                      // 0x64 mod_min
            0x60,                                      // 0x65 mod_max
            OnOff::Off.to_byte(),                      // 0x66
            OnOff::On.to_byte(),                       // 0x67
            CrossFaderMode::Toe.to_byte(),             // 0x68
            CrossFaderMode::Heel.to_byte(),            // 0x69
            CrossFaderMode::Off.to_byte(),             // 0x6A
            CrossFaderMode::Toe.to_byte(),             // 0x6B
            0x08,                                      // 0x6C delay_min
            0x60,                                      // 0x6D delay_max
            0x18,                                      // 0x6E reverb_min
            0x58,                                      // 0x6F reverb_max
            0x08,                                      // 0x70 chorus_min
            0x48,                                      // 0x71 chorus_max
        ];
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x00, 0x00, 0x5B],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.gk_vol_function, Some(ExpFunction::ToneVolume));
        assert_eq!(area.gk_vol_pitch_bend_depth.unwrap().get(), -1);
        assert_eq!(area.gk_vol_mod_min, Some(0x30));
        assert_eq!(area.gk_vol_mod_max, Some(0x60));
        assert_eq!(area.gk_vol_cross_fader_pcm_1, Some(CrossFaderMode::Toe));
        assert_eq!(area.gk_vol_cross_fader_modeling, Some(CrossFaderMode::Off));
        assert_eq!(area.gk_vol_delay_max, Some(0x60));
        assert_eq!(area.gk_vol_chorus_max, Some(0x48));
        assert!(area.unknown_bytes.is_empty());

        // Distinct from EXP-OFF / EXP-ON / EXP-SW blocks.
        assert!(area.exp_function.is_none());
        assert!(area.exp_on_function.is_none());
        assert!(area.exp_sw_function.is_none());

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn gk_s1_block_decodes_and_round_trips() {
        let payload: Vec<u8> = vec![
            ExpSwFunction::ToneSw.to_byte(), // 0x72
            0x11,                            // 0x73 placeholder
            0x22,                            // 0x74 placeholder
            0x33,                            // 0x75 placeholder
            0x44,                            // 0x76 placeholder
            OnOff::On.to_byte(),             // 0x77 off_pcm_1
            OnOff::Off.to_byte(),            // 0x78
            OnOff::On.to_byte(),             // 0x79
            OnOff::Off.to_byte(),            // 0x7A
            OnOff::Off.to_byte(),            // 0x7B on_pcm_1
            OnOff::On.to_byte(),             // 0x7C
            OnOff::Off.to_byte(),            // 0x7D
            OnOff::On.to_byte(),             // 0x7E
        ];
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x00, 0x00, 0x72],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.gk_s1_function, Some(ExpSwFunction::ToneSw));
        assert_eq!(area.gk_s1_tone_sw_off_pcm_1, Some(OnOff::On));
        assert_eq!(area.gk_s1_tone_sw_off_normal_pu, Some(OnOff::Off));
        assert_eq!(area.gk_s1_tone_sw_on_pcm_1, Some(OnOff::Off));
        assert_eq!(area.gk_s1_tone_sw_on_normal_pu, Some(OnOff::On));
        // Placeholders preserved.
        assert_eq!(area.unknown_bytes.get("00:00:73"), Some(&0x11));
        assert_eq!(area.unknown_bytes.get("00:00:76"), Some(&0x44));

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn gk_s2_block_spans_page_boundary() {
        // GK S2 lives at page 0x00 offset 0x7F and continues into page 0x01.
        // Send it as TWO separate DT1 frames matching the natural Roland
        // wire convention, then verify decode + page-respecting round-trip.
        let frames = vec![
            // Function byte alone at the very end of page 0x00.
            Frame::Dt1 {
                device_id: 0x10,
                address: [TEMP_MSB, 0x00, 0x00, 0x7F],
                data: Cow::Owned(vec![ExpSwFunction::AmpSw.to_byte()]),
            },
            // 4 placeholder bytes + 8 tone-sw bytes at page 0x01.
            Frame::Dt1 {
                device_id: 0x10,
                address: [TEMP_MSB, 0x01, 0x00, 0x00],
                data: Cow::Owned(vec![
                    0xAA,
                    0xBB,
                    0xCC,
                    0xDD,
                    OnOff::On.to_byte(),
                    OnOff::Off.to_byte(),
                    OnOff::On.to_byte(),
                    OnOff::Off.to_byte(),
                    OnOff::Off.to_byte(),
                    OnOff::On.to_byte(),
                    OnOff::Off.to_byte(),
                    OnOff::On.to_byte(),
                ]),
            },
        ];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.gk_s2_function, Some(ExpSwFunction::AmpSw));
        assert_eq!(area.gk_s2_tone_sw_off_pcm_1, Some(OnOff::On));
        assert_eq!(area.gk_s2_tone_sw_off_normal_pu, Some(OnOff::Off));
        assert_eq!(area.gk_s2_tone_sw_on_pcm_1, Some(OnOff::Off));
        assert_eq!(area.gk_s2_tone_sw_on_normal_pu, Some(OnOff::On));
        // Placeholder bytes at page 0x01 offsets 0x00..=0x03 land in unknown_bytes.
        assert_eq!(area.unknown_bytes.get("01:00:00"), Some(&0xAA));
        assert_eq!(area.unknown_bytes.get("01:00:03"), Some(&0xDD));

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn payload_carries_lo_at_7f_to_next_hi() {
        // A 3-byte payload starting at lo=0x7E should land at lo=0x7E,
        // lo=0x7F, then (hi=0x01, lo=0x00) — NOT (hi=0x00, lo=0x80).
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x04, 0x00, 0x7E], // page 0x04 (MFX 2) so no typed match
            data: Cow::Owned(vec![0xA1, 0xA2, 0xA3]),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.unknown_bytes.get("04:00:7E"), Some(&0xA1));
        assert_eq!(area.unknown_bytes.get("04:00:7F"), Some(&0xA2));
        assert_eq!(area.unknown_bytes.get("04:01:00"), Some(&0xA3));
        // The wrong (overflow-to-0x80) behaviour would surface here:
        assert!(!area.unknown_bytes.contains_key("04:00:80"));
    }

    #[test]
    fn exp_sw_function_byte_symmetry() {
        for raw in 0x00_u8..=0x0D {
            let v = ExpSwFunction::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "mismatch for 0x{raw:02X}");
        }
        assert!(ExpSwFunction::from_byte(0x0E).is_none());
    }

    #[test]
    fn exp_function_byte_symmetry() {
        for raw in 0x00_u8..=0x09 {
            let v = ExpFunction::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "mismatch for 0x{raw:02X}");
        }
        assert!(ExpFunction::from_byte(0x0A).is_none());
    }

    #[test]
    fn ctl_function_byte_symmetry() {
        for raw in 0x00_u8..=0x10 {
            let v = CtlFunction::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "to_byte mismatch for 0x{raw:02X}");
        }
        assert!(CtlFunction::from_byte(0x11).is_none());
    }

    #[test]
    fn name_too_long_rejected() {
        use std::str::FromStr;
        let err = PatchName::from_str("0123456789ABCDEFG").unwrap_err();
        assert!(matches!(err, PatchNameError::TooLong(17)));
    }
}
