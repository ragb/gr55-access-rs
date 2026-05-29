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
use crate::system::{
    decode_nibble_pair, encode_nibble_pair, HoldType, OnOff, PatchLevel, PitchBendDepth, SwitchMode,
};

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

/// Master Assign Source (one of 9 hardware controllers or one of 63 MIDI CCs).
///
/// FloorBoard `midi.xml` lays this out at byte values `0x00..=0x47` (72
/// codepoints) where bytes `0x09..=0x27` map to CC#01..=CC#31 and
/// `0x28..=0x47` map to CC#64..=CC#95. The wrapped `MidiCc(u8)` carries the
/// CC number directly; `from_byte` enforces the 1..=31 or 64..=95
/// constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "cc")]
pub enum AssignSource {
    CtrlPdl,
    ExpPdl,
    ExpPdlOn,
    ExpPdlSw,
    IntPedal,
    WavePedal,
    GkS1,
    GkS2,
    GkVol,
    MidiCc(u8),
}

impl AssignSource {
    pub fn from_byte(b: u8) -> Option<Self> {
        use AssignSource::*;
        Some(match b {
            0x00 => CtrlPdl,
            0x01 => ExpPdl,
            0x02 => ExpPdlOn,
            0x03 => ExpPdlSw,
            0x04 => IntPedal,
            0x05 => WavePedal,
            0x06 => GkS1,
            0x07 => GkS2,
            0x08 => GkVol,
            0x09..=0x27 => MidiCc(b - 0x08),         // CC#01..=CC#31
            0x28..=0x47 => MidiCc(b - 0x28 + 0x40),  // CC#64..=CC#95
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use AssignSource::*;
        match self {
            CtrlPdl => 0x00,
            ExpPdl => 0x01,
            ExpPdlOn => 0x02,
            ExpPdlSw => 0x03,
            IntPedal => 0x04,
            WavePedal => 0x05,
            GkS1 => 0x06,
            GkS2 => 0x07,
            GkVol => 0x08,
            MidiCc(cc) if (1..=31).contains(&cc) => 0x08 + cc,
            MidiCc(cc) if (64..=95).contains(&cc) => 0x28 + (cc - 0x40),
            // Caller guaranteed a valid CC; if not, fall back to 0 (Off-ish).
            // The new() validator below is the recommended construction path.
            MidiCc(_) => 0x00,
        }
    }
    /// Validated constructor for `MidiCc` — accepts CC numbers 1..=31 or 64..=95.
    pub fn midi_cc(cc: u8) -> Option<Self> {
        if (1..=31).contains(&cc) || (64..=95).contains(&cc) {
            Some(AssignSource::MidiCc(cc))
        } else {
            None
        }
    }
}

/// Assign Source Mode — how the source maps to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignSourceMode {
    Moment,
    Toggle,
}

impl AssignSourceMode {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Moment),
            0x01 => Some(Self::Toggle),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Moment => 0x00,
            Self::Toggle => 0x01,
        }
    }
}

/// Internal pedal trigger source for the Assign's Int Pedal.
///
/// 11 logical variants. **Note:** FloorBoard `midi.xml` ships a typo
/// here — both `EXP PDL SW` and `GK S1` are listed as `value="08"`, with
/// `value="09"` missing. The intended mapping is sequential: GK S1 = 0x09,
/// GK S2 = 0x0A. We encode that intended layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignInternalTrigger {
    PatchChange,
    CtrlPdl,
    Exp1PdlLow,
    Exp1PdlMid,
    Exp1PdlHigh,
    Exp1PdlOnLow,
    Exp1PdlOnMid,
    Exp1PdlOnHigh,
    ExpPdlSw,
    GkS1,
    GkS2,
}

impl AssignInternalTrigger {
    pub fn from_byte(b: u8) -> Option<Self> {
        use AssignInternalTrigger::*;
        Some(match b {
            0x00 => PatchChange,
            0x01 => CtrlPdl,
            0x02 => Exp1PdlLow,
            0x03 => Exp1PdlMid,
            0x04 => Exp1PdlHigh,
            0x05 => Exp1PdlOnLow,
            0x06 => Exp1PdlOnMid,
            0x07 => Exp1PdlOnHigh,
            0x08 => ExpPdlSw,
            0x09 => GkS1,
            0x0A => GkS2,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use AssignInternalTrigger::*;
        match self {
            PatchChange => 0x00,
            CtrlPdl => 0x01,
            Exp1PdlLow => 0x02,
            Exp1PdlMid => 0x03,
            Exp1PdlHigh => 0x04,
            Exp1PdlOnLow => 0x05,
            Exp1PdlOnMid => 0x06,
            Exp1PdlOnHigh => 0x07,
            ExpPdlSw => 0x08,
            GkS1 => 0x09,
            GkS2 => 0x0A,
        }
    }
}

/// Internal-pedal acceleration curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignIntPdlCurve {
    Linear,
    SlowRise,
    FastRise,
}

impl AssignIntPdlCurve {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Linear),
            0x01 => Some(Self::SlowRise),
            0x02 => Some(Self::FastRise),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Linear => 0x00,
            Self::SlowRise => 0x01,
            Self::FastRise => 0x02,
        }
    }
}

/// Wave-pedal LFO waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignWaveForm {
    Saw,
    Triangle,
    Sine,
}

impl AssignWaveForm {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Saw),
            0x01 => Some(Self::Triangle),
            0x02 => Some(Self::Sine),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Saw => 0x00,
            Self::Triangle => 0x01,
            Self::Sine => 0x02,
        }
    }
}

/// MFX effect type selector at page `0x03` (or `0x04` for MFX 2)
/// offset `0x05`. 20 variants. Mined from FloorBoard `midi.xml:40735-40755`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MfxType {
    Equalizer,
    SuperFilter,
    Phaser,
    StepPhaser,
    RingModulator,
    Tremolo,
    AutoPan,
    Slicer,
    VkRotary,
    HexaChorus,
    SpaceD,
    Flanger,
    StepFlanger,
    GuitarAmpSim,
    Compressor,
    Limiter,
    ThreeTapPanDelay,
    TimeCtrlDelay,
    LofiCompressor,
    PitchShifter,
}

impl MfxType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use MfxType::*;
        Some(match b {
            0x00 => Equalizer,
            0x01 => SuperFilter,
            0x02 => Phaser,
            0x03 => StepPhaser,
            0x04 => RingModulator,
            0x05 => Tremolo,
            0x06 => AutoPan,
            0x07 => Slicer,
            0x08 => VkRotary,
            0x09 => HexaChorus,
            0x0A => SpaceD,
            0x0B => Flanger,
            0x0C => StepFlanger,
            0x0D => GuitarAmpSim,
            0x0E => Compressor,
            0x0F => Limiter,
            0x10 => ThreeTapPanDelay,
            0x11 => TimeCtrlDelay,
            0x12 => LofiCompressor,
            0x13 => PitchShifter,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use MfxType::*;
        match self {
            Equalizer => 0x00,
            SuperFilter => 0x01,
            Phaser => 0x02,
            StepPhaser => 0x03,
            RingModulator => 0x04,
            Tremolo => 0x05,
            AutoPan => 0x06,
            Slicer => 0x07,
            VkRotary => 0x08,
            HexaChorus => 0x09,
            SpaceD => 0x0A,
            Flanger => 0x0B,
            StepFlanger => 0x0C,
            GuitarAmpSim => 0x0D,
            Compressor => 0x0E,
            Limiter => 0x0F,
            ThreeTapPanDelay => 0x10,
            TimeCtrlDelay => 0x11,
            LofiCompressor => 0x12,
            PitchShifter => 0x13,
        }
    }
}

/// MFX EQ "Low" band frequency (2 settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqLowFreq {
    Hz200,
    Hz400,
}

impl EqLowFreq {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Hz200),
            0x01 => Some(Self::Hz400),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Hz200 => 0x00,
            Self::Hz400 => 0x01,
        }
    }
}

/// MFX EQ "Mid" band frequency (17 settings, 200Hz..=8.0kHz).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqMidFreq {
    Hz200,
    Hz250,
    Hz315,
    Hz400,
    Hz500,
    Hz630,
    Hz800,
    Khz1_0,
    Khz1_25,
    Khz1_6,
    Khz2_0,
    Khz2_5,
    Khz3_15,
    Khz4_0,
    Khz5_0,
    Khz6_3,
    Khz8_0,
}

impl EqMidFreq {
    pub fn from_byte(b: u8) -> Option<Self> {
        use EqMidFreq::*;
        Some(match b {
            0x00 => Hz200,
            0x01 => Hz250,
            0x02 => Hz315,
            0x03 => Hz400,
            0x04 => Hz500,
            0x05 => Hz630,
            0x06 => Hz800,
            0x07 => Khz1_0,
            0x08 => Khz1_25,
            0x09 => Khz1_6,
            0x0A => Khz2_0,
            0x0B => Khz2_5,
            0x0C => Khz3_15,
            0x0D => Khz4_0,
            0x0E => Khz5_0,
            0x0F => Khz6_3,
            0x10 => Khz8_0,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use EqMidFreq::*;
        match self {
            Hz200 => 0x00,
            Hz250 => 0x01,
            Hz315 => 0x02,
            Hz400 => 0x03,
            Hz500 => 0x04,
            Hz630 => 0x05,
            Hz800 => 0x06,
            Khz1_0 => 0x07,
            Khz1_25 => 0x08,
            Khz1_6 => 0x09,
            Khz2_0 => 0x0A,
            Khz2_5 => 0x0B,
            Khz3_15 => 0x0C,
            Khz4_0 => 0x0D,
            Khz5_0 => 0x0E,
            Khz6_3 => 0x0F,
            Khz8_0 => 0x10,
        }
    }
}

/// MFX EQ "Mid" band Q factor (5 settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqMidQ {
    Q0_5,
    Q1,
    Q2,
    Q4,
    Q8,
}

impl EqMidQ {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Q0_5),
            0x01 => Some(Self::Q1),
            0x02 => Some(Self::Q2),
            0x03 => Some(Self::Q4),
            0x04 => Some(Self::Q8),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Q0_5 => 0x00,
            Self::Q1 => 0x01,
            Self::Q2 => 0x02,
            Self::Q4 => 0x03,
            Self::Q8 => 0x04,
        }
    }
}

/// MFX EQ "High" band frequency (3 settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqHighFreq {
    Khz2_0,
    Khz4_0,
    Khz8_0,
}

impl EqHighFreq {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Khz2_0),
            0x01 => Some(Self::Khz4_0),
            0x02 => Some(Self::Khz8_0),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Khz2_0 => 0x00,
            Self::Khz4_0 => 0x01,
            Self::Khz8_0 => 0x02,
        }
    }
}

/// The Patch's single MFX slot. The MFX parameter block is 256 bytes
/// laid across pages `0x03` (linear `0..=127`) and `0x04` (linear
/// `128..=255`). Despite FloorBoard `midi.xml` LSB-labelling page `0x04`
/// as "MFX 2", its content is the **continuation** of MFX 1's
/// type-specific region — there is only one MFX slot, not two.
///
/// The 6 common header bytes (sends, switch, type, pan, plus a
/// "reserved" byte) are typed individually. The 11-byte block at
/// linear `0x07..=0x11` carries the **Equalizer effect's** parameters
/// (they are present whenever the byte at `0x05` is `MfxType::Equalizer`).
/// The remaining bytes at linear `0x12..=0xFF` form a parallel pile of
/// per-effect-type parameter blocks, one disjoint range per effect type
/// per the build-time-validated [`crate::mfx_params`] table; this
/// commit keeps them in `raw_tail` keyed by linear offset for lossless
/// round-trip.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mfx {
    // Common header (0x00..=0x06).
    /// Chorus Send (raw 0..=100) at offset `0x00`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chorus_send: Option<u8>,
    /// Delay Send at `0x01`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_send: Option<u8>,
    /// Reverb Send at `0x02`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverb_send: Option<u8>,
    /// FloorBoard labels offset `0x03` as `name="reserved"` with no
    /// `customdesc` and full 0..=255 range. Round-tripped as raw u8.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserved: Option<u8>,
    /// MFX Switch at `0x04`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch: Option<OnOff>,
    /// MFX Type selector at `0x05`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfx_type: Option<MfxType>,
    /// MFX Pan at `0x06` — raw 0..=100 representing the device's L50..R50
    /// position scale (0 = L50, 50 = center, 100 = R50).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pan: Option<u8>,

    // EQ block (0x07..=0x11) — always present, independent of MFX type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_low_freq: Option<EqLowFreq>,
    /// Low Gain at `0x08` (wire `0x00..=0x1E` = -15..=+15 dB). Raw byte.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_low_gain: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_mid1_freq: Option<EqMidFreq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_mid1_gain: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_mid1_q: Option<EqMidQ>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_mid2_freq: Option<EqMidFreq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_mid2_gain: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_mid2_q: Option<EqMidQ>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_high_freq: Option<EqHighFreq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_high_gain: Option<u8>,
    /// EQ Level at `0x11` (raw 0..=127).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_level: Option<u8>,

    /// Every byte not covered by the typed fields above. Keys are
    /// **linear offsets** spanning both pages: `0x00..=0x7F` = page
    /// `0x03` offsets `0x00..=0x7F`, `0x80..=0xFF` = page `0x04`
    /// offsets `0x00..=0x7F`. Looking these up against
    /// [`crate::mfx_params::MFX_PARAMS`] gives the parameter name and
    /// owning effect type.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw_tail: BTreeMap<u16, u8>,
}

impl Mfx {
    /// Try to absorb a single byte at linear offset `linear` (0..=255).
    /// Returns false if the byte hit a typed slot but failed validation,
    /// so the caller can route it to `unknown_bytes` instead.
    fn store_byte(&mut self, linear: u16, b: u8) -> bool {
        match linear {
            0x00 if b <= 100 => {
                self.chorus_send = Some(b);
                true
            }
            0x01 if b <= 100 => {
                self.delay_send = Some(b);
                true
            }
            0x02 if b <= 100 => {
                self.reverb_send = Some(b);
                true
            }
            0x03 => {
                self.reserved = Some(b);
                true
            }
            0x04 => match OnOff::from_byte(b) {
                Some(v) => {
                    self.switch = Some(v);
                    true
                }
                None => false,
            },
            0x05 => match MfxType::from_byte(b) {
                Some(v) => {
                    self.mfx_type = Some(v);
                    true
                }
                None => false,
            },
            0x06 if b <= 100 => {
                self.pan = Some(b);
                true
            }
            0x07 => match EqLowFreq::from_byte(b) {
                Some(v) => {
                    self.eq_low_freq = Some(v);
                    true
                }
                None => false,
            },
            0x08 if b <= 0x1E => {
                self.eq_low_gain = Some(b);
                true
            }
            0x09 => match EqMidFreq::from_byte(b) {
                Some(v) => {
                    self.eq_mid1_freq = Some(v);
                    true
                }
                None => false,
            },
            0x0A if b <= 0x1E => {
                self.eq_mid1_gain = Some(b);
                true
            }
            0x0B => match EqMidQ::from_byte(b) {
                Some(v) => {
                    self.eq_mid1_q = Some(v);
                    true
                }
                None => false,
            },
            0x0C => match EqMidFreq::from_byte(b) {
                Some(v) => {
                    self.eq_mid2_freq = Some(v);
                    true
                }
                None => false,
            },
            0x0D if b <= 0x1E => {
                self.eq_mid2_gain = Some(b);
                true
            }
            0x0E => match EqMidQ::from_byte(b) {
                Some(v) => {
                    self.eq_mid2_q = Some(v);
                    true
                }
                None => false,
            },
            0x0F => match EqHighFreq::from_byte(b) {
                Some(v) => {
                    self.eq_high_freq = Some(v);
                    true
                }
                None => false,
            },
            0x10 if b <= 0x1E => {
                self.eq_high_gain = Some(b);
                true
            }
            0x11 if b <= 0x7F => {
                self.eq_level = Some(b);
                true
            }
            // Type-specific tail — see Mfx docstring + mfx_params table.
            0x12..=0xFF => {
                self.raw_tail.insert(linear, b);
                true
            }
            _ => false,
        }
    }

    fn emit_bytes(&self, bytes: &mut BTreeMap<[u8; 4], u8>, base_msb: u8) {
        macro_rules! put {
            ($linear:expr, $val:expr) => {
                if let Some(v) = $val {
                    let (page, off) = mfx_address_split($linear);
                    bytes.insert([base_msb, page, 0x00, off], v);
                }
            };
        }
        put!(0x00, self.chorus_send);
        put!(0x01, self.delay_send);
        put!(0x02, self.reverb_send);
        put!(0x03, self.reserved);
        put!(0x04, self.switch.map(OnOff::to_byte));
        put!(0x05, self.mfx_type.map(MfxType::to_byte));
        put!(0x06, self.pan);
        put!(0x07, self.eq_low_freq.map(EqLowFreq::to_byte));
        put!(0x08, self.eq_low_gain);
        put!(0x09, self.eq_mid1_freq.map(EqMidFreq::to_byte));
        put!(0x0A, self.eq_mid1_gain);
        put!(0x0B, self.eq_mid1_q.map(EqMidQ::to_byte));
        put!(0x0C, self.eq_mid2_freq.map(EqMidFreq::to_byte));
        put!(0x0D, self.eq_mid2_gain);
        put!(0x0E, self.eq_mid2_q.map(EqMidQ::to_byte));
        put!(0x0F, self.eq_high_freq.map(EqHighFreq::to_byte));
        put!(0x10, self.eq_high_gain);
        put!(0x11, self.eq_level);
        for (linear, b) in &self.raw_tail {
            let (page, off) = mfx_address_split(*linear);
            bytes.insert([base_msb, page, 0x00, off], *b);
        }
    }

    /// The MFX type currently selected. `None` if the type byte isn't set
    /// or holds an out-of-range value.
    pub fn active_type(&self) -> Option<MfxType> {
        self.mfx_type
    }

    /// Iterate only the bytes that this MFX holds for the currently
    /// **active** effect type (the one selected by `mfx_type` at offset
    /// `0x05`), plus the always-present common header. Returns nothing if
    /// `mfx_type` isn't set.
    ///
    /// This is the editor-friendly view: when the user has picked
    /// `Type = SuperFilter`, they want to see Chorus Send + Delay Send +
    /// Reverb Send + Switch + Type + Pan + the 13 Super-Filter
    /// parameters — not the inert bytes belonging to other 19 effect
    /// types that happen to round-trip through `raw_tail`.
    pub fn iter_active_type_params(
        &self,
    ) -> impl Iterator<Item = (u16, u8, &'static crate::mfx_params::MfxParamEntry)> + '_ {
        let active = self.mfx_type.map(map_mfx_type_to_owner);
        self.iter_params().filter(move |(_, _, entry)| match active {
            Some(ty) => entry.owning_type.is_none() || entry.owning_type == Some(ty),
            None => entry.owning_type.is_none(),
        })
    }

    /// Iterate only the common-header bytes (those with no `owning_type`
    /// — sends, switch, type, pan, plus FloorBoard's "reserved" byte).
    pub fn iter_common_params(
        &self,
    ) -> impl Iterator<Item = (u16, u8, &'static crate::mfx_params::MfxParamEntry)> + '_ {
        self.iter_params()
            .filter(|(_, _, entry)| entry.owning_type.is_none())
    }

    /// Iterate only the bytes owned by a specific effect type. Useful for
    /// "what's stored under Phaser even though the active type is
    /// Equalizer?" — the GR-55 preserves all 20 types' parameters on
    /// disk; switching types just changes which range the device uses.
    pub fn iter_type_params(
        &self,
        owner: crate::mfx_params::MfxTypeOwner,
    ) -> impl Iterator<Item = (u16, u8, &'static crate::mfx_params::MfxParamEntry)> + '_ {
        self.iter_params()
            .filter(move |(_, _, entry)| entry.owning_type == Some(owner))
    }

    /// Iterate every (linear, byte, param_entry) triple this MFX holds —
    /// typed fields plus raw_tail bytes — paired with their FloorBoard
    /// metadata. Useful for editors that want to render every parameter
    /// with its name and owning type.
    pub fn iter_params(&self) -> impl Iterator<Item = (u16, u8, &'static crate::mfx_params::MfxParamEntry)> + '_ {
        // Build a quick lookup of which linear offsets the typed fields
        // occupy so we know to fall back to raw_tail for the rest.
        let typed = [
            (0x00, self.chorus_send),
            (0x01, self.delay_send),
            (0x02, self.reverb_send),
            (0x03, self.reserved),
            (0x04, self.switch.map(OnOff::to_byte)),
            (0x05, self.mfx_type.map(MfxType::to_byte)),
            (0x06, self.pan),
            (0x07, self.eq_low_freq.map(EqLowFreq::to_byte)),
            (0x08, self.eq_low_gain),
            (0x09, self.eq_mid1_freq.map(EqMidFreq::to_byte)),
            (0x0A, self.eq_mid1_gain),
            (0x0B, self.eq_mid1_q.map(EqMidQ::to_byte)),
            (0x0C, self.eq_mid2_freq.map(EqMidFreq::to_byte)),
            (0x0D, self.eq_mid2_gain),
            (0x0E, self.eq_mid2_q.map(EqMidQ::to_byte)),
            (0x0F, self.eq_high_freq.map(EqHighFreq::to_byte)),
            (0x10, self.eq_high_gain),
            (0x11, self.eq_level),
        ];
        typed
            .into_iter()
            .filter_map(|(lin, v)| v.map(|b| (lin as u16, b, &crate::mfx_params::MFX_PARAMS[lin as usize])))
            .chain(self.raw_tail.iter().map(|(&lin, &b)| {
                (lin, b, &crate::mfx_params::MFX_PARAMS[lin as usize])
            }))
    }
}

/// Bridge `patch::MfxType` (the on-wire effect type enum) to
/// `mfx_params::MfxTypeOwner` (the build-time table's enum). Same 20
/// variants in the same order; we keep them as distinct types so the
/// generated table can evolve independently of the typed Patch model.
pub(crate) fn map_mfx_type_to_owner(ty: MfxType) -> crate::mfx_params::MfxTypeOwner {
    use crate::mfx_params::MfxTypeOwner;
    match ty {
        MfxType::Equalizer => MfxTypeOwner::Equalizer,
        MfxType::SuperFilter => MfxTypeOwner::SuperFilter,
        MfxType::Phaser => MfxTypeOwner::Phaser,
        MfxType::StepPhaser => MfxTypeOwner::StepPhaser,
        MfxType::RingModulator => MfxTypeOwner::RingModulator,
        MfxType::Tremolo => MfxTypeOwner::Tremolo,
        MfxType::AutoPan => MfxTypeOwner::AutoPan,
        MfxType::Slicer => MfxTypeOwner::Slicer,
        MfxType::VkRotary => MfxTypeOwner::VkRotary,
        MfxType::HexaChorus => MfxTypeOwner::HexaChorus,
        MfxType::SpaceD => MfxTypeOwner::SpaceD,
        MfxType::Flanger => MfxTypeOwner::Flanger,
        MfxType::StepFlanger => MfxTypeOwner::StepFlanger,
        MfxType::GuitarAmpSim => MfxTypeOwner::GuitarAmpSim,
        MfxType::Compressor => MfxTypeOwner::Compressor,
        MfxType::Limiter => MfxTypeOwner::Limiter,
        MfxType::ThreeTapPanDelay => MfxTypeOwner::ThreeTapPanDelay,
        MfxType::TimeCtrlDelay => MfxTypeOwner::TimeCtrlDelay,
        MfxType::LofiCompressor => MfxTypeOwner::LofiCompressor,
        MfxType::PitchShifter => MfxTypeOwner::PitchShifter,
    }
}

/// Split a linear MFX offset (0..=255) into the wire (page, offset)
/// pair. Page `0x03` covers linear `0..=127`; page `0x04` covers
/// `128..=255`.
fn mfx_address_split(linear: u16) -> (u8, u8) {
    if linear < 128 {
        (0x03, linear as u8)
    } else {
        (0x04, (linear - 128) as u8)
    }
}

/// The Patch's single MOD slot. Lives entirely on page `0x07` at offsets
/// `0x11..=0x7F`. The 7 common header bytes (sends, switch, type, pan,
/// and an undocumented `null_14` byte) are typed; the type-specific
/// tail at `0x18..=0x59` is parked in `raw_tail` keyed by page offset.
/// Looking offsets up against [`crate::mod_params::MOD_PARAMS`] gives
/// the parameter name and owning effect type — all 14 MOD types have
/// disjoint byte ranges per the build-time verification.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mod {
    /// Amp/Mod Chorus Send at `0x07:00:11` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chorus_send: Option<u8>,
    /// Amp/Mod Delay Send at `0x07:00:12`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_send: Option<u8>,
    /// Amp/Mod Reverb Send at `0x07:00:13`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverb_send: Option<u8>,
    /// FloorBoard labels `0x07:00:14` as `customdesc="null"` with full
    /// 0..=255 range. Round-tripped as raw u8.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub null_14: Option<u8>,
    /// MOD switch at `0x07:00:15`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch: Option<OnOff>,
    /// MOD effect type at `0x07:00:16` (14 variants).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_type: Option<ModType>,
    /// MOD Pan at `0x07:00:17` (raw 0..=100, L50..R50).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pan: Option<u8>,
    /// Type-specific tail at `0x07:00:18..=59`, keyed by page offset.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw_tail: BTreeMap<u8, u8>,
}

impl Mod {
    /// Try to absorb a single byte at page-0x07 offset `off`. Returns
    /// false if the byte hit a typed slot but failed validation.
    fn store_byte(&mut self, off: u8, b: u8) -> bool {
        match off {
            0x11 if b <= 100 => {
                self.chorus_send = Some(b);
                true
            }
            0x12 if b <= 100 => {
                self.delay_send = Some(b);
                true
            }
            0x13 if b <= 100 => {
                self.reverb_send = Some(b);
                true
            }
            0x14 => {
                self.null_14 = Some(b);
                true
            }
            0x15 => match OnOff::from_byte(b) {
                Some(v) => {
                    self.switch = Some(v);
                    true
                }
                None => false,
            },
            0x16 => match ModType::from_byte(b) {
                Some(v) => {
                    self.mod_type = Some(v);
                    true
                }
                None => false,
            },
            0x17 if b <= 100 => {
                self.pan = Some(b);
                true
            }
            // Type-specific tail. Per crate::mod_params, MOD's 14 effect
            // types own disjoint byte ranges in 0x18..=0x59.
            0x18..=0x59 => {
                self.raw_tail.insert(off, b);
                true
            }
            _ => false,
        }
    }

    fn emit_bytes(&self, bytes: &mut BTreeMap<[u8; 4], u8>, base_msb: u8) {
        macro_rules! put {
            ($off:expr, $val:expr) => {
                if let Some(v) = $val {
                    bytes.insert([base_msb, 0x07, 0x00, $off], v);
                }
            };
        }
        put!(0x11, self.chorus_send);
        put!(0x12, self.delay_send);
        put!(0x13, self.reverb_send);
        put!(0x14, self.null_14);
        put!(0x15, self.switch.map(OnOff::to_byte));
        put!(0x16, self.mod_type.map(ModType::to_byte));
        put!(0x17, self.pan);
        for (off, b) in &self.raw_tail {
            bytes.insert([base_msb, 0x07, 0x00, *off], *b);
        }
    }

    /// Convenience accessor for the active MOD effect type.
    pub fn active_type(&self) -> Option<ModType> {
        self.mod_type
    }

    /// Iterate every (offset, byte, ModParamEntry) the slot holds — typed
    /// fields plus raw_tail bytes — paired with their FloorBoard metadata.
    pub fn iter_params(
        &self,
    ) -> impl Iterator<Item = (u8, u8, &'static crate::mod_params::ParamEntry)> + '_ {
        let typed = [
            (0x11_u8, self.chorus_send),
            (0x12, self.delay_send),
            (0x13, self.reverb_send),
            (0x14, self.null_14),
            (0x15, self.switch.map(OnOff::to_byte)),
            (0x16, self.mod_type.map(ModType::to_byte)),
            (0x17, self.pan),
        ];
        typed
            .into_iter()
            .filter_map(|(off, v)| v.map(|b| (off, b, &crate::mod_params::MOD_PARAMS[off as usize])))
            .chain(
                self.raw_tail
                    .iter()
                    .map(|(&off, &b)| (off, b, &crate::mod_params::MOD_PARAMS[off as usize])),
            )
    }
}

/// The Patch's single Modeling slot. Holds the 30-byte common header
/// at page `0x10` offsets `0x00..=0x1D` and a 226-byte type-specific
/// tail spanning page `0x10` `0x1E..=0x7F` + all of page `0x11`. The
/// tail is parked in `raw_tail` keyed by linear offset (0..=255, page
/// `0x10` = 0..=127, page `0x11` = 128..=255) for lossless round-trip.
/// Looking offsets up against [`crate::modeling_params::MODELING_PARAMS`]
/// returns the mode/category/type-set/name 4-tuple.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modeling {
    /// Guitar Mode category at `0x10:00:00`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gm_category: Option<GuitarModeCategory>,
    /// Guitar Mode E.Guitar type at `0x10:00:01`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gm_egtr_type: Option<GmEGuitarType>,
    /// Guitar Mode Acoustic type at `0x10:00:02`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gm_acoustic_type: Option<GmAcousticType>,
    /// Guitar Mode E.Bass type at `0x10:00:03`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gm_ebass_type: Option<GmEBassType>,
    /// Guitar Mode Synth type at `0x10:00:04`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gm_synth_type: Option<ModelingSynthType>,
    /// Bass Mode category at `0x10:00:05`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bm_category: Option<BassModeCategory>,
    /// Bass Mode E.Bass type at `0x10:00:06`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bm_ebass_type: Option<BmEBassType>,
    /// Bass Mode E.Guitar type at `0x10:00:07`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bm_egtr_type: Option<BmEGuitarType>,
    /// Bass Mode Synth type at `0x10:00:08`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bm_synth_type: Option<ModelingSynthType>,
    /// Tone level at `0x10:00:09` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone_level: Option<u8>,
    /// Tone switch at `0x10:00:0A` — wire-reversed (`On = 0x00`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone_sw: Option<AnalogPuToneSw>,
    /// Per-string modeling level at `0x10:00:0B..=0x10` (strings 1..=6).
    #[serde(default, skip_serializing_if = "string_shift_all_none")]
    pub string_level: [Option<u8>; 6],
    /// Per-string pitch step at `0x10:00:11/13/15/17/19/1B` — raw
    /// `0..=0x30` = -24..=+24 semitones.
    #[serde(default, skip_serializing_if = "string_shift_all_none")]
    pub pitch_step: [Option<u8>; 6],
    /// Per-string pitch fine at `0x10:00:12/14/16/18/1A/1C` — raw
    /// `0..=0x64` = -50..=+50 cents.
    #[serde(default, skip_serializing_if = "string_shift_all_none")]
    pub pitch_fine: [Option<u8>; 6],
    /// 12-string switch at `0x10:00:1D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twelve_string: Option<OnOff>,
    /// Type-specific tail keyed by linear offset (0..=255, page 0x10 =
    /// 0..=127, page 0x11 = 128..=255). Lookups against
    /// [`crate::modeling_params::MODELING_PARAMS`] give the
    /// mode/category/type-set/name 4-tuple.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw_tail: BTreeMap<u16, u8>,
}

impl Modeling {
    fn store_byte(&mut self, linear: u16, b: u8) -> bool {
        match linear {
            0x00 => match GuitarModeCategory::from_byte(b) {
                Some(v) => {
                    self.gm_category = Some(v);
                    true
                }
                None => false,
            },
            0x01 => match GmEGuitarType::from_byte(b) {
                Some(v) => {
                    self.gm_egtr_type = Some(v);
                    true
                }
                None => false,
            },
            0x02 => match GmAcousticType::from_byte(b) {
                Some(v) => {
                    self.gm_acoustic_type = Some(v);
                    true
                }
                None => false,
            },
            0x03 => match GmEBassType::from_byte(b) {
                Some(v) => {
                    self.gm_ebass_type = Some(v);
                    true
                }
                None => false,
            },
            0x04 => match ModelingSynthType::from_byte(b) {
                Some(v) => {
                    self.gm_synth_type = Some(v);
                    true
                }
                None => false,
            },
            0x05 => match BassModeCategory::from_byte(b) {
                Some(v) => {
                    self.bm_category = Some(v);
                    true
                }
                None => false,
            },
            0x06 => match BmEBassType::from_byte(b) {
                Some(v) => {
                    self.bm_ebass_type = Some(v);
                    true
                }
                None => false,
            },
            0x07 => match BmEGuitarType::from_byte(b) {
                Some(v) => {
                    self.bm_egtr_type = Some(v);
                    true
                }
                None => false,
            },
            0x08 => match ModelingSynthType::from_byte(b) {
                Some(v) => {
                    self.bm_synth_type = Some(v);
                    true
                }
                None => false,
            },
            0x09 if b <= 100 => {
                self.tone_level = Some(b);
                true
            }
            0x0A => match AnalogPuToneSw::from_byte(b) {
                Some(v) => {
                    self.tone_sw = Some(v);
                    true
                }
                None => false,
            },
            0x0B..=0x10 if b <= 100 => {
                self.string_level[(linear - 0x0B) as usize] = Some(b);
                true
            }
            // Per-string pitch step (even) + fine (odd) interleaved at
            // 0x11..=0x1C.
            0x11 if b <= 0x30 => {
                self.pitch_step[0] = Some(b);
                true
            }
            0x12 if b <= 0x64 => {
                self.pitch_fine[0] = Some(b);
                true
            }
            0x13 if b <= 0x30 => {
                self.pitch_step[1] = Some(b);
                true
            }
            0x14 if b <= 0x64 => {
                self.pitch_fine[1] = Some(b);
                true
            }
            0x15 if b <= 0x30 => {
                self.pitch_step[2] = Some(b);
                true
            }
            0x16 if b <= 0x64 => {
                self.pitch_fine[2] = Some(b);
                true
            }
            0x17 if b <= 0x30 => {
                self.pitch_step[3] = Some(b);
                true
            }
            0x18 if b <= 0x64 => {
                self.pitch_fine[3] = Some(b);
                true
            }
            0x19 if b <= 0x30 => {
                self.pitch_step[4] = Some(b);
                true
            }
            0x1A if b <= 0x64 => {
                self.pitch_fine[4] = Some(b);
                true
            }
            0x1B if b <= 0x30 => {
                self.pitch_step[5] = Some(b);
                true
            }
            0x1C if b <= 0x64 => {
                self.pitch_fine[5] = Some(b);
                true
            }
            0x1D => match OnOff::from_byte(b) {
                Some(v) => {
                    self.twelve_string = Some(v);
                    true
                }
                None => false,
            },
            // Type-specific tail spans page 0x10 0x1E..=0x7F + page 0x11
            // (linear 0x80..=0xFF).
            0x1E..=0xFF => {
                self.raw_tail.insert(linear, b);
                true
            }
            _ => false,
        }
    }

    fn emit_bytes(&self, bytes: &mut BTreeMap<[u8; 4], u8>, base_msb: u8) {
        macro_rules! put {
            ($linear:expr, $val:expr) => {
                if let Some(v) = $val {
                    let (page, off) = modeling_address_split($linear);
                    bytes.insert([base_msb, page, 0x00, off], v);
                }
            };
        }
        put!(0x00, self.gm_category.map(GuitarModeCategory::to_byte));
        put!(0x01, self.gm_egtr_type.map(GmEGuitarType::to_byte));
        put!(0x02, self.gm_acoustic_type.map(GmAcousticType::to_byte));
        put!(0x03, self.gm_ebass_type.map(GmEBassType::to_byte));
        put!(0x04, self.gm_synth_type.map(ModelingSynthType::to_byte));
        put!(0x05, self.bm_category.map(BassModeCategory::to_byte));
        put!(0x06, self.bm_ebass_type.map(BmEBassType::to_byte));
        put!(0x07, self.bm_egtr_type.map(BmEGuitarType::to_byte));
        put!(0x08, self.bm_synth_type.map(ModelingSynthType::to_byte));
        put!(0x09, self.tone_level);
        put!(0x0A, self.tone_sw.map(AnalogPuToneSw::to_byte));
        for (i, v) in self.string_level.iter().enumerate() {
            put!(0x0B + i as u16, *v);
        }
        for (i, v) in self.pitch_step.iter().enumerate() {
            put!(0x11 + (i as u16) * 2, *v);
        }
        for (i, v) in self.pitch_fine.iter().enumerate() {
            put!(0x12 + (i as u16) * 2, *v);
        }
        put!(0x1D, self.twelve_string.map(OnOff::to_byte));
        for (linear, b) in &self.raw_tail {
            let (page, off) = modeling_address_split(*linear);
            bytes.insert([base_msb, page, 0x00, off], *b);
        }
    }

    /// Iterate every populated (linear, byte, ModelingParamEntry) triple
    /// the slot carries — typed fields plus raw_tail bytes — paired with
    /// their FloorBoard 2-axis ownership metadata.
    pub fn iter_params(
        &self,
    ) -> impl Iterator<Item = (u16, u8, &'static crate::modeling_params::ModelingParamEntry)> + '_
    {
        let mut typed: Vec<(u16, Option<u8>)> = vec![
            (0x00, self.gm_category.map(GuitarModeCategory::to_byte)),
            (0x01, self.gm_egtr_type.map(GmEGuitarType::to_byte)),
            (0x02, self.gm_acoustic_type.map(GmAcousticType::to_byte)),
            (0x03, self.gm_ebass_type.map(GmEBassType::to_byte)),
            (0x04, self.gm_synth_type.map(ModelingSynthType::to_byte)),
            (0x05, self.bm_category.map(BassModeCategory::to_byte)),
            (0x06, self.bm_ebass_type.map(BmEBassType::to_byte)),
            (0x07, self.bm_egtr_type.map(BmEGuitarType::to_byte)),
            (0x08, self.bm_synth_type.map(ModelingSynthType::to_byte)),
            (0x09, self.tone_level),
            (0x0A, self.tone_sw.map(AnalogPuToneSw::to_byte)),
        ];
        for (i, v) in self.string_level.iter().enumerate() {
            typed.push((0x0B + i as u16, *v));
        }
        for (i, v) in self.pitch_step.iter().enumerate() {
            typed.push((0x11 + (i as u16) * 2, *v));
        }
        for (i, v) in self.pitch_fine.iter().enumerate() {
            typed.push((0x12 + (i as u16) * 2, *v));
        }
        typed.push((0x1D, self.twelve_string.map(OnOff::to_byte)));

        typed
            .into_iter()
            .filter_map(|(lin, v)| {
                v.map(|b| (lin, b, &crate::modeling_params::MODELING_PARAMS[lin as usize]))
            })
            .chain(self.raw_tail.iter().map(|(&lin, &b)| {
                (lin, b, &crate::modeling_params::MODELING_PARAMS[lin as usize])
            }))
    }
}

/// Split a linear Modeling offset (0..=255) into the wire (page, offset)
/// pair. Page `0x10` covers linear `0..=127`; page `0x11` covers
/// `128..=255`.
fn modeling_address_split(linear: u16) -> (u8, u8) {
    if linear < 128 {
        (0x10, linear as u8)
    } else {
        (0x11, (linear - 128) as u8)
    }
}

/// Linear PCM tone index (0..=909) for the 910 named tones in the GR-55
/// catalog.
///
/// **Wire encoding** (FloorBoard `midi.xml:44900-45828`): the catalog is
/// split across 8 banks of up to 128 tones each, addressed by two
/// consecutive bytes within the PCM page:
///
/// - PCM-page offset `0x01` = bank (0..=7).
/// - PCM-page offset `0x02` = position within bank (0..=127).
///
/// The linear index is `bank * 128 + position`. Bank 0 pos 0 = tone "001
/// St.Piano 1"; the last populated tone is bank 7 pos 13 = "910 …" (bank
/// 7 is partially filled).
///
/// Storing both bytes as a single `u16` keeps the model wire-correct and
/// makes range-validation (909 is the inclusive max) trivial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PcmToneIndex(u16);

impl PcmToneIndex {
    /// Validated constructor. Accepts 0..=909.
    pub fn new(linear: u16) -> Option<Self> {
        if linear <= 909 {
            Some(PcmToneIndex(linear))
        } else {
            None
        }
    }
    /// Linear index (0..=909). Add 1 for the device's 1-based display
    /// number ("001 St.Piano 1" = linear 0).
    pub fn get(self) -> u16 {
        self.0
    }
    /// Combine the two wire bytes into a linear index. Returns `None`
    /// when the resulting linear value exceeds 909, or when either
    /// component byte exceeds its valid range (bank > 7, pos > 0x7F).
    pub fn from_two_bytes(bank: u8, position: u8) -> Option<Self> {
        if bank > 7 || position > 0x7F {
            return None;
        }
        let linear = (bank as u16) * 128 + (position as u16);
        Self::new(linear)
    }
    /// Split the linear index back into wire bytes (bank, position).
    pub fn to_two_bytes(self) -> [u8; 2] {
        let bank = (self.0 / 128) as u8;
        let position = (self.0 % 128) as u8;
        [bank, position]
    }
    /// Tone name from FloorBoard's PARAM `name` attribute, with the
    /// leading 1-based display number stripped. E.g. linear 0 →
    /// `"St.Piano 1"`, linear 909 → `"Dance Kit 3"`. Pulled from the
    /// build-time-generated table in [`crate::pcm_tones`].
    pub fn name(self) -> &'static str {
        crate::pcm_tones::PCM_TONE_NAMES[self.0 as usize]
    }
    /// Tone category (from FloorBoard's `customdesc`) — e.g.
    /// `"Acoustic Piano"`, `"Synth Lead"`, `"Drums"`. Repeats heavily:
    /// the 910 tones share 46 distinct categories. Useful as a UI
    /// "browse by category" key.
    pub fn category(self) -> &'static str {
        crate::pcm_tones::PCM_TONE_CATEGORIES[self.0 as usize]
    }
    /// 1-based display number (1..=910). Matches the number FloorBoard
    /// prepends to each tone name in the original midi.xml PARAM list.
    pub fn display_number(self) -> u16 {
        self.0 + 1
    }
}

/// PCM tone portamento switch at PCM-page offset `0x0C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortamentoSwitch {
    Off,
    On,
    Tone,
}

impl PortamentoSwitch {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Off),
            0x01 => Some(Self::On),
            0x02 => Some(Self::Tone),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Off => 0x00,
            Self::On => 0x01,
            Self::Tone => 0x02,
        }
    }
}

/// PCM tone TVA release mode at PCM-page offset `0x0F`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvaReleaseMode {
    Mode1,
    Mode2,
}

impl TvaReleaseMode {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Mode1),
            0x01 => Some(Self::Mode2),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Mode1 => 0x00,
            Self::Mode2 => 0x01,
        }
    }
}

/// One PCM tone slot. The GR-55 has 4: PCM-1-A (page `0x20`), PCM-2-A
/// (page `0x21`), PCM-1-B (page `0x30`), PCM-2-B (page `0x31`).
///
/// This commit types the common header (offsets `0x00..=0x0F`); the
/// remaining 112 bytes (`0x10..=0x7F`) form a PCM-tone-type-dependent
/// envelope/filter/LFO/effects block kept in `raw_tail` pending
/// per-tone sum modelling.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pcm {
    /// Synth mode at offset `0x00`. FloorBoard documents 0x58 = synth,
    /// 0x56 = drum; other values may be valid. Raw u8.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synth_mode: Option<u8>,
    /// PCM tone index at offsets `0x01` (bank) + `0x02` (position).
    /// Combined into a single linear 0..=909 — see [`PcmToneIndex`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synth_tone: Option<PcmToneIndex>,
    /// Tone switch at `0x03`. Uses the same wire-reversed encoding as
    /// [`AnalogPuToneSw`] (0x00 = On, 0x01 = Off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone_sw: Option<AnalogPuToneSw>,
    /// Tone level at `0x04` (raw 0..=127, display 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone_level: Option<u8>,
    /// Octave at `0x05` — wire `0x3D..=0x43` maps to `-3..=+3`. Stored
    /// raw to match the SystemArea convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub octave: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chromatic: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legato: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nuance_sw: Option<OnOff>,
    /// Pan at `0x09`. FloorBoard documents a non-monotonic L50..R50
    /// mapping across bytes `0x01..=0x7F`; stored raw for round-trip
    /// fidelity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pan: Option<u8>,
    /// Pitch shift at `0x0A` — wire `0x28..=0x58` = -24..=+24 semitones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_shift: Option<u8>,
    /// Pitch fine at `0x0B` — wire `0x0E..=0x72` = -50..=+50 cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_fine: Option<u8>,
    /// Portamento switch at `0x0C` (Off / On / Tone).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portamento_sw: Option<PortamentoSwitch>,
    /// Portamento time at `0x0D` (raw 0..=7).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portamento_time: Option<u8>,
    /// Offset `0x0E` is `abbr="Portamento"` but has no `customdesc`.
    /// Raw u8 0..=15.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portamento_raw_0e: Option<u8>,
    /// TVA release mode at `0x0F` (Mode 1 / Mode 2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tva_release_mode: Option<TvaReleaseMode>,
    /// Per-string PCM level at `0x10..=0x15` (strings 1..=6 = String 1 [H]
    /// through String 6 [L], raw 0..=100). FloorBoard `midi.xml` documents
    /// these as `desc="String level"`.
    #[serde(default, skip_serializing_if = "string_shift_all_none")]
    pub string_level: [Option<u8>; 6],
    /// PCM line route at `0x16` (`ByPass` / `Amp/MOD` / `MFX`). Reuses
    /// the [`LineRoute`] enum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_route: Option<LineRoute>,

    /// Decode-time buffer for the header-page bank byte (`0x20:0x01`
    /// or `0x21:0x01`). Lifted into [`synth_tone`] in `finalize`.
    #[doc(hidden)]
    #[serde(skip)]
    pending_bank: Option<u8>,
    /// Decode-time buffer for the header-page position byte (`0x20:0x02`
    /// or `0x21:0x02`). Lifted into [`synth_tone`] in `finalize`.
    #[doc(hidden)]
    #[serde(skip)]
    pending_pos: Option<u8>,

    /// Bytes on this PCM tone's **tail page** (`0x30` for Tone 1,
    /// `0x31` for Tone 2), keyed by offset within that page (`0x00..=0x7F`).
    ///
    /// FloorBoard `midi.xml` tags every byte here as `customdesc="null"`,
    /// but FloorBoard's *C++ source* (specifically
    /// `soundSource_synth_a.cpp` lines 88–158) documents the complete
    /// layout. The tail holds the standard Roland synth-voice
    /// parameter block:
    ///
    /// | Offset | Param | Offset | Param |
    /// |---|---|---|---|
    /// | 0x00 | Filter Type | 0x14 | TVA Attack Vel Sens |
    /// | 0x01 | Cutoff | 0x15 | TVA Atk Nuance Sens |
    /// | 0x02 | Resonance | 0x16 | TVA Level Nuance Sens |
    /// | 0x03 | Cutoff Vel Sens | 0x17 | Pitch ENV Vel Sens |
    /// | 0x04 | Cutoff Vel Curve | 0x18 | Pitch ENV Depth |
    /// | 0x05 | Cutoff Keyfollow | 0x19 | Pitch Attack Time |
    /// | 0x06 | Cutoff Nuance Sens | 0x1A | Pitch Decay Time |
    /// | 0x07 | TVF Env Depth | 0x1B | Portamento Type (RATE/TIME) |
    /// | 0x08 | TVF Attack Time | 0x1C | LFO1 Rate |
    /// | 0x09 | TVF Decay Time | 0x1E–0x21 | LFO1 Pitch/TVF/TVA/Pan Depth |
    /// | 0x0A | TVF Sustain Level | 0x22 | LFO2 Rate |
    /// | 0x0B | TVF Release Time | 0x24–0x27 | LFO2 Pitch/TVF/TVA/Pan Depth |
    /// | 0x0C | TVF Attack Vel Sens | | |
    /// | 0x0D | TVF Atk Nuance Sens | | |
    /// | 0x0E | Level Velocity Sens | | |
    /// | 0x0F | Velocity Curve Type | | |
    /// | 0x10 | TVA Attack Time | | |
    /// | 0x11 | TVA Decay Time | | |
    /// | 0x12 | TVA Sustain Level | | |
    /// | 0x13 | TVA Release Time | | |
    ///
    /// (Offsets `0x1D` and `0x23` are not referenced by FloorBoard's
    /// UI — likely reserved.)
    ///
    /// These bytes round-trip losslessly through this map until a
    /// follow-up commit promotes them to typed fields backed by a
    /// build-time-generated param table.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw_tail: BTreeMap<u8, u8>,
}

impl Pcm {
    /// Absorb a byte that arrived on this slot's **header page**
    /// (`0x20` for Tone 1, `0x21` for Tone 2). The byte's offset
    /// is within that page (`0x00..=0x7F`).
    fn store_header_byte(&mut self, off: u8, b: u8) -> bool {
        match off {
            0x00 => {
                self.synth_mode = Some(b);
                true
            }
            // 0x01 (bank) and 0x02 (position) are parked in dedicated
            // `pending_*` fields during the byte-by-byte pass and
            // lifted into `synth_tone: PcmToneIndex` in `finalize`
            // once both bytes are known.
            0x01 => {
                self.pending_bank = Some(b);
                true
            }
            0x02 => {
                self.pending_pos = Some(b);
                true
            }
            0x03 => match AnalogPuToneSw::from_byte(b) {
                Some(v) => {
                    self.tone_sw = Some(v);
                    true
                }
                None => false,
            },
            0x04 if b <= 0x7F => {
                self.tone_level = Some(b);
                true
            }
            0x05 if (0x3D..=0x43).contains(&b) => {
                self.octave = Some(b);
                true
            }
            0x06 => match OnOff::from_byte(b) {
                Some(v) => {
                    self.chromatic = Some(v);
                    true
                }
                None => false,
            },
            0x07 => match OnOff::from_byte(b) {
                Some(v) => {
                    self.legato = Some(v);
                    true
                }
                None => false,
            },
            0x08 => match OnOff::from_byte(b) {
                Some(v) => {
                    self.nuance_sw = Some(v);
                    true
                }
                None => false,
            },
            0x09 if b <= 0x7F => {
                self.pan = Some(b);
                true
            }
            0x0A if (0x28..=0x58).contains(&b) => {
                self.pitch_shift = Some(b);
                true
            }
            0x0B if (0x0E..=0x72).contains(&b) => {
                self.pitch_fine = Some(b);
                true
            }
            0x0C => match PortamentoSwitch::from_byte(b) {
                Some(v) => {
                    self.portamento_sw = Some(v);
                    true
                }
                None => false,
            },
            0x0D if b <= 7 => {
                self.portamento_time = Some(b);
                true
            }
            0x0E if b <= 0x0F => {
                self.portamento_raw_0e = Some(b);
                true
            }
            0x0F => match TvaReleaseMode::from_byte(b) {
                Some(v) => {
                    self.tva_release_mode = Some(v);
                    true
                }
                None => false,
            },
            0x10..=0x15 if b <= 100 => {
                self.string_level[(off - 0x10) as usize] = Some(b);
                true
            }
            0x16 => match LineRoute::from_byte(b) {
                Some(v) => {
                    self.line_route = Some(v);
                    true
                }
                None => false,
            },
            // 0x17..=0x7F are FloorBoard-undocumented header-page
            // offsets — most are placeholder/reserved bytes (the
            // header page tops out at line_route at 0x16). Route to
            // the patch-level unknown_bytes by failing the match.
            _ => false,
        }
    }

    /// Absorb a byte that arrived on this slot's **tail page** (`0x30`
    /// for Tone 1, `0x31` for Tone 2). The byte's offset is within
    /// that page (`0x00..=0x7F`). All tail bytes currently pass
    /// through `raw_tail`; a follow-up commit will type the documented
    /// FILTER/TVF/TVA/Pitch ENV/LFO1/LFO2 parameters per
    /// `soundSource_synth_a.cpp`.
    fn store_tail_byte(&mut self, off: u8, b: u8) -> bool {
        if off <= 0x7F {
            self.raw_tail.insert(off, b);
            true
        } else {
            false
        }
    }

    /// Combine the parked bank+position bytes into the typed
    /// `synth_tone` field. Called once from `PatchArea::from_frames_at`
    /// after the byte-by-byte sweep. If only one of the two bytes is
    /// present, they stay parked for the next round-trip; if the
    /// combined linear value is out of range, both stay parked.
    fn finalize(&mut self) {
        if let (Some(b), Some(p)) = (self.pending_bank, self.pending_pos) {
            if let Some(idx) = PcmToneIndex::from_two_bytes(b, p) {
                self.synth_tone = Some(idx);
                self.pending_bank = None;
                self.pending_pos = None;
            }
        }
    }

    /// Emit this slot's bytes to its two wire pages (header_page +
    /// tail_page, derived from the slot index via [`pcm_pages_for_slot`]).
    fn emit_bytes(&self, bytes: &mut BTreeMap<[u8; 4], u8>, base_msb: u8, slot_idx: usize) {
        let (header_page, tail_page) = pcm_pages_for_slot(slot_idx);
        macro_rules! put_h {
            ($off:expr, $val:expr) => {
                if let Some(v) = $val {
                    bytes.insert([base_msb, header_page, 0x00, $off], v);
                }
            };
        }
        put_h!(0x00, self.synth_mode);
        // synth_tone always emits both wire bytes when set. If
        // synth_tone is None but pending_bank/pending_pos survived a
        // partial decode, emit those instead.
        if let Some(idx) = self.synth_tone {
            let [bank, pos] = idx.to_two_bytes();
            bytes.insert([base_msb, header_page, 0x00, 0x01], bank);
            bytes.insert([base_msb, header_page, 0x00, 0x02], pos);
        } else {
            if let Some(b) = self.pending_bank {
                bytes.insert([base_msb, header_page, 0x00, 0x01], b);
            }
            if let Some(p) = self.pending_pos {
                bytes.insert([base_msb, header_page, 0x00, 0x02], p);
            }
        }
        put_h!(0x03, self.tone_sw.map(AnalogPuToneSw::to_byte));
        put_h!(0x04, self.tone_level);
        put_h!(0x05, self.octave);
        put_h!(0x06, self.chromatic.map(OnOff::to_byte));
        put_h!(0x07, self.legato.map(OnOff::to_byte));
        put_h!(0x08, self.nuance_sw.map(OnOff::to_byte));
        put_h!(0x09, self.pan);
        put_h!(0x0A, self.pitch_shift);
        put_h!(0x0B, self.pitch_fine);
        put_h!(0x0C, self.portamento_sw.map(PortamentoSwitch::to_byte));
        put_h!(0x0D, self.portamento_time);
        put_h!(0x0E, self.portamento_raw_0e);
        put_h!(0x0F, self.tva_release_mode.map(TvaReleaseMode::to_byte));
        for (i, v) in self.string_level.iter().enumerate() {
            put_h!(0x10 + i as u8, *v);
        }
        put_h!(0x16, self.line_route.map(LineRoute::to_byte));
        for (off, b) in &self.raw_tail {
            bytes.insert([base_msb, tail_page, 0x00, *off], *b);
        }
    }

    /// Iterate this tone's set tail bytes paired with their FloorBoard
    /// metadata from [`crate::pcm_tail_params::PCM_TAIL_PARAMS`]. Bytes
    /// at offsets `0x28..=0x7F` (FloorBoard-undocumented) yield `None`
    /// for the metadata slot.
    pub fn iter_tail_params(
        &self,
    ) -> impl Iterator<
        Item = (
            u8,
            u8,
            Option<&'static crate::pcm_tail_params::PcmTailParamEntry>,
        ),
    > + '_ {
        self.raw_tail
            .iter()
            .map(|(&off, &b)| (off, b, crate::pcm_tail_params::param_for(off)))
    }
}

fn all_pcm_none(arr: &[Option<Pcm>; 2]) -> bool {
    arr.iter().all(Option::is_none)
}

/// Where each of a PCM tone's two MIDI pages routes inside a single
/// `Pcm` slot.
///
/// **Wire model** (confirmed against FloorBoard's
/// `soundSource_synth_a.cpp` / `soundSource_synth_b.cpp`): the GR-55
/// has **two** PCM tones, not four. Each tone's data spans two pages:
///
/// - PCM Tone 1: common header on page `0x20`, tone-shaping tail
///   (Filter / TVF / TVA / Pitch Env / LFO1 / LFO2) on page `0x30`.
/// - PCM Tone 2: common header on `0x21`, tail on `0x31`.
///
/// FloorBoard `midi.xml` labels pages `0x30` / `0x31` as "PCM-1-B" /
/// "PCM-2-B" which suggested four independent slots, but the actual
/// MIDI memory layout pairs them with `0x20` / `0x21` as the same
/// tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PcmPageRole {
    /// Slot index (0 = Tone 1, 1 = Tone 2) + this byte goes on the
    /// common header page.
    Header(usize),
    /// Slot index + the byte goes on the tone-shaping tail page.
    Tail(usize),
}

fn pcm_route_for_page(page: u8) -> Option<PcmPageRole> {
    match page {
        0x20 => Some(PcmPageRole::Header(0)),
        0x21 => Some(PcmPageRole::Header(1)),
        0x30 => Some(PcmPageRole::Tail(0)),
        0x31 => Some(PcmPageRole::Tail(1)),
        _ => None,
    }
}

/// The two wire pages a slot writes its bytes to: `(header_page, tail_page)`.
fn pcm_pages_for_slot(idx: usize) -> (u8, u8) {
    match idx {
        0 => (0x20, 0x30),
        1 => (0x21, 0x31),
        _ => unreachable!("pcm slot index out of range"),
    }
}

/// Guitar Mode modeling category at page `0x10` offset `0x00`. Selects
/// which of `gm_egtr_type` / `gm_acoustic_type` / `gm_ebass_type` /
/// `gm_synth_type` the device actually plays. Mined from FloorBoard
/// `midi.xml:43855-43860`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuitarModeCategory {
    ElectricGuitar,
    Acoustic,
    ElectricBass,
    Synth,
}

impl GuitarModeCategory {
    pub fn from_byte(b: u8) -> Option<Self> {
        use GuitarModeCategory::*;
        Some(match b {
            0x00 => ElectricGuitar,
            0x01 => Acoustic,
            0x02 => ElectricBass,
            0x03 => Synth,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use GuitarModeCategory::*;
        match self {
            ElectricGuitar => 0x00,
            Acoustic => 0x01,
            ElectricBass => 0x02,
            Synth => 0x03,
        }
    }
}

/// Guitar Mode E.Guitar type at page `0x10` offset `0x01`. 10 vintage-amp /
/// instrument emulations. Mined from FloorBoard `midi.xml:43862-43873`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GmEGuitarType {
    ClassicStrat,
    ModernStrat,
    HhStrat,
    Telecaster,
    LesPaul,
    LesPaulP90,
    Lips,
    Rickenbacker,
    Gibson335,
    GibsonL4,
}

impl GmEGuitarType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use GmEGuitarType::*;
        Some(match b {
            0x00 => ClassicStrat,
            0x01 => ModernStrat,
            0x02 => HhStrat,
            0x03 => Telecaster,
            0x04 => LesPaul,
            0x05 => LesPaulP90,
            0x06 => Lips,
            0x07 => Rickenbacker,
            0x08 => Gibson335,
            0x09 => GibsonL4,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use GmEGuitarType::*;
        match self {
            ClassicStrat => 0x00,
            ModernStrat => 0x01,
            HhStrat => 0x02,
            Telecaster => 0x03,
            LesPaul => 0x04,
            LesPaulP90 => 0x05,
            Lips => 0x06,
            Rickenbacker => 0x07,
            Gibson335 => 0x08,
            GibsonL4 => 0x09,
        }
    }
}

/// Guitar Mode Acoustic instrument type at page `0x10` offset `0x02`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GmAcousticType {
    Steel,
    Nylon,
    Sitar,
    Banjo,
    Resonator,
}

impl GmAcousticType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use GmAcousticType::*;
        Some(match b {
            0x00 => Steel,
            0x01 => Nylon,
            0x02 => Sitar,
            0x03 => Banjo,
            0x04 => Resonator,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use GmAcousticType::*;
        match self {
            Steel => 0x00,
            Nylon => 0x01,
            Sitar => 0x02,
            Banjo => 0x03,
            Resonator => 0x04,
        }
    }
}

/// Guitar Mode E.Bass type at page `0x10` offset `0x03`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GmEBassType {
    JazzBass,
    PBass,
}

impl GmEBassType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::JazzBass),
            0x01 => Some(Self::PBass),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::JazzBass => 0x00,
            Self::PBass => 0x01,
        }
    }
}

/// Modeling Synth type. Used at both page `0x10` offset `0x04` (Guitar
/// Mode) and page `0x10` offset `0x08` (Bass Mode) — the byte layout
/// is identical for both modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelingSynthType {
    AnalogGr,
    Wave,
    FilterBass,
    Crystal,
    Organ,
    Brass,
}

impl ModelingSynthType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ModelingSynthType::*;
        Some(match b {
            0x00 => AnalogGr,
            0x01 => Wave,
            0x02 => FilterBass,
            0x03 => Crystal,
            0x04 => Organ,
            0x05 => Brass,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use ModelingSynthType::*;
        match self {
            AnalogGr => 0x00,
            Wave => 0x01,
            FilterBass => 0x02,
            Crystal => 0x03,
            Organ => 0x04,
            Brass => 0x05,
        }
    }
}

/// Bass Mode modeling category at page `0x10` offset `0x05`. Distinct
/// from `GuitarModeCategory` — Bass Mode omits `Acoustic` and reorders
/// the remaining three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BassModeCategory {
    ElectricBass,
    Synth,
    ElectricGuitar,
}

impl BassModeCategory {
    pub fn from_byte(b: u8) -> Option<Self> {
        use BassModeCategory::*;
        Some(match b {
            0x00 => ElectricBass,
            0x01 => Synth,
            0x02 => ElectricGuitar,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use BassModeCategory::*;
        match self {
            ElectricBass => 0x00,
            Synth => 0x01,
            ElectricGuitar => 0x02,
        }
    }
}

/// Bass Mode E.Bass type at page `0x10` offset `0x06`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BmEBassType {
    VintJazzBass,
    JazzBass,
    VintPBass,
    PBass,
    MusicMan,
    Rickenbacker,
    ThunderBird,
    Active,
    HofnerViolin,
}

impl BmEBassType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use BmEBassType::*;
        Some(match b {
            0x00 => VintJazzBass,
            0x01 => JazzBass,
            0x02 => VintPBass,
            0x03 => PBass,
            0x04 => MusicMan,
            0x05 => Rickenbacker,
            0x06 => ThunderBird,
            0x07 => Active,
            0x08 => HofnerViolin,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use BmEBassType::*;
        match self {
            VintJazzBass => 0x00,
            JazzBass => 0x01,
            VintPBass => 0x02,
            PBass => 0x03,
            MusicMan => 0x04,
            Rickenbacker => 0x05,
            ThunderBird => 0x06,
            Active => 0x07,
            HofnerViolin => 0x08,
        }
    }
}

/// Bass Mode E.Guitar type at page `0x10` offset `0x07` (only 2 vars —
/// Classic Strat and Les Paul).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BmEGuitarType {
    ClassicStrat,
    LesPaul,
}

impl BmEGuitarType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::ClassicStrat),
            0x01 => Some(Self::LesPaul),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::ClassicStrat => 0x00,
            Self::LesPaul => 0x01,
        }
    }
}

/// PreAmp model at page `0x07` offset `0x01`. 42 variants — all the
/// classic-amp emulations the GR-55 ships with, grouped into named
/// families (JC Clean, TW Clean, Crunch, Combo, Match, BG Lead, MS
/// Classic, MS Modern, R-Fier, T-Amp, HI-Gain, METAL, Bass). Mined
/// from FloorBoard `midi.xml:43069-43111`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreampType {
    BossClean,
    Jc120,
    JazzCombo,
    FullRange,
    CleanTwin,
    ProCrunch,
    Tweed,
    DeluxCrunch,
    BossCrunch,
    Blues,
    WildCrunch,
    StackCrunch,
    VoDrive,
    VoLead,
    VoClean,
    MatchDrive,
    FatMatch,
    MatchLead,
    BgLead,
    BgDrive,
    BgRhythm,
    Ms1959I,
    Ms1959IPlusII,
    MsHiGain,
    MsScoop,
    RFierVintage,
    RFierModern,
    RFierClean,
    TAmpLead,
    TAmpCrunch,
    TAmpClean,
    BossDrive,
    Sldn,
    LeadStack,
    HeavyLead,
    BossMetal,
    Drive5150,
    MetalLead,
    EdgeLead,
    BassClean,
    BassCrunch,
    BassHiGain,
}

impl PreampType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use PreampType::*;
        Some(match b {
            0x00 => BossClean,
            0x01 => Jc120,
            0x02 => JazzCombo,
            0x03 => FullRange,
            0x04 => CleanTwin,
            0x05 => ProCrunch,
            0x06 => Tweed,
            0x07 => DeluxCrunch,
            0x08 => BossCrunch,
            0x09 => Blues,
            0x0A => WildCrunch,
            0x0B => StackCrunch,
            0x0C => VoDrive,
            0x0D => VoLead,
            0x0E => VoClean,
            0x0F => MatchDrive,
            0x10 => FatMatch,
            0x11 => MatchLead,
            0x12 => BgLead,
            0x13 => BgDrive,
            0x14 => BgRhythm,
            0x15 => Ms1959I,
            0x16 => Ms1959IPlusII,
            0x17 => MsHiGain,
            0x18 => MsScoop,
            0x19 => RFierVintage,
            0x1A => RFierModern,
            0x1B => RFierClean,
            0x1C => TAmpLead,
            0x1D => TAmpCrunch,
            0x1E => TAmpClean,
            0x1F => BossDrive,
            0x20 => Sldn,
            0x21 => LeadStack,
            0x22 => HeavyLead,
            0x23 => BossMetal,
            0x24 => Drive5150,
            0x25 => MetalLead,
            0x26 => EdgeLead,
            0x27 => BassClean,
            0x28 => BassCrunch,
            0x29 => BassHiGain,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use PreampType::*;
        match self {
            BossClean => 0x00,
            Jc120 => 0x01,
            JazzCombo => 0x02,
            FullRange => 0x03,
            CleanTwin => 0x04,
            ProCrunch => 0x05,
            Tweed => 0x06,
            DeluxCrunch => 0x07,
            BossCrunch => 0x08,
            Blues => 0x09,
            WildCrunch => 0x0A,
            StackCrunch => 0x0B,
            VoDrive => 0x0C,
            VoLead => 0x0D,
            VoClean => 0x0E,
            MatchDrive => 0x0F,
            FatMatch => 0x10,
            MatchLead => 0x11,
            BgLead => 0x12,
            BgDrive => 0x13,
            BgRhythm => 0x14,
            Ms1959I => 0x15,
            Ms1959IPlusII => 0x16,
            MsHiGain => 0x17,
            MsScoop => 0x18,
            RFierVintage => 0x19,
            RFierModern => 0x1A,
            RFierClean => 0x1B,
            TAmpLead => 0x1C,
            TAmpCrunch => 0x1D,
            TAmpClean => 0x1E,
            BossDrive => 0x1F,
            Sldn => 0x20,
            LeadStack => 0x21,
            HeavyLead => 0x22,
            BossMetal => 0x23,
            Drive5150 => 0x24,
            MetalLead => 0x25,
            EdgeLead => 0x26,
            BassClean => 0x27,
            BassCrunch => 0x28,
            BassHiGain => 0x29,
        }
    }
}

/// PreAmp Gain switch at page `0x07` offset `0x04`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreampGainSw {
    Low,
    Middle,
    High,
}

impl PreampGainSw {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Low),
            0x01 => Some(Self::Middle),
            0x02 => Some(Self::High),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Low => 0x00,
            Self::Middle => 0x01,
            Self::High => 0x02,
        }
    }
}

/// Speaker simulator cabinet at page `0x07` offset `0x0C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerType {
    Off,
    Original,
    OneByEight,
    OneByTen,
    OneByTwelve,
    TwoByTwelve,
    FourByTen,
    FourByTwelve,
    EightByTwelve,
}

impl SpeakerType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use SpeakerType::*;
        Some(match b {
            0x00 => Off,
            0x01 => Original,
            0x02 => OneByEight,
            0x03 => OneByTen,
            0x04 => OneByTwelve,
            0x05 => TwoByTwelve,
            0x06 => FourByTen,
            0x07 => FourByTwelve,
            0x08 => EightByTwelve,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use SpeakerType::*;
        match self {
            Off => 0x00,
            Original => 0x01,
            OneByEight => 0x02,
            OneByTen => 0x03,
            OneByTwelve => 0x04,
            TwoByTwelve => 0x05,
            FourByTen => 0x06,
            FourByTwelve => 0x07,
            EightByTwelve => 0x08,
        }
    }
}

/// Microphone model at page `0x07` offset `0x0D`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicType {
    Dyn57,
    Dyn421,
    Cnd451,
    Cnd87,
    Flat,
}

impl MicType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use MicType::*;
        Some(match b {
            0x00 => Dyn57,
            0x01 => Dyn421,
            0x02 => Cnd451,
            0x03 => Cnd87,
            0x04 => Flat,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use MicType::*;
        match self {
            Dyn57 => 0x00,
            Dyn421 => 0x01,
            Cnd451 => 0x02,
            Cnd87 => 0x03,
            Flat => 0x04,
        }
    }
}

/// Microphone distance (off-mic / on-mic) at page `0x07` offset `0x0E`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicDistance {
    OffMic,
    OnMic,
}

impl MicDistance {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::OffMic),
            0x01 => Some(Self::OnMic),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::OffMic => 0x00,
            Self::OnMic => 0x01,
        }
    }
}

/// MOD effect type at page `0x07` offset `0x16`. 14 variants determining
/// what the bytes at `0x18..=0x7F` mean. Mined from FloorBoard
/// `midi.xml:43221-43236`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModType {
    Distortion,
    Wah,
    Compressor,
    Limiter,
    Octave,
    Phaser,
    Flanger,
    Tremolo,
    Rotary,
    UniVibe,
    Panner,
    Delay,
    Chorus,
    Equalizer,
}

impl ModType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ModType::*;
        Some(match b {
            0x00 => Distortion,
            0x01 => Wah,
            0x02 => Compressor,
            0x03 => Limiter,
            0x04 => Octave,
            0x05 => Phaser,
            0x06 => Flanger,
            0x07 => Tremolo,
            0x08 => Rotary,
            0x09 => UniVibe,
            0x0A => Panner,
            0x0B => Delay,
            0x0C => Chorus,
            0x0D => Equalizer,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use ModType::*;
        match self {
            Distortion => 0x00,
            Wah => 0x01,
            Compressor => 0x02,
            Limiter => 0x03,
            Octave => 0x04,
            Phaser => 0x05,
            Flanger => 0x06,
            Tremolo => 0x07,
            Rotary => 0x08,
            UniVibe => 0x09,
            Panner => 0x0A,
            Delay => 0x0B,
            Chorus => 0x0C,
            Equalizer => 0x0D,
        }
    }
}

/// Chorus algorithm at page `0x06` offset `0x01`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChorusType {
    Mono,
    Stereo,
    MonoMild,
    StereoMild,
}

impl ChorusType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ChorusType::*;
        Some(match b {
            0x00 => Mono,
            0x01 => Stereo,
            0x02 => MonoMild,
            0x03 => StereoMild,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use ChorusType::*;
        match self {
            Mono => 0x00,
            Stereo => 0x01,
            MonoMild => 0x02,
            StereoMild => 0x03,
        }
    }
}

/// Delay algorithm at page `0x06` offset `0x06`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelayType {
    Single,
    Pan,
    Reverse,
    Analog,
    Tape,
    Modulate,
    HiCut,
}

impl DelayType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use DelayType::*;
        Some(match b {
            0x00 => Single,
            0x01 => Pan,
            0x02 => Reverse,
            0x03 => Analog,
            0x04 => Tape,
            0x05 => Modulate,
            0x06 => HiCut,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use DelayType::*;
        match self {
            Single => 0x00,
            Pan => 0x01,
            Reverse => 0x02,
            Analog => 0x03,
            Tape => 0x04,
            Modulate => 0x05,
            HiCut => 0x06,
        }
    }
}

/// Reverb algorithm at page `0x06` offset `0x0D`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReverbType {
    Ambience,
    Room,
    Hall1,
    Hall2,
    Plate,
}

impl ReverbType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ReverbType::*;
        Some(match b {
            0x00 => Ambience,
            0x01 => Room,
            0x02 => Hall1,
            0x03 => Hall2,
            0x04 => Plate,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use ReverbType::*;
        match self {
            Ambience => 0x00,
            Room => 0x01,
            Hall1 => 0x02,
            Hall2 => 0x03,
            Plate => 0x04,
        }
    }
}

/// Reverb High Cut at page `0x06` offset `0x0F` (10 settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReverbHighCut {
    Hz700,
    Khz1_0,
    Khz1_4,
    Khz2_0,
    Khz3_0,
    Khz4_0,
    Khz6_0,
    Khz8_0,
    Khz11,
    Flat,
}

impl ReverbHighCut {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ReverbHighCut::*;
        Some(match b {
            0x00 => Hz700,
            0x01 => Khz1_0,
            0x02 => Khz1_4,
            0x03 => Khz2_0,
            0x04 => Khz3_0,
            0x05 => Khz4_0,
            0x06 => Khz6_0,
            0x07 => Khz8_0,
            0x08 => Khz11,
            0x09 => Flat,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use ReverbHighCut::*;
        match self {
            Hz700 => 0x00,
            Khz1_0 => 0x01,
            Khz1_4 => 0x02,
            Khz2_0 => 0x03,
            Khz3_0 => 0x04,
            Khz4_0 => 0x05,
            Khz6_0 => 0x06,
            Khz8_0 => 0x07,
            Khz11 => 0x08,
            Flat => 0x09,
        }
    }
}

/// Patch EQ Lo Cut at page `0x06` offset `0x12` (11 settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchEqLoCut {
    Flat,
    Hz55,
    Hz110,
    Hz165,
    Hz200,
    Hz280,
    Hz340,
    Hz400,
    Hz500,
    Hz630,
    Hz800,
}

impl PatchEqLoCut {
    pub fn from_byte(b: u8) -> Option<Self> {
        use PatchEqLoCut::*;
        Some(match b {
            0x00 => Flat,
            0x01 => Hz55,
            0x02 => Hz110,
            0x03 => Hz165,
            0x04 => Hz200,
            0x05 => Hz280,
            0x06 => Hz340,
            0x07 => Hz400,
            0x08 => Hz500,
            0x09 => Hz630,
            0x0A => Hz800,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use PatchEqLoCut::*;
        match self {
            Flat => 0x00,
            Hz55 => 0x01,
            Hz110 => 0x02,
            Hz165 => 0x03,
            Hz200 => 0x04,
            Hz280 => 0x05,
            Hz340 => 0x06,
            Hz400 => 0x07,
            Hz500 => 0x08,
            Hz630 => 0x09,
            Hz800 => 0x0A,
        }
    }
}

/// Patch EQ Lo/Hi Mid Freq at page `0x06` offsets `0x14` and `0x17`
/// (28 settings, 20Hz..=10kHz). Reused for both bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchEqMidFreq {
    Hz20_0,
    Hz25_0,
    Hz31_5,
    Hz40_0,
    Hz50_0,
    Hz63_0,
    Hz80_0,
    Hz100,
    Hz125,
    Hz160,
    Hz200,
    Hz250,
    Hz315,
    Hz400,
    Hz500,
    Hz630,
    Hz800,
    Khz1_00,
    Khz1_25,
    Khz1_60,
    Khz2_00,
    Khz2_50,
    Khz3_15,
    Khz4_00,
    Khz5_00,
    Khz6_30,
    Khz8_00,
    Khz10_0,
}

impl PatchEqMidFreq {
    pub fn from_byte(b: u8) -> Option<Self> {
        use PatchEqMidFreq::*;
        Some(match b {
            0x00 => Hz20_0,
            0x01 => Hz25_0,
            0x02 => Hz31_5,
            0x03 => Hz40_0,
            0x04 => Hz50_0,
            0x05 => Hz63_0,
            0x06 => Hz80_0,
            0x07 => Hz100,
            0x08 => Hz125,
            0x09 => Hz160,
            0x0A => Hz200,
            0x0B => Hz250,
            0x0C => Hz315,
            0x0D => Hz400,
            0x0E => Hz500,
            0x0F => Hz630,
            0x10 => Hz800,
            0x11 => Khz1_00,
            0x12 => Khz1_25,
            0x13 => Khz1_60,
            0x14 => Khz2_00,
            0x15 => Khz2_50,
            0x16 => Khz3_15,
            0x17 => Khz4_00,
            0x18 => Khz5_00,
            0x19 => Khz6_30,
            0x1A => Khz8_00,
            0x1B => Khz10_0,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use PatchEqMidFreq::*;
        match self {
            Hz20_0 => 0x00,
            Hz25_0 => 0x01,
            Hz31_5 => 0x02,
            Hz40_0 => 0x03,
            Hz50_0 => 0x04,
            Hz63_0 => 0x05,
            Hz80_0 => 0x06,
            Hz100 => 0x07,
            Hz125 => 0x08,
            Hz160 => 0x09,
            Hz200 => 0x0A,
            Hz250 => 0x0B,
            Hz315 => 0x0C,
            Hz400 => 0x0D,
            Hz500 => 0x0E,
            Hz630 => 0x0F,
            Hz800 => 0x10,
            Khz1_00 => 0x11,
            Khz1_25 => 0x12,
            Khz1_60 => 0x13,
            Khz2_00 => 0x14,
            Khz2_50 => 0x15,
            Khz3_15 => 0x16,
            Khz4_00 => 0x17,
            Khz5_00 => 0x18,
            Khz6_30 => 0x19,
            Khz8_00 => 0x1A,
            Khz10_0 => 0x1B,
        }
    }
}

/// Patch EQ Lo/Hi Mid Q at page `0x06` offsets `0x15` and `0x18`
/// (6 settings). Reused for both bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchEqMidQ {
    Q0_5,
    Q1,
    Q2,
    Q4,
    Q8,
    Q16,
}

impl PatchEqMidQ {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Q0_5),
            0x01 => Some(Self::Q1),
            0x02 => Some(Self::Q2),
            0x03 => Some(Self::Q4),
            0x04 => Some(Self::Q8),
            0x05 => Some(Self::Q16),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Q0_5 => 0x00,
            Self::Q1 => 0x01,
            Self::Q2 => 0x02,
            Self::Q4 => 0x03,
            Self::Q8 => 0x04,
            Self::Q16 => 0x05,
        }
    }
}

/// Patch EQ High Cut at page `0x06` offset `0x1A` (10 settings,
/// 700Hz..=11kHz + Flat). Same byte layout as [`ReverbHighCut`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchEqHighCut {
    Hz700,
    Khz1_00,
    Khz1_40,
    Khz2_00,
    Khz3_00,
    Khz4_00,
    Khz6_00,
    Khz8_00,
    Khz11_0,
    Flat,
}

impl PatchEqHighCut {
    pub fn from_byte(b: u8) -> Option<Self> {
        use PatchEqHighCut::*;
        Some(match b {
            0x00 => Hz700,
            0x01 => Khz1_00,
            0x02 => Khz1_40,
            0x03 => Khz2_00,
            0x04 => Khz3_00,
            0x05 => Khz4_00,
            0x06 => Khz6_00,
            0x07 => Khz8_00,
            0x08 => Khz11_0,
            0x09 => Flat,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use PatchEqHighCut::*;
        match self {
            Hz700 => 0x00,
            Khz1_00 => 0x01,
            Khz1_40 => 0x02,
            Khz2_00 => 0x03,
            Khz3_00 => 0x04,
            Khz4_00 => 0x05,
            Khz6_00 => 0x06,
            Khz8_00 => 0x07,
            Khz11_0 => 0x08,
            Flat => 0x09,
        }
    }
}

/// Patch EQ Character at page `0x06` offset `0x1D` (-3..=+3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchEqCharacter {
    Minus3,
    Minus2,
    Minus1,
    Zero,
    Plus1,
    Plus2,
    Plus3,
}

impl PatchEqCharacter {
    pub fn from_byte(b: u8) -> Option<Self> {
        use PatchEqCharacter::*;
        Some(match b {
            0x00 => Minus3,
            0x01 => Minus2,
            0x02 => Minus1,
            0x03 => Zero,
            0x04 => Plus1,
            0x05 => Plus2,
            0x06 => Plus3,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use PatchEqCharacter::*;
        match self {
            Minus3 => 0x00,
            Minus2 => 0x01,
            Minus1 => 0x02,
            Zero => 0x03,
            Plus1 => 0x04,
            Plus2 => 0x05,
            Plus3 => 0x06,
        }
    }
}

/// GK Set selector at page `0x02` offset `0x24`. 11 variants: System
/// (= follow the global system-area setting) plus the 10 per-patch
/// overrides. Mined from FloorBoard `midi.xml:39880-39895`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchGkSet {
    System,
    User1,
    User2,
    User3,
    User4,
    User5,
    User6,
    User7,
    User8,
    User9,
    User10,
}

impl PatchGkSet {
    pub fn from_byte(b: u8) -> Option<Self> {
        use PatchGkSet::*;
        Some(match b {
            0x00 => System,
            0x01 => User1,
            0x02 => User2,
            0x03 => User3,
            0x04 => User4,
            0x05 => User5,
            0x06 => User6,
            0x07 => User7,
            0x08 => User8,
            0x09 => User9,
            0x0A => User10,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use PatchGkSet::*;
        match self {
            System => 0x00,
            User1 => 0x01,
            User2 => 0x02,
            User3 => 0x03,
            User4 => 0x04,
            User5 => 0x05,
            User6 => 0x06,
            User7 => 0x07,
            User8 => 0x08,
            User9 => 0x09,
            User10 => 0x0A,
        }
    }
}

/// Guitar Out routing at page `0x02` offset `0x25`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuitarOut {
    Off,
    NormalPu,
    Modeling,
    Both,
}

impl GuitarOut {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Off),
            0x01 => Some(Self::NormalPu),
            0x02 => Some(Self::Modeling),
            0x03 => Some(Self::Both),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Off => 0x00,
            Self::NormalPu => 0x01,
            Self::Modeling => 0x02,
            Self::Both => 0x03,
        }
    }
}

/// V-LINK control target (EXP, EXP ON, GK VOL fields at page `0x02`
/// offsets `0x29..=0x2B`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VLinkControl {
    Off,
    ColorCb,
    ColorCr,
    Bright,
    PlaySpeed,
}

impl VLinkControl {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Off),
            0x01 => Some(Self::ColorCb),
            0x02 => Some(Self::ColorCr),
            0x03 => Some(Self::Bright),
            0x04 => Some(Self::PlaySpeed),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Off => 0x00,
            Self::ColorCb => 0x01,
            Self::ColorCr => 0x02,
            Self::Bright => 0x03,
            Self::PlaySpeed => 0x04,
        }
    }
}

/// Patch structure selector at page `0x02` offset `0x2C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchStructure {
    Structure1,
    Structure2,
}

impl PatchStructure {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Structure1),
            0x01 => Some(Self::Structure2),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Structure1 => 0x00,
            Self::Structure2 => 0x01,
        }
    }
}

/// Per-mode line route at page `0x02` offsets `0x2D` (Modeling) and `0x2E`
/// (AnalogPU). Three positions: `ByPass`, `Amp/MOD`, `MFX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineRoute {
    ByPass,
    AmpMod,
    Mfx,
}

impl LineRoute {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::ByPass),
            0x01 => Some(Self::AmpMod),
            0x02 => Some(Self::Mfx),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::ByPass => 0x00,
            Self::AmpMod => 0x01,
            Self::Mfx => 0x02,
        }
    }
}

/// Analog PU "Tone Sw" at page `0x02` offset `0x32`. **WIRE-REVERSED**
/// — FloorBoard `midi.xml` lists value 0x00 as `On` and 0x01 as `Off`,
/// the opposite of the standard [`OnOff`] enum. We model it as a
/// distinct type to keep that wire reversal explicit at every site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalogPuToneSw {
    On,
    Off,
}

impl AnalogPuToneSw {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::On),
            0x01 => Some(Self::Off),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::On => 0x00,
            Self::Off => 0x01,
        }
    }
}

/// Alt Tuning preset at page `0x02` offset `0x35`. 13 variants.
///
/// **NOTE:** FloorBoard `midi.xml` has a data bug here — entries for
/// `-1 Octave`, `+1 Octave`, and `User` all collide at `value="0A"`.
/// We assume the intended sequential mapping (`0x0A`, `0x0B`, `0x0C`)
/// since the device has 13 distinct alt-tuning presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AltTuningType {
    OpenD,
    OpenE,
    OpenG,
    OpenA,
    DropD,
    DModal,
    MinusOneStep,
    MinusTwoStep,
    Baritone,
    Nashville,
    MinusOneOctave,
    PlusOneOctave,
    User,
}

impl AltTuningType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use AltTuningType::*;
        Some(match b {
            0x00 => OpenD,
            0x01 => OpenE,
            0x02 => OpenG,
            0x03 => OpenA,
            0x04 => DropD,
            0x05 => DModal,
            0x06 => MinusOneStep,
            0x07 => MinusTwoStep,
            0x08 => Baritone,
            0x09 => Nashville,
            0x0A => MinusOneOctave,
            0x0B => PlusOneOctave,
            0x0C => User,
            _ => return None,
        })
    }
    pub fn to_byte(self) -> u8 {
        use AltTuningType::*;
        match self {
            OpenD => 0x00,
            OpenE => 0x01,
            OpenG => 0x02,
            OpenA => 0x03,
            DropD => 0x04,
            DModal => 0x05,
            MinusOneStep => 0x06,
            MinusTwoStep => 0x07,
            Baritone => 0x08,
            Nashville => 0x09,
            MinusOneOctave => 0x0A,
            PlusOneOctave => 0x0B,
            User => 0x0C,
        }
    }
}

/// Per-patch tempo. Wire encoding is 4-nibble (same as
/// `crate::system::MasterBpm` and the System-area `PatchLevel`):
/// the high nibble lives at page `0x02` offset `0x3C` and the low
/// nibble at `0x3D`. Roland devices typically support 40..=250 BPM
/// for patch tempo; the type accepts the full `0..=255` the 4-nibble
/// encoding can carry and leaves device-specific clamping to the
/// hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatchTempo(pub u8);

impl PatchTempo {
    pub fn new(value: u8) -> Self {
        PatchTempo(value)
    }
    pub fn raw(self) -> u8 {
        self.0
    }
    fn from_two_bytes(hi: u8, lo: u8) -> Option<Self> {
        decode_nibble_pair(hi, lo).map(PatchTempo)
    }
    fn to_two_bytes(self) -> [u8; 2] {
        encode_nibble_pair(self.0)
    }
}

/// One Master Assign slot. The GR-55 has 8 of these (`PatchArea::master_assigns`).
///
/// Each slot is a 19-byte block that binds an external source (pedal /
/// switch / MIDI CC) to a patch target with optional min/max, range, and
/// internal-pedal envelope. Target / Min / Max each occupy three
/// consecutive bytes whose semantics aren't fully captured by FloorBoard
/// `midi.xml` (only the first byte of each triple has a `customdesc`); the
/// other two are kept as `Option<u8>` companion bytes so the round-trip is
/// lossless.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assign {
    /// On/Off at offset +0x00.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_off: Option<OnOff>,
    /// Target (3 bytes at +0x01..+0x03; only the first has `customdesc`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_c: Option<u8>,
    /// Min (3 bytes at +0x04..+0x06).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_c: Option<u8>,
    /// Max (3 bytes at +0x07..+0x09).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_c: Option<u8>,
    /// Source at +0x0A.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<AssignSource>,
    /// Source mode at +0x0B.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_mode: Option<AssignSourceMode>,
    /// Range Low at +0x0C (raw 0..=126).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_low: Option<u8>,
    /// Range High at +0x0D (raw 1..=127).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_high: Option<u8>,
    /// Internal pedal trigger at +0x0E.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_trigger: Option<AssignInternalTrigger>,
    /// Internal pedal time at +0x0F (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub int_pdl_time: Option<u8>,
    /// Internal pedal curve at +0x10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub int_pdl_curve: Option<AssignIntPdlCurve>,
    /// Wave Rate at +0x11 — raw 0..=100 OR a named note-value 0x65..=0x71.
    /// Kept as `Option<u8>` for now; range-guarded to 0x00..=0x71.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave_rate: Option<u8>,
    /// Wave form at +0x12.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave_form: Option<AssignWaveForm>,
}

impl Assign {
    /// Store a single byte into the slot identified by `byte_in_assign` (an
    /// offset 0..=18 relative to the assign's first address). Returns false
    /// if the typed decode rejects the byte (out of range, invalid enum
    /// variant, etc.) so the caller can route it to `unknown_bytes`.
    fn store_byte(&mut self, byte_in_assign: u8, b: u8) -> bool {
        match byte_in_assign {
            0x00 => match OnOff::from_byte(b) {
                Some(v) => {
                    self.on_off = Some(v);
                    true
                }
                None => false,
            },
            0x01 if b <= 0x0F => {
                self.target = Some(b);
                true
            }
            0x02 if b <= 0x0F => {
                self.target_b = Some(b);
                true
            }
            0x03 if b <= 0x0F => {
                self.target_c = Some(b);
                true
            }
            0x04 if b <= 0x0F => {
                self.min = Some(b);
                true
            }
            0x05 if b <= 0x0F => {
                self.min_b = Some(b);
                true
            }
            0x06 if b <= 0x0F => {
                self.min_c = Some(b);
                true
            }
            0x07 if b <= 0x0F => {
                self.max = Some(b);
                true
            }
            0x08 if b <= 0x0F => {
                self.max_b = Some(b);
                true
            }
            0x09 if b <= 0x0F => {
                self.max_c = Some(b);
                true
            }
            0x0A => match AssignSource::from_byte(b) {
                Some(v) => {
                    self.source = Some(v);
                    true
                }
                None => false,
            },
            0x0B => match AssignSourceMode::from_byte(b) {
                Some(v) => {
                    self.source_mode = Some(v);
                    true
                }
                None => false,
            },
            0x0C if b <= 126 => {
                self.range_low = Some(b);
                true
            }
            0x0D if (1..=127).contains(&b) => {
                self.range_high = Some(b);
                true
            }
            0x0E => match AssignInternalTrigger::from_byte(b) {
                Some(v) => {
                    self.internal_trigger = Some(v);
                    true
                }
                None => false,
            },
            0x0F if b <= 100 => {
                self.int_pdl_time = Some(b);
                true
            }
            0x10 => match AssignIntPdlCurve::from_byte(b) {
                Some(v) => {
                    self.int_pdl_curve = Some(v);
                    true
                }
                None => false,
            },
            0x11 if b <= 0x71 => {
                self.wave_rate = Some(b);
                true
            }
            0x12 => match AssignWaveForm::from_byte(b) {
                Some(v) => {
                    self.wave_form = Some(v);
                    true
                }
                None => false,
            },
            _ => false,
        }
    }

    /// Append this assign's bytes to the encode-side map. `idx` is the
    /// 0..=7 slot index; addresses are computed via `assign_address`,
    /// which handles the page 0x01 → 0x02 boundary for Assigns 7 and 8.
    fn emit_bytes(&self, bytes: &mut BTreeMap<[u8; 4], u8>, base_msb: u8, idx: usize) {
        macro_rules! put {
            ($off:expr, $val:expr) => {
                if let Some(v) = $val {
                    bytes.insert(assign_address(base_msb, idx, $off), v);
                }
            };
        }
        put!(0x00, self.on_off.map(OnOff::to_byte));
        put!(0x01, self.target);
        put!(0x02, self.target_b);
        put!(0x03, self.target_c);
        put!(0x04, self.min);
        put!(0x05, self.min_b);
        put!(0x06, self.min_c);
        put!(0x07, self.max);
        put!(0x08, self.max_b);
        put!(0x09, self.max_c);
        put!(0x0A, self.source.map(AssignSource::to_byte));
        put!(0x0B, self.source_mode.map(AssignSourceMode::to_byte));
        put!(0x0C, self.range_low);
        put!(0x0D, self.range_high);
        put!(0x0E, self.internal_trigger.map(AssignInternalTrigger::to_byte));
        put!(0x0F, self.int_pdl_time);
        put!(0x10, self.int_pdl_curve.map(AssignIntPdlCurve::to_byte));
        put!(0x11, self.wave_rate);
        put!(0x12, self.wave_form.map(AssignWaveForm::to_byte));
    }
}

fn all_assigns_none(arr: &[Option<Assign>; 8]) -> bool {
    arr.iter().all(Option::is_none)
}

fn string_shift_all_none(arr: &[Option<u8>; 6]) -> bool {
    arr.iter().all(Option::is_none)
}

/// Address of byte `byte_in_assign` (0..=18) of slot `idx` (0..=7).
///
/// All 8 Assigns occupy one contiguous 152-byte span. If we picture page
/// `0x01` as the low half of a 256-byte virtual space (offsets 0..=0x7F)
/// and page `0x02` as the high half (0x80..=0xFF), the flat starting
/// offset of Assign N is `0x0C + N*19`, regardless of which physical
/// page that lands on. Assign7 (idx 6) starts at flat offset `0x7E` —
/// the last 2 bytes live on page `0x01`, the remaining 17 wrap to page
/// `0x02` at offsets `0x00..=0x10`. Assign8 (idx 7) starts at flat
/// offset `0x91`, which is page `0x02` offset `0x11`.
fn assign_address(base_msb: u8, idx: usize, byte_in_assign: u8) -> [u8; 4] {
    let flat = 0x0C_u16 + (idx as u16) * 19 + byte_in_assign as u16;
    if flat < 0x80 {
        [base_msb, 0x01, 0x00, flat as u8]
    } else {
        [base_msb, 0x02, 0x00, (flat - 0x80) as u8]
    }
}

/// Inverse of [`assign_address`]: given a decoded frame's (page, hi, lo),
/// return the (assign_index, byte_in_assign) pair that owns it — or
/// `None` if the address falls outside the Master Assign span.
fn assign_locate(page: u8, hi: u8, lo: u8) -> Option<(usize, u8)> {
    if hi != 0 {
        return None;
    }
    let flat = match page {
        0x01 if (0x0C..=0x7F).contains(&lo) => lo as u16,
        0x02 if lo <= 0x23 => lo as u16 + 0x80,
        _ => return None,
    };
    let off = (flat - 0x0C) as usize;
    let idx = off / 19;
    if idx >= 8 {
        return None;
    }
    Some((idx, (off % 19) as u8))
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

    /// The 8 Master Assigns. Assigns 1-6 live entirely on page `0x01`
    /// (each is 19 bytes; Assign1 starts at `0x01:00:0C`, Assign2 at
    /// `0x01:00:1F`, ..., Assign6 at `0x01:00:6B`). Assigns 7 and 8 span
    /// the `0x01 → 0x02` page boundary via [`assign_address`].
    #[serde(default, skip_serializing_if = "all_assigns_none")]
    pub master_assigns: [Option<Assign>; 8],

    // ---- Page 0x02 tail (offsets 0x24..=0x47): per-patch metadata ----
    /// GK Set selector at `0x02:00:24`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_set: Option<PatchGkSet>,
    /// Guitar Out routing at `0x02:00:25`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guitar_out: Option<GuitarOut>,
    /// V-Link Pallet selector at `0x02:00:26` (0 = LAST, 1..=32).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_pallet: Option<u8>,
    /// V-Link Clip selector at `0x02:00:27` (0 = LAST, 1..=32).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_clip: Option<u8>,
    /// V-Link Note Clip Change at `0x02:00:28` (0 = OFF, 1..=4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_note_clip_change: Option<u8>,
    /// V-Link EXP control target at `0x02:00:29`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_exp: Option<VLinkControl>,
    /// V-Link EXP ON control target at `0x02:00:2A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_exp_on: Option<VLinkControl>,
    /// V-Link GK VOL control target at `0x02:00:2B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_gk_vol: Option<VLinkControl>,
    /// Patch structure selector at `0x02:00:2C`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structure: Option<PatchStructure>,
    /// Modeling line route at `0x02:00:2D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modeling_line_route: Option<LineRoute>,
    /// AnalogPU line route at `0x02:00:2E`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analog_pu_line_route: Option<LineRoute>,
    /// Patch Level at `0x02:00:30/31` (4-nibble, 0..=200).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_level: Option<PatchLevel>,
    /// AnalogPU Tone Sw at `0x02:00:32` (wire-reversed enum — see
    /// [`AnalogPuToneSw`] docs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analog_pu_tone_sw: Option<AnalogPuToneSw>,
    /// AnalogPU Tone Level at `0x02:00:33` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analog_pu_tone_level: Option<u8>,
    /// Alt Tuning Switch at `0x02:00:34`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_tuning_sw: Option<OnOff>,
    /// Alt Tuning Type at `0x02:00:35`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_tuning_type: Option<AltTuningType>,
    /// Alt Tuning user shift per string at `0x02:00:36..=3B` (strings
    /// 1..=6 = string 1 (H) through string 6 (L)). Wire encoding maps
    /// `0x28..=0x58` to display values `-24..=+24` semitones; stored
    /// raw to mirror the System area's `user_tuning_shift_strings`.
    #[serde(default, skip_serializing_if = "string_shift_all_none")]
    pub alt_tuning_user_shift: [Option<u8>; 6],
    /// Patch Tempo at `0x02:00:3C/3D` (4-nibble).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_tempo: Option<PatchTempo>,
    /// Chorus bypass level at `0x02:00:3E` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bypass_chorus: Option<u8>,
    /// Delay bypass level at `0x02:00:3F`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bypass_delay: Option<u8>,
    /// Reverb bypass level at `0x02:00:40`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bypass_reverb: Option<u8>,
    /// EXP Pedal Modulation MIN at `0x02:00:42` (per-patch duplicate of
    /// the page-0 `exp_mod_min`; reserved for the modulation envelope
    /// the device applies independently of the patch's per-output Mod
    /// PCM1/PCM2 routings). Raw 0..=127.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_mod_min_envelope: Option<u8>,
    /// EXP Pedal Modulation MAX at `0x02:00:43`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_mod_max_envelope: Option<u8>,
    /// EXP Pedal ON Modulation MIN at `0x02:00:44`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_mod_min_envelope: Option<u8>,
    /// EXP Pedal ON Modulation MAX at `0x02:00:45`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_mod_max_envelope: Option<u8>,
    /// GK Volume Modulation MIN at `0x02:00:46`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_mod_min_envelope: Option<u8>,
    /// GK Volume Modulation MAX at `0x02:00:47`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_mod_max_envelope: Option<u8>,

    /// The single MFX slot. Its data spans pages `0x03` (linear
    /// `0..=127`) and `0x04` (linear `128..=255`) — page `0x04`'s
    /// misleading "MFX 2" LSB label notwithstanding, FloorBoard
    /// `midi.xml` places effect parameters disjointly across the two
    /// pages; per [`crate::mfx_params`] the 256-byte block holds 20
    /// effect types' parameter ranges with no overlap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfx: Option<Mfx>,

    // ---- Page 0x06: Chorus / Delay / Reverb / EQ (offsets 0x00..=0x1D) ----
    /// Chorus switch at `0x06:00:00`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chorus_switch: Option<OnOff>,
    /// Chorus algorithm at `0x06:00:01`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chorus_type: Option<ChorusType>,
    /// Chorus rate at `0x06:00:02` (hybrid 0..=100 numeric or
    /// 0x65..=0x71 named note-values, same shape as Assign wave_rate).
    /// Stored as raw u8 with range guard 0x00..=0x71.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chorus_rate: Option<u8>,
    /// Chorus depth at `0x06:00:03` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chorus_depth: Option<u8>,
    /// Chorus level at `0x06:00:04` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chorus_level: Option<u8>,
    /// Delay switch at `0x06:00:05`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_switch: Option<OnOff>,
    /// Delay algorithm at `0x06:00:06`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_type: Option<DelayType>,
    /// Delay time at `0x06:00:07` — only the first of three nibble bytes
    /// has FloorBoard documentation; `_b` and `_c` are the companion
    /// bytes (each 0..=15) kept lossless. Likely a single 12-bit time
    /// value across three nibbles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_time: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_time_b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_time_c: Option<u8>,
    /// Delay feedback at `0x06:00:0A` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_feedback: Option<u8>,
    /// Delay level at `0x06:00:0B` (raw 0..=120).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_level: Option<u8>,
    /// Reverb switch at `0x06:00:0C`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverb_switch: Option<OnOff>,
    /// Reverb algorithm at `0x06:00:0D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverb_type: Option<ReverbType>,
    /// Reverb time at `0x06:00:0E` (raw 0..=99, display 0.1s..=10.0s).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverb_time: Option<u8>,
    /// Reverb high cut at `0x06:00:0F`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverb_high_cut: Option<ReverbHighCut>,
    /// Reverb level at `0x06:00:10` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverb_level: Option<u8>,
    /// Patch EQ switch at `0x06:00:11`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_switch: Option<OnOff>,
    /// Patch EQ Lo Cut at `0x06:00:12`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_lo_cut: Option<PatchEqLoCut>,
    /// Patch EQ Low Gain at `0x06:00:13` (wire 0..=0x28 = -20..=+20 dB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_low_gain: Option<u8>,
    /// Patch EQ Lo Mid Freq at `0x06:00:14`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_lo_mid_freq: Option<PatchEqMidFreq>,
    /// Patch EQ Lo Mid Q at `0x06:00:15`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_lo_mid_q: Option<PatchEqMidQ>,
    /// Patch EQ Low Mid Gain at `0x06:00:16`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_low_mid_gain: Option<u8>,
    /// Patch EQ Hi Mid Freq at `0x06:00:17`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_hi_mid_freq: Option<PatchEqMidFreq>,
    /// Patch EQ Hi Mid Q at `0x06:00:18`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_hi_mid_q: Option<PatchEqMidQ>,
    /// Patch EQ Hi Mid Gain at `0x06:00:19`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_hi_mid_gain: Option<u8>,
    /// Patch EQ High Cut at `0x06:00:1A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_high_cut: Option<PatchEqHighCut>,
    /// Patch EQ High Gain at `0x06:00:1B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_high_gain: Option<u8>,
    /// Patch EQ Level at `0x06:00:1C`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_level: Option<u8>,
    /// Patch EQ Character at `0x06:00:1D` (-3..=+3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_eq_character: Option<PatchEqCharacter>,

    // ---- Page 0x07: PreAmp/Speaker (0x00..=0x10) + MOD header (0x11..=0x17) ----
    // The MOD-type-dependent tail at 0x18..=0x7F falls through to
    // unknown_bytes pending sum-type modelling, like the MFX tail.
    /// PreAmp switch at `0x07:00:00`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_switch: Option<OnOff>,
    /// PreAmp model at `0x07:00:01`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_type: Option<PreampType>,
    /// PreAmp Gain at `0x07:00:02` (raw 0..=120).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_gain: Option<u8>,
    /// PreAmp Level at `0x07:00:03` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_level: Option<u8>,
    /// PreAmp Gain switch at `0x07:00:04`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_gain_sw: Option<PreampGainSw>,
    /// PreAmp Solo switch at `0x07:00:05`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_solo_sw: Option<OnOff>,
    /// PreAmp Solo Level at `0x07:00:06` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_solo_level: Option<u8>,
    /// PreAmp Bass at `0x07:00:07` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_bass: Option<u8>,
    /// PreAmp Middle at `0x07:00:08`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_middle: Option<u8>,
    /// PreAmp Treble at `0x07:00:09`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_treble: Option<u8>,
    /// PreAmp Presence at `0x07:00:0A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_presence: Option<u8>,
    /// PreAmp Bright switch at `0x07:00:0B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp_bright_sw: Option<OnOff>,
    /// Speaker simulator cabinet at `0x07:00:0C`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_type: Option<SpeakerType>,
    /// Microphone model at `0x07:00:0D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mic_type: Option<MicType>,
    /// Microphone distance at `0x07:00:0E`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mic_distance: Option<MicDistance>,
    /// Microphone position at `0x07:00:0F` (raw 0 = Center, 1..=10 = 1..=10cm).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mic_position: Option<u8>,
    /// Microphone level at `0x07:00:10` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mic_level: Option<u8>,
    /// The single MOD slot. Holds the typed common header at page `0x07`
    /// offsets `0x11..=0x17` (sends, switch, type, pan) plus a
    /// per-effect-type tail at `0x18..=0x59` parked in `Mod::raw_tail`
    /// keyed by offset. Per [`crate::mod_params`] the 14 MOD effect
    /// types own disjoint byte ranges within that tail, so the table is
    /// a build-time-verified parameter label/owner lookup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modulation: Option<Mod>,
    /// Noise Suppressor switch at `0x07:00:5A`. NS lives on the same
    /// page as PreAmp + MOD but is a separate sub-effect, so it's
    /// modelled as flat fields here rather than nested inside `Mod`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns_switch: Option<OnOff>,
    /// NS Threshold at `0x07:00:5B` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns_threshold: Option<u8>,
    /// NS Release at `0x07:00:5C` (raw 0..=100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns_release: Option<u8>,

    /// The single Modeling slot. Holds the 30 typed common-header bytes
    /// at page `0x10` offsets `0x00..=0x1D` plus a 226-byte type-specific
    /// tail (page `0x10` `0x1E..=0x7F` + all of page `0x11`) parked in
    /// `Modeling::raw_tail` keyed by linear offset. Per
    /// [`crate::modeling_params`] the tail's ownership follows a 2-axis
    /// taxonomy (mode-by-page, category-by-`abbr`, type-by-`desc`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modeling: Option<Modeling>,

    /// The 2 PCM tones. Each tone's data spans two MIDI pages:
    ///
    /// - `pcm[0]` = PCM Tone 1: header on page `0x20`, tone-shaping
    ///   tail (Filter / TVF / TVA / Pitch Env / LFO1 / LFO2) on page
    ///   `0x30`.
    /// - `pcm[1]` = PCM Tone 2: header on `0x21`, tail on `0x31`.
    ///
    /// FloorBoard `midi.xml` labels pages `0x30` / `0x31` as
    /// "PCM-1-B" / "PCM-2-B" which suggested four independent slots —
    /// but FloorBoard's C++ source (`soundSource_synth_a.cpp` /
    /// `_b.cpp`) confirms each tone's editor binds both pages
    /// together. The MIDI memory layout is two tones × two pages, not
    /// four independent slots.
    #[serde(default, skip_serializing_if = "all_pcm_none")]
    pub pcm: [Option<Pcm>; 2],

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
        // Post-process 4-nibble pairs (PatchLevel + PatchTempo). These can
        // only be combined once both bytes are seen, so they're parked in
        // `unknown_bytes` during the byte-by-byte pass and lifted out here.
        area.lift_nibble_pair(
            "02:00:30",
            "02:00:31",
            PatchLevel::from_two_bytes,
            |area, v| area.patch_level = Some(v),
        );
        area.lift_nibble_pair(
            "02:00:3C",
            "02:00:3D",
            PatchTempo::from_two_bytes,
            |area, v| area.patch_tempo = Some(v),
        );
        // Lift the 2-byte (bank, position) PCM tone selector in each
        // populated slot from `raw_tail` into the typed `synth_tone`
        // field.
        for slot in area.pcm.iter_mut().flatten() {
            slot.finalize();
        }
        area
    }

    /// If both `hi_key` and `lo_key` are present in `unknown_bytes` and
    /// `decode(hi, lo)` succeeds, remove them and apply the result via
    /// `apply`. Used to lift 4-nibble pairs into their typed fields after
    /// the byte-by-byte decoder finishes.
    fn lift_nibble_pair<T>(
        &mut self,
        hi_key: &str,
        lo_key: &str,
        decode: impl FnOnce(u8, u8) -> Option<T>,
        apply: impl FnOnce(&mut Self, T),
    ) {
        let Some(&hi) = self.unknown_bytes.get(hi_key) else {
            return;
        };
        let Some(&lo) = self.unknown_bytes.get(lo_key) else {
            return;
        };
        let Some(value) = decode(hi, lo) else {
            return;
        };
        self.unknown_bytes.remove(hi_key);
        self.unknown_bytes.remove(lo_key);
        apply(self, value);
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
            // Master Assigns 1-8 (page 0x01 offset 0x0C through page 0x02 offset 0x23).
            // The 8 assigns occupy one contiguous 152-byte span; assign_locate
            // maps any (page, hi, lo) inside that span to (assign_index, byte_in_assign).
            (_, 0x00, _) if assign_locate(page, hi, lo).is_some() => {
                let (idx, bia) = assign_locate(page, hi, lo).unwrap();
                let assign = self.master_assigns[idx].get_or_insert_with(Assign::default);
                if !assign.store_byte(bia, b) {
                    self.unknown_bytes.insert(format_key(page, hi, lo), b);
                }
            }
            // Page 0x02 tail (per-patch metadata at 0x24..=0x47).
            (0x02, 0x00, 0x24) => self.gk_set = PatchGkSet::from_byte(b),
            (0x02, 0x00, 0x25) => self.guitar_out = GuitarOut::from_byte(b),
            (0x02, 0x00, 0x26) if b <= 32 => self.v_link_pallet = Some(b),
            (0x02, 0x00, 0x27) if b <= 32 => self.v_link_clip = Some(b),
            (0x02, 0x00, 0x28) if b <= 4 => self.v_link_note_clip_change = Some(b),
            (0x02, 0x00, 0x29) => self.v_link_exp = VLinkControl::from_byte(b),
            (0x02, 0x00, 0x2A) => self.v_link_exp_on = VLinkControl::from_byte(b),
            (0x02, 0x00, 0x2B) => self.v_link_gk_vol = VLinkControl::from_byte(b),
            (0x02, 0x00, 0x2C) => self.structure = PatchStructure::from_byte(b),
            (0x02, 0x00, 0x2D) => self.modeling_line_route = LineRoute::from_byte(b),
            (0x02, 0x00, 0x2E) => self.analog_pu_line_route = LineRoute::from_byte(b),
            // 0x2F is FloorBoard-undocumented (no customdesc, range 0..=255)
            // — fall through to unknown_bytes.
            // 0x30/0x31 are decoded after the loop in `from_frames_at` since
            // they require both bytes present together.
            (0x02, 0x00, 0x32) => self.analog_pu_tone_sw = AnalogPuToneSw::from_byte(b),
            (0x02, 0x00, 0x33) if b <= 100 => self.analog_pu_tone_level = Some(b),
            (0x02, 0x00, 0x34) => self.alt_tuning_sw = OnOff::from_byte(b),
            (0x02, 0x00, 0x35) => self.alt_tuning_type = AltTuningType::from_byte(b),
            (0x02, 0x00, lo @ 0x36..=0x3B) if (0x28..=0x58).contains(&b) => {
                self.alt_tuning_user_shift[(lo - 0x36) as usize] = Some(b);
            }
            // 0x3C/0x3D decoded as PatchTempo after the loop.
            (0x02, 0x00, 0x3E) if b <= 100 => self.bypass_chorus = Some(b),
            (0x02, 0x00, 0x3F) if b <= 100 => self.bypass_delay = Some(b),
            (0x02, 0x00, 0x40) if b <= 100 => self.bypass_reverb = Some(b),
            (0x02, 0x00, 0x42) if b <= 127 => self.exp_mod_min_envelope = Some(b),
            (0x02, 0x00, 0x43) if b <= 127 => self.exp_mod_max_envelope = Some(b),
            (0x02, 0x00, 0x44) if b <= 127 => self.exp_on_mod_min_envelope = Some(b),
            (0x02, 0x00, 0x45) if b <= 127 => self.exp_on_mod_max_envelope = Some(b),
            (0x02, 0x00, 0x46) if b <= 127 => self.gk_vol_mod_min_envelope = Some(b),
            (0x02, 0x00, 0x47) if b <= 127 => self.gk_vol_mod_max_envelope = Some(b),
            // Single MFX slot — pages 0x03 (linear 0..=127) and 0x04
            // (linear 128..=255) feed into the same buffer.
            (0x03, 0x00, off) => {
                let linear = off as u16;
                let mfx = self.mfx.get_or_insert_with(Mfx::default);
                if !mfx.store_byte(linear, b) {
                    self.unknown_bytes.insert(format_key(page, hi, lo), b);
                }
            }
            (0x04, 0x00, off) => {
                let linear = 128 + off as u16;
                let mfx = self.mfx.get_or_insert_with(Mfx::default);
                if !mfx.store_byte(linear, b) {
                    self.unknown_bytes.insert(format_key(page, hi, lo), b);
                }
            }
            // Page 0x06: Chorus / Delay / Reverb / EQ.
            (0x06, 0x00, 0x00) => self.chorus_switch = OnOff::from_byte(b),
            (0x06, 0x00, 0x01) => self.chorus_type = ChorusType::from_byte(b),
            (0x06, 0x00, 0x02) if b <= 0x71 => self.chorus_rate = Some(b),
            (0x06, 0x00, 0x03) if b <= 100 => self.chorus_depth = Some(b),
            (0x06, 0x00, 0x04) if b <= 100 => self.chorus_level = Some(b),
            (0x06, 0x00, 0x05) => self.delay_switch = OnOff::from_byte(b),
            (0x06, 0x00, 0x06) => self.delay_type = DelayType::from_byte(b),
            (0x06, 0x00, 0x07) if b <= 0x0F => self.delay_time = Some(b),
            (0x06, 0x00, 0x08) if b <= 0x0F => self.delay_time_b = Some(b),
            (0x06, 0x00, 0x09) if b <= 0x0F => self.delay_time_c = Some(b),
            (0x06, 0x00, 0x0A) if b <= 100 => self.delay_feedback = Some(b),
            (0x06, 0x00, 0x0B) if b <= 120 => self.delay_level = Some(b),
            (0x06, 0x00, 0x0C) => self.reverb_switch = OnOff::from_byte(b),
            (0x06, 0x00, 0x0D) => self.reverb_type = ReverbType::from_byte(b),
            (0x06, 0x00, 0x0E) if b <= 99 => self.reverb_time = Some(b),
            (0x06, 0x00, 0x0F) => self.reverb_high_cut = ReverbHighCut::from_byte(b),
            (0x06, 0x00, 0x10) if b <= 100 => self.reverb_level = Some(b),
            (0x06, 0x00, 0x11) => self.patch_eq_switch = OnOff::from_byte(b),
            (0x06, 0x00, 0x12) => self.patch_eq_lo_cut = PatchEqLoCut::from_byte(b),
            (0x06, 0x00, 0x13) if b <= 0x28 => self.patch_eq_low_gain = Some(b),
            (0x06, 0x00, 0x14) => self.patch_eq_lo_mid_freq = PatchEqMidFreq::from_byte(b),
            (0x06, 0x00, 0x15) => self.patch_eq_lo_mid_q = PatchEqMidQ::from_byte(b),
            (0x06, 0x00, 0x16) if b <= 0x28 => self.patch_eq_low_mid_gain = Some(b),
            (0x06, 0x00, 0x17) => self.patch_eq_hi_mid_freq = PatchEqMidFreq::from_byte(b),
            (0x06, 0x00, 0x18) => self.patch_eq_hi_mid_q = PatchEqMidQ::from_byte(b),
            (0x06, 0x00, 0x19) if b <= 0x28 => self.patch_eq_hi_mid_gain = Some(b),
            (0x06, 0x00, 0x1A) => self.patch_eq_high_cut = PatchEqHighCut::from_byte(b),
            (0x06, 0x00, 0x1B) if b <= 0x28 => self.patch_eq_high_gain = Some(b),
            (0x06, 0x00, 0x1C) if b <= 0x28 => self.patch_eq_level = Some(b),
            (0x06, 0x00, 0x1D) => self.patch_eq_character = PatchEqCharacter::from_byte(b),
            // Page 0x07: PreAmp/Speaker + MOD header.
            (0x07, 0x00, 0x00) => self.preamp_switch = OnOff::from_byte(b),
            (0x07, 0x00, 0x01) => self.preamp_type = PreampType::from_byte(b),
            (0x07, 0x00, 0x02) if b <= 120 => self.preamp_gain = Some(b),
            (0x07, 0x00, 0x03) if b <= 100 => self.preamp_level = Some(b),
            (0x07, 0x00, 0x04) => self.preamp_gain_sw = PreampGainSw::from_byte(b),
            (0x07, 0x00, 0x05) => self.preamp_solo_sw = OnOff::from_byte(b),
            (0x07, 0x00, 0x06) if b <= 100 => self.preamp_solo_level = Some(b),
            (0x07, 0x00, 0x07) if b <= 100 => self.preamp_bass = Some(b),
            (0x07, 0x00, 0x08) if b <= 100 => self.preamp_middle = Some(b),
            (0x07, 0x00, 0x09) if b <= 100 => self.preamp_treble = Some(b),
            (0x07, 0x00, 0x0A) if b <= 100 => self.preamp_presence = Some(b),
            (0x07, 0x00, 0x0B) => self.preamp_bright_sw = OnOff::from_byte(b),
            (0x07, 0x00, 0x0C) => self.speaker_type = SpeakerType::from_byte(b),
            (0x07, 0x00, 0x0D) => self.mic_type = MicType::from_byte(b),
            (0x07, 0x00, 0x0E) => self.mic_distance = MicDistance::from_byte(b),
            (0x07, 0x00, 0x0F) if b <= 10 => self.mic_position = Some(b),
            (0x07, 0x00, 0x10) if b <= 100 => self.mic_level = Some(b),
            (0x07, 0x00, off @ 0x11..=0x59) => {
                let modu = self.modulation.get_or_insert_with(Mod::default);
                if !modu.store_byte(off, b) {
                    self.unknown_bytes.insert(format_key(page, hi, lo), b);
                }
            }
            (0x07, 0x00, 0x5A) => self.ns_switch = OnOff::from_byte(b),
            (0x07, 0x00, 0x5B) if b <= 100 => self.ns_threshold = Some(b),
            (0x07, 0x00, 0x5C) if b <= 100 => self.ns_release = Some(b),
            // Page 0x10: Modeling common header.
            // Modeling slot — pages 0x10 (linear 0..=127) and 0x11
            // (linear 128..=255) feed into the same buffer.
            (0x10, 0x00, off) => {
                let linear = off as u16;
                let modeling = self.modeling.get_or_insert_with(Modeling::default);
                if !modeling.store_byte(linear, b) {
                    self.unknown_bytes.insert(format_key(page, hi, lo), b);
                }
            }
            (0x11, 0x00, off) => {
                let linear = 128 + off as u16;
                let modeling = self.modeling.get_or_insert_with(Modeling::default);
                if !modeling.store_byte(linear, b) {
                    self.unknown_bytes.insert(format_key(page, hi, lo), b);
                }
            }
            // PCM tone slots — pages 0x20/0x21 carry the header bytes
            // for Tone 1/Tone 2, pages 0x30/0x31 carry their tail bytes.
            (_, 0x00, off) if pcm_route_for_page(page).is_some() => {
                let (idx, accepted) = match pcm_route_for_page(page).unwrap() {
                    PcmPageRole::Header(idx) => {
                        let pcm = self.pcm[idx].get_or_insert_with(Pcm::default);
                        (idx, pcm.store_header_byte(off, b))
                    }
                    PcmPageRole::Tail(idx) => {
                        let pcm = self.pcm[idx].get_or_insert_with(Pcm::default);
                        (idx, pcm.store_tail_byte(off, b))
                    }
                };
                if !accepted {
                    self.unknown_bytes.insert(format_key(page, hi, lo), b);
                }
                let _ = idx;
            }
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
        // Master Assigns 1-8
        for (idx, slot) in self.master_assigns.iter().enumerate() {
            if let Some(assign) = slot {
                assign.emit_bytes(&mut bytes, base_msb, idx);
            }
        }
        // Page 0x02 tail
        if let Some(v) = self.gk_set {
            bytes.insert([base_msb, 0x02, 0x00, 0x24], v.to_byte());
        }
        if let Some(v) = self.guitar_out {
            bytes.insert([base_msb, 0x02, 0x00, 0x25], v.to_byte());
        }
        if let Some(v) = self.v_link_pallet {
            bytes.insert([base_msb, 0x02, 0x00, 0x26], v);
        }
        if let Some(v) = self.v_link_clip {
            bytes.insert([base_msb, 0x02, 0x00, 0x27], v);
        }
        if let Some(v) = self.v_link_note_clip_change {
            bytes.insert([base_msb, 0x02, 0x00, 0x28], v);
        }
        if let Some(v) = self.v_link_exp {
            bytes.insert([base_msb, 0x02, 0x00, 0x29], v.to_byte());
        }
        if let Some(v) = self.v_link_exp_on {
            bytes.insert([base_msb, 0x02, 0x00, 0x2A], v.to_byte());
        }
        if let Some(v) = self.v_link_gk_vol {
            bytes.insert([base_msb, 0x02, 0x00, 0x2B], v.to_byte());
        }
        if let Some(v) = self.structure {
            bytes.insert([base_msb, 0x02, 0x00, 0x2C], v.to_byte());
        }
        if let Some(v) = self.modeling_line_route {
            bytes.insert([base_msb, 0x02, 0x00, 0x2D], v.to_byte());
        }
        if let Some(v) = self.analog_pu_line_route {
            bytes.insert([base_msb, 0x02, 0x00, 0x2E], v.to_byte());
        }
        if let Some(v) = self.patch_level {
            let [hi, lo] = v.to_two_bytes();
            bytes.insert([base_msb, 0x02, 0x00, 0x30], hi);
            bytes.insert([base_msb, 0x02, 0x00, 0x31], lo);
        }
        if let Some(v) = self.analog_pu_tone_sw {
            bytes.insert([base_msb, 0x02, 0x00, 0x32], v.to_byte());
        }
        if let Some(v) = self.analog_pu_tone_level {
            bytes.insert([base_msb, 0x02, 0x00, 0x33], v);
        }
        if let Some(v) = self.alt_tuning_sw {
            bytes.insert([base_msb, 0x02, 0x00, 0x34], v.to_byte());
        }
        if let Some(v) = self.alt_tuning_type {
            bytes.insert([base_msb, 0x02, 0x00, 0x35], v.to_byte());
        }
        for (i, v) in self.alt_tuning_user_shift.iter().enumerate() {
            if let Some(b) = v {
                bytes.insert([base_msb, 0x02, 0x00, 0x36 + i as u8], *b);
            }
        }
        if let Some(v) = self.patch_tempo {
            let [hi, lo] = v.to_two_bytes();
            bytes.insert([base_msb, 0x02, 0x00, 0x3C], hi);
            bytes.insert([base_msb, 0x02, 0x00, 0x3D], lo);
        }
        if let Some(v) = self.bypass_chorus {
            bytes.insert([base_msb, 0x02, 0x00, 0x3E], v);
        }
        if let Some(v) = self.bypass_delay {
            bytes.insert([base_msb, 0x02, 0x00, 0x3F], v);
        }
        if let Some(v) = self.bypass_reverb {
            bytes.insert([base_msb, 0x02, 0x00, 0x40], v);
        }
        if let Some(v) = self.exp_mod_min_envelope {
            bytes.insert([base_msb, 0x02, 0x00, 0x42], v);
        }
        if let Some(v) = self.exp_mod_max_envelope {
            bytes.insert([base_msb, 0x02, 0x00, 0x43], v);
        }
        if let Some(v) = self.exp_on_mod_min_envelope {
            bytes.insert([base_msb, 0x02, 0x00, 0x44], v);
        }
        if let Some(v) = self.exp_on_mod_max_envelope {
            bytes.insert([base_msb, 0x02, 0x00, 0x45], v);
        }
        if let Some(v) = self.gk_vol_mod_min_envelope {
            bytes.insert([base_msb, 0x02, 0x00, 0x46], v);
        }
        if let Some(v) = self.gk_vol_mod_max_envelope {
            bytes.insert([base_msb, 0x02, 0x00, 0x47], v);
        }
        // Single MFX slot — emits bytes to both page 0x03 and 0x04.
        if let Some(mfx) = &self.mfx {
            mfx.emit_bytes(&mut bytes, base_msb);
        }
        // Page 0x06
        if let Some(v) = self.chorus_switch {
            bytes.insert([base_msb, 0x06, 0x00, 0x00], v.to_byte());
        }
        if let Some(v) = self.chorus_type {
            bytes.insert([base_msb, 0x06, 0x00, 0x01], v.to_byte());
        }
        if let Some(v) = self.chorus_rate {
            bytes.insert([base_msb, 0x06, 0x00, 0x02], v);
        }
        if let Some(v) = self.chorus_depth {
            bytes.insert([base_msb, 0x06, 0x00, 0x03], v);
        }
        if let Some(v) = self.chorus_level {
            bytes.insert([base_msb, 0x06, 0x00, 0x04], v);
        }
        if let Some(v) = self.delay_switch {
            bytes.insert([base_msb, 0x06, 0x00, 0x05], v.to_byte());
        }
        if let Some(v) = self.delay_type {
            bytes.insert([base_msb, 0x06, 0x00, 0x06], v.to_byte());
        }
        if let Some(v) = self.delay_time {
            bytes.insert([base_msb, 0x06, 0x00, 0x07], v);
        }
        if let Some(v) = self.delay_time_b {
            bytes.insert([base_msb, 0x06, 0x00, 0x08], v);
        }
        if let Some(v) = self.delay_time_c {
            bytes.insert([base_msb, 0x06, 0x00, 0x09], v);
        }
        if let Some(v) = self.delay_feedback {
            bytes.insert([base_msb, 0x06, 0x00, 0x0A], v);
        }
        if let Some(v) = self.delay_level {
            bytes.insert([base_msb, 0x06, 0x00, 0x0B], v);
        }
        if let Some(v) = self.reverb_switch {
            bytes.insert([base_msb, 0x06, 0x00, 0x0C], v.to_byte());
        }
        if let Some(v) = self.reverb_type {
            bytes.insert([base_msb, 0x06, 0x00, 0x0D], v.to_byte());
        }
        if let Some(v) = self.reverb_time {
            bytes.insert([base_msb, 0x06, 0x00, 0x0E], v);
        }
        if let Some(v) = self.reverb_high_cut {
            bytes.insert([base_msb, 0x06, 0x00, 0x0F], v.to_byte());
        }
        if let Some(v) = self.reverb_level {
            bytes.insert([base_msb, 0x06, 0x00, 0x10], v);
        }
        if let Some(v) = self.patch_eq_switch {
            bytes.insert([base_msb, 0x06, 0x00, 0x11], v.to_byte());
        }
        if let Some(v) = self.patch_eq_lo_cut {
            bytes.insert([base_msb, 0x06, 0x00, 0x12], v.to_byte());
        }
        if let Some(v) = self.patch_eq_low_gain {
            bytes.insert([base_msb, 0x06, 0x00, 0x13], v);
        }
        if let Some(v) = self.patch_eq_lo_mid_freq {
            bytes.insert([base_msb, 0x06, 0x00, 0x14], v.to_byte());
        }
        if let Some(v) = self.patch_eq_lo_mid_q {
            bytes.insert([base_msb, 0x06, 0x00, 0x15], v.to_byte());
        }
        if let Some(v) = self.patch_eq_low_mid_gain {
            bytes.insert([base_msb, 0x06, 0x00, 0x16], v);
        }
        if let Some(v) = self.patch_eq_hi_mid_freq {
            bytes.insert([base_msb, 0x06, 0x00, 0x17], v.to_byte());
        }
        if let Some(v) = self.patch_eq_hi_mid_q {
            bytes.insert([base_msb, 0x06, 0x00, 0x18], v.to_byte());
        }
        if let Some(v) = self.patch_eq_hi_mid_gain {
            bytes.insert([base_msb, 0x06, 0x00, 0x19], v);
        }
        if let Some(v) = self.patch_eq_high_cut {
            bytes.insert([base_msb, 0x06, 0x00, 0x1A], v.to_byte());
        }
        if let Some(v) = self.patch_eq_high_gain {
            bytes.insert([base_msb, 0x06, 0x00, 0x1B], v);
        }
        if let Some(v) = self.patch_eq_level {
            bytes.insert([base_msb, 0x06, 0x00, 0x1C], v);
        }
        if let Some(v) = self.patch_eq_character {
            bytes.insert([base_msb, 0x06, 0x00, 0x1D], v.to_byte());
        }
        // Page 0x07
        if let Some(v) = self.preamp_switch {
            bytes.insert([base_msb, 0x07, 0x00, 0x00], v.to_byte());
        }
        if let Some(v) = self.preamp_type {
            bytes.insert([base_msb, 0x07, 0x00, 0x01], v.to_byte());
        }
        if let Some(v) = self.preamp_gain {
            bytes.insert([base_msb, 0x07, 0x00, 0x02], v);
        }
        if let Some(v) = self.preamp_level {
            bytes.insert([base_msb, 0x07, 0x00, 0x03], v);
        }
        if let Some(v) = self.preamp_gain_sw {
            bytes.insert([base_msb, 0x07, 0x00, 0x04], v.to_byte());
        }
        if let Some(v) = self.preamp_solo_sw {
            bytes.insert([base_msb, 0x07, 0x00, 0x05], v.to_byte());
        }
        if let Some(v) = self.preamp_solo_level {
            bytes.insert([base_msb, 0x07, 0x00, 0x06], v);
        }
        if let Some(v) = self.preamp_bass {
            bytes.insert([base_msb, 0x07, 0x00, 0x07], v);
        }
        if let Some(v) = self.preamp_middle {
            bytes.insert([base_msb, 0x07, 0x00, 0x08], v);
        }
        if let Some(v) = self.preamp_treble {
            bytes.insert([base_msb, 0x07, 0x00, 0x09], v);
        }
        if let Some(v) = self.preamp_presence {
            bytes.insert([base_msb, 0x07, 0x00, 0x0A], v);
        }
        if let Some(v) = self.preamp_bright_sw {
            bytes.insert([base_msb, 0x07, 0x00, 0x0B], v.to_byte());
        }
        if let Some(v) = self.speaker_type {
            bytes.insert([base_msb, 0x07, 0x00, 0x0C], v.to_byte());
        }
        if let Some(v) = self.mic_type {
            bytes.insert([base_msb, 0x07, 0x00, 0x0D], v.to_byte());
        }
        if let Some(v) = self.mic_distance {
            bytes.insert([base_msb, 0x07, 0x00, 0x0E], v.to_byte());
        }
        if let Some(v) = self.mic_position {
            bytes.insert([base_msb, 0x07, 0x00, 0x0F], v);
        }
        if let Some(v) = self.mic_level {
            bytes.insert([base_msb, 0x07, 0x00, 0x10], v);
        }
        if let Some(modu) = &self.modulation {
            modu.emit_bytes(&mut bytes, base_msb);
        }
        if let Some(v) = self.ns_switch {
            bytes.insert([base_msb, 0x07, 0x00, 0x5A], v.to_byte());
        }
        if let Some(v) = self.ns_threshold {
            bytes.insert([base_msb, 0x07, 0x00, 0x5B], v);
        }
        if let Some(v) = self.ns_release {
            bytes.insert([base_msb, 0x07, 0x00, 0x5C], v);
        }
        // Single Modeling slot — emits bytes to both page 0x10 and 0x11.
        if let Some(modeling) = &self.modeling {
            modeling.emit_bytes(&mut bytes, base_msb);
        }
        // PCM tones — each slot emits to two pages (header + tail).
        for (idx, slot) in self.pcm.iter().enumerate() {
            if let Some(pcm) = slot {
                pcm.emit_bytes(&mut bytes, base_msb, idx);
            }
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
        // Use page 0x40 — well above any typed page — so the bytes fall
        // through to unknown_bytes where we can inspect them.
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x40, 0x00, 0x7E],
            data: Cow::Owned(vec![0xA1, 0xA2, 0xA3]),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.unknown_bytes.get("40:00:7E"), Some(&0xA1));
        assert_eq!(area.unknown_bytes.get("40:00:7F"), Some(&0xA2));
        assert_eq!(area.unknown_bytes.get("40:01:00"), Some(&0xA3));
        // The wrong (overflow-to-0x80) behaviour would surface here:
        assert!(!area.unknown_bytes.contains_key("40:00:80"));
    }

    #[test]
    fn master_assign_1_decodes_typed_fields() {
        let payload: Vec<u8> = vec![
            OnOff::On.to_byte(),                            // +0x00 on_off
            0x05,                                           // +0x01 target
            0x00,                                           // +0x02 target_b
            0x00,                                           // +0x03 target_c
            0x00,                                           // +0x04 min
            0x00,                                           // +0x05 min_b
            0x00,                                           // +0x06 min_c
            0x0F,                                           // +0x07 max
            0x00,                                           // +0x08 max_b
            0x00,                                           // +0x09 max_c
            AssignSource::ExpPdl.to_byte(),                 // +0x0A source
            AssignSourceMode::Toggle.to_byte(),             // +0x0B source_mode
            0x00,                                           // +0x0C range_low
            0x7F,                                           // +0x0D range_high
            AssignInternalTrigger::PatchChange.to_byte(),   // +0x0E internal_trigger
            0x50,                                           // +0x0F int_pdl_time
            AssignIntPdlCurve::SlowRise.to_byte(),          // +0x10 int_pdl_curve
            0x6B,                                           // +0x11 wave_rate (= quarter note)
            AssignWaveForm::Triangle.to_byte(),             // +0x12 wave_form
        ];
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x01, 0x00, 0x0C], // Assign1 base
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);

        let a1 = area.master_assigns[0]
            .as_ref()
            .expect("Assign1 should decode");
        assert_eq!(a1.on_off, Some(OnOff::On));
        assert_eq!(a1.target, Some(0x05));
        assert_eq!(a1.max, Some(0x0F));
        assert_eq!(a1.source, Some(AssignSource::ExpPdl));
        assert_eq!(a1.source_mode, Some(AssignSourceMode::Toggle));
        assert_eq!(a1.range_low, Some(0x00));
        assert_eq!(a1.range_high, Some(0x7F));
        assert_eq!(a1.internal_trigger, Some(AssignInternalTrigger::PatchChange));
        assert_eq!(a1.int_pdl_time, Some(0x50));
        assert_eq!(a1.int_pdl_curve, Some(AssignIntPdlCurve::SlowRise));
        assert_eq!(a1.wave_rate, Some(0x6B));
        assert_eq!(a1.wave_form, Some(AssignWaveForm::Triangle));
        assert!(area.master_assigns[1].is_none());
        assert!(area.unknown_bytes.is_empty());

        // Round-trip preserves every typed byte.
        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn master_assign_7_spans_page_boundary() {
        // Assign7 starts at flat offset 0x0C + 6*19 = 0x7E (page 0x01).
        // First 2 bytes land on page 0x01, remaining 17 wrap to page 0x02
        // at offsets 0x00..=0x10. Send as two DT1 frames matching the
        // natural wire convention.
        let frames = vec![
            Frame::Dt1 {
                device_id: 0x10,
                address: [TEMP_MSB, 0x01, 0x00, 0x7E],
                data: Cow::Owned(vec![OnOff::On.to_byte(), 0x07]), // on_off, target
            },
            Frame::Dt1 {
                device_id: 0x10,
                address: [TEMP_MSB, 0x02, 0x00, 0x00],
                data: Cow::Owned(vec![
                    0x00, 0x00, // target_b, target_c
                    0x02, 0x00, 0x00, // min, min_b, min_c
                    0x0E, 0x00, 0x00, // max, max_b, max_c
                    AssignSource::midi_cc(7).unwrap().to_byte(),  // source = CC#07
                    AssignSourceMode::Moment.to_byte(),
                    0x10, // range_low
                    0x70, // range_high
                    AssignInternalTrigger::CtrlPdl.to_byte(),
                    0x20, // int_pdl_time
                    AssignIntPdlCurve::FastRise.to_byte(),
                    0x68, // wave_rate (half note)
                    AssignWaveForm::Sine.to_byte(),
                ]),
            },
        ];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        let a7 = area.master_assigns[6]
            .as_ref()
            .expect("Assign7 should decode");
        assert_eq!(a7.on_off, Some(OnOff::On));
        assert_eq!(a7.target, Some(0x07));
        assert_eq!(a7.min, Some(0x02));
        assert_eq!(a7.max, Some(0x0E));
        assert_eq!(a7.source, AssignSource::midi_cc(7));
        assert_eq!(a7.source_mode, Some(AssignSourceMode::Moment));
        assert_eq!(a7.range_low, Some(0x10));
        assert_eq!(a7.range_high, Some(0x70));
        assert_eq!(a7.internal_trigger, Some(AssignInternalTrigger::CtrlPdl));
        assert_eq!(a7.int_pdl_time, Some(0x20));
        assert_eq!(a7.int_pdl_curve, Some(AssignIntPdlCurve::FastRise));
        assert_eq!(a7.wave_rate, Some(0x68));
        assert_eq!(a7.wave_form, Some(AssignWaveForm::Sine));
        // No other slots populated.
        for i in [0, 1, 2, 3, 4, 5, 7] {
            assert!(area.master_assigns[i].is_none(), "slot {i} should be None");
        }
        assert!(area.unknown_bytes.is_empty());

        // Round-trip preserves the page split.
        let back_frames = area.to_frames(0x10, TEMP_MSB).unwrap();
        // Verify the encoded addresses really do span both pages.
        let mut saw_page1 = false;
        let mut saw_page2 = false;
        for f in &back_frames {
            if let Frame::Dt1 { address, .. } = f {
                if address[1] == 0x01 && address[3] >= 0x7E {
                    saw_page1 = true;
                }
                if address[1] == 0x02 && address[3] <= 0x10 {
                    saw_page2 = true;
                }
            }
        }
        assert!(saw_page1, "expected encoded bytes on page 0x01 0x7E/0x7F");
        assert!(saw_page2, "expected encoded bytes on page 0x02 0x00..=0x10");

        let round = PatchArea::from_frames_at(&back_frames, TEMP_MSB);
        assert_eq!(round, area);
    }

    #[test]
    fn page_02_tail_round_trips_metadata_block() {
        // Cover 0x24..=0x47 (with two intentional gaps that fall through
        // to unknown_bytes: 0x2F which has no FloorBoard customdesc, and
        // 0x41 which has no PARAM at all).
        let mut payload: Vec<u8> = vec![
            PatchGkSet::User3.to_byte(),       // 0x24
            GuitarOut::Modeling.to_byte(),     // 0x25
            12,                                // 0x26 v_link_pallet
            7,                                 // 0x27 v_link_clip
            3,                                 // 0x28 v_link_note_clip_change
            VLinkControl::ColorCb.to_byte(),   // 0x29
            VLinkControl::Bright.to_byte(),    // 0x2A
            VLinkControl::PlaySpeed.to_byte(), // 0x2B
            PatchStructure::Structure2.to_byte(), // 0x2C
            LineRoute::AmpMod.to_byte(),       // 0x2D
            LineRoute::Mfx.to_byte(),          // 0x2E
            0xAA,                              // 0x2F undocumented placeholder
        ];
        // Patch Level = 75 (0x4B) — nibbles 4 and B → bytes 0x04 0x0B.
        let [pl_hi, pl_lo] = PatchLevel::new(75).unwrap().to_two_bytes();
        payload.push(pl_hi); // 0x30
        payload.push(pl_lo); // 0x31
        payload.push(AnalogPuToneSw::Off.to_byte()); // 0x32 (= 0x01 — wire-reversed!)
        payload.push(80); // 0x33 analog_pu_tone_level
        payload.push(OnOff::On.to_byte()); // 0x34 alt_tuning_sw
        payload.push(AltTuningType::Baritone.to_byte()); // 0x35
        for v in [0x30, 0x32, 0x34, 0x40, 0x4E, 0x50] {
            payload.push(v); // 0x36..=0x3B
        }
        // Patch Tempo = 0x96 (150) → nibbles 9, 6 → bytes 0x09 0x06.
        let [pt_hi, pt_lo] = PatchTempo::new(0x96).to_two_bytes();
        payload.push(pt_hi); // 0x3C
        payload.push(pt_lo); // 0x3D
        payload.push(50); // 0x3E bypass_chorus
        payload.push(60); // 0x3F bypass_delay
        payload.push(70); // 0x40 bypass_reverb
        payload.push(0xBB); // 0x41 untyped placeholder
        payload.push(0x10); // 0x42 exp_mod_min_envelope
        payload.push(0x70); // 0x43 exp_mod_max_envelope
        payload.push(0x05); // 0x44 exp_on_mod_min_envelope
        payload.push(0x65); // 0x45 exp_on_mod_max_envelope
        payload.push(0x20); // 0x46 gk_vol_mod_min_envelope
        payload.push(0x60); // 0x47 gk_vol_mod_max_envelope

        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x02, 0x00, 0x24],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);

        assert_eq!(area.gk_set, Some(PatchGkSet::User3));
        assert_eq!(area.guitar_out, Some(GuitarOut::Modeling));
        assert_eq!(area.v_link_pallet, Some(12));
        assert_eq!(area.v_link_clip, Some(7));
        assert_eq!(area.v_link_note_clip_change, Some(3));
        assert_eq!(area.v_link_exp, Some(VLinkControl::ColorCb));
        assert_eq!(area.v_link_exp_on, Some(VLinkControl::Bright));
        assert_eq!(area.v_link_gk_vol, Some(VLinkControl::PlaySpeed));
        assert_eq!(area.structure, Some(PatchStructure::Structure2));
        assert_eq!(area.modeling_line_route, Some(LineRoute::AmpMod));
        assert_eq!(area.analog_pu_line_route, Some(LineRoute::Mfx));
        assert_eq!(area.patch_level.unwrap().get(), 75);
        assert_eq!(area.analog_pu_tone_sw, Some(AnalogPuToneSw::Off));
        assert_eq!(area.analog_pu_tone_level, Some(80));
        assert_eq!(area.alt_tuning_sw, Some(OnOff::On));
        assert_eq!(area.alt_tuning_type, Some(AltTuningType::Baritone));
        assert_eq!(area.alt_tuning_user_shift[0], Some(0x30));
        assert_eq!(area.alt_tuning_user_shift[5], Some(0x50));
        assert_eq!(area.patch_tempo.unwrap().raw(), 0x96);
        assert_eq!(area.bypass_chorus, Some(50));
        assert_eq!(area.bypass_delay, Some(60));
        assert_eq!(area.bypass_reverb, Some(70));
        assert_eq!(area.exp_mod_min_envelope, Some(0x10));
        assert_eq!(area.exp_mod_max_envelope, Some(0x70));
        assert_eq!(area.exp_on_mod_min_envelope, Some(0x05));
        assert_eq!(area.exp_on_mod_max_envelope, Some(0x65));
        assert_eq!(area.gk_vol_mod_min_envelope, Some(0x20));
        assert_eq!(area.gk_vol_mod_max_envelope, Some(0x60));
        // The two undocumented bytes survived via unknown_bytes.
        assert_eq!(area.unknown_bytes.get("02:00:2F"), Some(&0xAA));
        assert_eq!(area.unknown_bytes.get("02:00:41"), Some(&0xBB));

        // Round-trip preserves everything.
        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn mfx_block_decodes_header_eq_and_preserves_tail() {
        let mut payload = vec![
            70,                              // 0x00 chorus_send
            45,                              // 0x01 delay_send
            30,                              // 0x02 reverb_send
            0xAB,                            // 0x03 reserved (raw)
            OnOff::On.to_byte(),             // 0x04 switch
            MfxType::Phaser.to_byte(),       // 0x05 type
            50,                              // 0x06 pan (= center)
            EqLowFreq::Hz400.to_byte(),      // 0x07
            0x10,                            // 0x08 low_gain
            EqMidFreq::Khz1_0.to_byte(),     // 0x09
            0x12,                            // 0x0A mid1_gain
            EqMidQ::Q2.to_byte(),            // 0x0B
            EqMidFreq::Khz3_15.to_byte(),    // 0x0C
            0x0F,                            // 0x0D mid2_gain
            EqMidQ::Q4.to_byte(),            // 0x0E
            EqHighFreq::Khz4_0.to_byte(),    // 0x0F
            0x14,                            // 0x10 high_gain
            0x60,                            // 0x11 eq_level
        ];
        // Add 5 bytes into the type-specific tail so we can verify they
        // survive in raw_tail.
        payload.extend([0xC1, 0xC2, 0xC3, 0xC4, 0xC5]); // 0x12..=0x16
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x03, 0x00, 0x00], // MFX slot 0
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        let m = area.mfx.as_ref().expect("mfx should decode");
        assert_eq!(m.chorus_send, Some(70));
        assert_eq!(m.delay_send, Some(45));
        assert_eq!(m.reverb_send, Some(30));
        assert_eq!(m.reserved, Some(0xAB));
        assert_eq!(m.switch, Some(OnOff::On));
        assert_eq!(m.mfx_type, Some(MfxType::Phaser));
        assert_eq!(m.pan, Some(50));
        assert_eq!(m.eq_low_freq, Some(EqLowFreq::Hz400));
        assert_eq!(m.eq_low_gain, Some(0x10));
        assert_eq!(m.eq_mid1_freq, Some(EqMidFreq::Khz1_0));
        assert_eq!(m.eq_mid1_q, Some(EqMidQ::Q2));
        assert_eq!(m.eq_mid2_freq, Some(EqMidFreq::Khz3_15));
        assert_eq!(m.eq_mid2_q, Some(EqMidQ::Q4));
        assert_eq!(m.eq_high_freq, Some(EqHighFreq::Khz4_0));
        assert_eq!(m.eq_high_gain, Some(0x14));
        assert_eq!(m.eq_level, Some(0x60));
        // Type-specific tail survived in raw_tail, keyed by linear offset.
        assert_eq!(m.raw_tail.get(&0x12), Some(&0xC1));
        assert_eq!(m.raw_tail.get(&0x16), Some(&0xC5));

        // Round-trip
        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn page_06_effects_block_round_trips() {
        let payload: Vec<u8> = vec![
            OnOff::On.to_byte(),                  // 0x00 chorus_switch
            ChorusType::Stereo.to_byte(),         // 0x01 chorus_type
            0x40,                                 // 0x02 chorus_rate
            60,                                   // 0x03 chorus_depth
            70,                                   // 0x04 chorus_level
            OnOff::On.to_byte(),                  // 0x05 delay_switch
            DelayType::Tape.to_byte(),            // 0x06 delay_type
            0x0A,                                 // 0x07 delay_time
            0x05,                                 // 0x08 delay_time_b
            0x02,                                 // 0x09 delay_time_c
            55,                                   // 0x0A delay_feedback
            80,                                   // 0x0B delay_level
            OnOff::On.to_byte(),                  // 0x0C reverb_switch
            ReverbType::Hall1.to_byte(),          // 0x0D reverb_type
            50,                                   // 0x0E reverb_time (= 5.1s)
            ReverbHighCut::Khz4_0.to_byte(),      // 0x0F reverb_high_cut
            65,                                   // 0x10 reverb_level
            OnOff::On.to_byte(),                  // 0x11 patch_eq_switch
            PatchEqLoCut::Hz110.to_byte(),        // 0x12
            0x20,                                 // 0x13 patch_eq_low_gain
            PatchEqMidFreq::Hz400.to_byte(),      // 0x14
            PatchEqMidQ::Q2.to_byte(),            // 0x15
            0x10,                                 // 0x16 low_mid_gain
            PatchEqMidFreq::Khz3_15.to_byte(),    // 0x17
            PatchEqMidQ::Q4.to_byte(),            // 0x18
            0x18,                                 // 0x19 hi_mid_gain
            PatchEqHighCut::Khz4_00.to_byte(),    // 0x1A
            0x22,                                 // 0x1B patch_eq_high_gain
            0x20,                                 // 0x1C patch_eq_level
            PatchEqCharacter::Plus2.to_byte(),    // 0x1D
        ];
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x06, 0x00, 0x00],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);

        assert_eq!(area.chorus_switch, Some(OnOff::On));
        assert_eq!(area.chorus_type, Some(ChorusType::Stereo));
        assert_eq!(area.chorus_rate, Some(0x40));
        assert_eq!(area.chorus_depth, Some(60));
        assert_eq!(area.chorus_level, Some(70));
        assert_eq!(area.delay_switch, Some(OnOff::On));
        assert_eq!(area.delay_type, Some(DelayType::Tape));
        assert_eq!(area.delay_time, Some(0x0A));
        assert_eq!(area.delay_time_b, Some(0x05));
        assert_eq!(area.delay_time_c, Some(0x02));
        assert_eq!(area.delay_feedback, Some(55));
        assert_eq!(area.delay_level, Some(80));
        assert_eq!(area.reverb_switch, Some(OnOff::On));
        assert_eq!(area.reverb_type, Some(ReverbType::Hall1));
        assert_eq!(area.reverb_time, Some(50));
        assert_eq!(area.reverb_high_cut, Some(ReverbHighCut::Khz4_0));
        assert_eq!(area.reverb_level, Some(65));
        assert_eq!(area.patch_eq_switch, Some(OnOff::On));
        assert_eq!(area.patch_eq_lo_cut, Some(PatchEqLoCut::Hz110));
        assert_eq!(area.patch_eq_low_gain, Some(0x20));
        assert_eq!(area.patch_eq_lo_mid_freq, Some(PatchEqMidFreq::Hz400));
        assert_eq!(area.patch_eq_lo_mid_q, Some(PatchEqMidQ::Q2));
        assert_eq!(area.patch_eq_low_mid_gain, Some(0x10));
        assert_eq!(area.patch_eq_hi_mid_freq, Some(PatchEqMidFreq::Khz3_15));
        assert_eq!(area.patch_eq_hi_mid_q, Some(PatchEqMidQ::Q4));
        assert_eq!(area.patch_eq_hi_mid_gain, Some(0x18));
        assert_eq!(area.patch_eq_high_cut, Some(PatchEqHighCut::Khz4_00));
        assert_eq!(area.patch_eq_high_gain, Some(0x22));
        assert_eq!(area.patch_eq_level, Some(0x20));
        assert_eq!(area.patch_eq_character, Some(PatchEqCharacter::Plus2));
        assert!(area.unknown_bytes.is_empty());

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn page_07_preamp_and_mod_header_round_trip() {
        let mut payload: Vec<u8> = vec![
            OnOff::On.to_byte(),               // 0x00 preamp_switch
            PreampType::MsHiGain.to_byte(),    // 0x01 preamp_type
            90,                                // 0x02 preamp_gain
            80,                                // 0x03 preamp_level
            PreampGainSw::High.to_byte(),      // 0x04
            OnOff::Off.to_byte(),              // 0x05 solo_sw
            50,                                // 0x06 solo_level
            55,                                // 0x07 bass
            45,                                // 0x08 middle
            60,                                // 0x09 treble
            65,                                // 0x0A presence
            OnOff::On.to_byte(),               // 0x0B bright_sw
            SpeakerType::FourByTwelve.to_byte(), // 0x0C
            MicType::Dyn57.to_byte(),          // 0x0D
            MicDistance::OnMic.to_byte(),      // 0x0E
            3,                                 // 0x0F mic_position (3cm)
            85,                                // 0x10 mic_level
            40,                                // 0x11 mod_chorus_send
            35,                                // 0x12 mod_delay_send
            30,                                // 0x13 mod_reverb_send
            0xCC,                              // 0x14 mod_null_14
            OnOff::On.to_byte(),               // 0x15 mod_switch
            ModType::Phaser.to_byte(),         // 0x16 mod_type
            50,                                // 0x17 mod_pan (center)
        ];
        // Add 4 bytes into the MOD-type-dependent tail to verify they
        // survive in unknown_bytes (pending sum modelling).
        payload.extend([0xE1, 0xE2, 0xE3, 0xE4]); // 0x18..=0x1B
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x07, 0x00, 0x00],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);

        assert_eq!(area.preamp_switch, Some(OnOff::On));
        assert_eq!(area.preamp_type, Some(PreampType::MsHiGain));
        assert_eq!(area.preamp_gain, Some(90));
        assert_eq!(area.preamp_level, Some(80));
        assert_eq!(area.preamp_gain_sw, Some(PreampGainSw::High));
        assert_eq!(area.preamp_solo_sw, Some(OnOff::Off));
        assert_eq!(area.preamp_solo_level, Some(50));
        assert_eq!(area.preamp_bass, Some(55));
        assert_eq!(area.preamp_middle, Some(45));
        assert_eq!(area.preamp_treble, Some(60));
        assert_eq!(area.preamp_presence, Some(65));
        assert_eq!(area.preamp_bright_sw, Some(OnOff::On));
        assert_eq!(area.speaker_type, Some(SpeakerType::FourByTwelve));
        assert_eq!(area.mic_type, Some(MicType::Dyn57));
        assert_eq!(area.mic_distance, Some(MicDistance::OnMic));
        assert_eq!(area.mic_position, Some(3));
        assert_eq!(area.mic_level, Some(85));
        let modu = area
            .modulation
            .as_ref()
            .expect("modulation slot should populate");
        assert_eq!(modu.chorus_send, Some(40));
        assert_eq!(modu.delay_send, Some(35));
        assert_eq!(modu.reverb_send, Some(30));
        assert_eq!(modu.null_14, Some(0xCC));
        assert_eq!(modu.switch, Some(OnOff::On));
        assert_eq!(modu.mod_type, Some(ModType::Phaser));
        assert_eq!(modu.pan, Some(50));
        // MOD-type tail preserved in modulation.raw_tail keyed by offset.
        assert_eq!(modu.raw_tail.get(&0x18), Some(&0xE1));
        assert_eq!(modu.raw_tail.get(&0x1B), Some(&0xE4));

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn mfx_iter_active_type_includes_common_and_owning_type_bytes() {
        // Set active type = SuperFilter, populate a SuperFilter-owned byte
        // (linear 0x12 = "Super Filter Type") and a Phaser-owned byte
        // (linear 0x1F = "Phaser Mode") that's NOT the active type. The
        // active filter should include common header bytes + the SF byte,
        // but exclude the Phaser byte.
        let mut mfx = Mfx {
            mfx_type: Some(MfxType::SuperFilter),
            switch: Some(OnOff::On),
            chorus_send: Some(30),
            ..Mfx::default()
        };
        mfx.raw_tail.insert(0x12, 0x01); // Super Filter Type
        mfx.raw_tail.insert(0x1F, 0x00); // Phaser Mode (inert at this active type)

        let active: Vec<_> = mfx.iter_active_type_params().collect();
        let active_offsets: std::collections::BTreeSet<u16> =
            active.iter().map(|(off, _, _)| *off).collect();
        // Common header bytes present.
        assert!(active_offsets.contains(&0x00)); // chorus_send
        assert!(active_offsets.contains(&0x04)); // switch
        assert!(active_offsets.contains(&0x05)); // mfx_type
        // Super Filter byte present.
        assert!(active_offsets.contains(&0x12));
        // Phaser byte EXCLUDED.
        assert!(!active_offsets.contains(&0x1F));

        // iter_type_params for Phaser yields only the Phaser byte.
        let phaser: Vec<_> = mfx
            .iter_type_params(crate::mfx_params::MfxTypeOwner::Phaser)
            .collect();
        assert_eq!(phaser.len(), 1);
        assert_eq!(phaser[0].0, 0x1F);

        // iter_common_params yields only the common-header bytes.
        let common: Vec<_> = mfx.iter_common_params().collect();
        for (_, _, entry) in &common {
            assert!(entry.owning_type.is_none());
        }
    }

    #[test]
    fn modeling_iter_params_pairs_bytes_with_modeling_params_table() {
        // Populate a single-mode Modeling slot: Guitar Mode E.Guitar
        // Telecaster with tone level 50, plus one type-specific byte at
        // page-0x10 offset 0x2F (linear 0x2F, which the modeling_params
        // table identifies as E.GTR types "01-02" Volume? Let's see
        // what the spot-check matches).
        let mut modeling = Modeling {
            gm_category: Some(GuitarModeCategory::ElectricGuitar),
            gm_egtr_type: Some(GmEGuitarType::Telecaster),
            tone_level: Some(50),
            ..Modeling::default()
        };
        // Linear 0x2F is page 0x10 offset 0x2F = E.GTR types "01-02"
        // PU select per FloorBoard.
        modeling.raw_tail.insert(0x2F, 0x01);

        let collected: Vec<_> = modeling.iter_params().collect();
        let by_lin: std::collections::BTreeMap<u16, (u8, &str, &str)> = collected
            .iter()
            .map(|(lin, b, entry)| (*lin, (*b, entry.category, entry.types)))
            .collect();

        // Common header bytes have category "Modeling".
        let cat_byte = by_lin.get(&0x00).expect("category byte present");
        assert_eq!(cat_byte.1, "Modeling");

        let tone_level = by_lin.get(&0x09).expect("tone_level present");
        assert_eq!(tone_level.0, 50);
        assert_eq!(tone_level.1, "Modeling");

        // Tail byte at 0x2F: E.GTR shared by Strat types 01-02.
        let pu_select = by_lin.get(&0x2F).expect("0x2F present");
        assert_eq!(pu_select.1, "E.GTR");
        assert_eq!(pu_select.2, "01-02");
    }

    #[test]
    fn ns_block_round_trips() {
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x07, 0x00, 0x5A],
            data: Cow::Owned(vec![
                OnOff::On.to_byte(), // 0x5A ns_switch
                40,                  // 0x5B ns_threshold
                60,                  // 0x5C ns_release
            ]),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert_eq!(area.ns_switch, Some(OnOff::On));
        assert_eq!(area.ns_threshold, Some(40));
        assert_eq!(area.ns_release, Some(60));
        // NS bytes were previously falling through to unknown_bytes; with
        // typing they shouldn't anymore.
        assert!(area.unknown_bytes.is_empty());

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn mod_iter_params_pairs_bytes_with_mod_params_table() {
        // Set the type to Wah, populate one byte from Wah's range
        // (0x1F = "Wah Sens"), confirm iter_params yields the typed
        // header bytes plus the raw_tail byte, each with its label.
        let mut modu = Mod {
            mod_type: Some(ModType::Wah),
            switch: Some(OnOff::On),
            ..Mod::default()
        };
        modu.raw_tail.insert(0x1F, 0x40);

        let collected: Vec<_> = modu.iter_params().collect();
        let by_offset: std::collections::BTreeMap<u8, (u8, &str, Option<crate::mod_params::ModTypeOwner>)> =
            collected
                .iter()
                .map(|(off, b, entry)| (*off, (*b, entry.name, entry.owning_type)))
                .collect();

        // Common header bytes carry no owning type.
        assert_eq!(
            by_offset.get(&0x15).map(|x| x.2),
            Some(None),
            "switch byte 0x15 should have no owning type"
        );
        // Type byte at 0x16 likewise (the selector itself).
        assert_eq!(by_offset.get(&0x16).map(|x| x.2), Some(None));
        // Wah Sens at 0x1F is owned by Wah.
        let wah_sens = by_offset.get(&0x1F).expect("0x1F should be present");
        assert_eq!(wah_sens.0, 0x40);
        assert_eq!(wah_sens.2, Some(crate::mod_params::ModTypeOwner::Wah));
        assert!(wah_sens.1.contains("Sens"));
    }

    #[test]
    fn page_10_modeling_header_round_trips() {
        let mut payload: Vec<u8> = vec![
            GuitarModeCategory::ElectricGuitar.to_byte(), // 0x00
            GmEGuitarType::Telecaster.to_byte(),          // 0x01
            GmAcousticType::Nylon.to_byte(),              // 0x02
            GmEBassType::JazzBass.to_byte(),              // 0x03
            ModelingSynthType::Wave.to_byte(),            // 0x04
            BassModeCategory::Synth.to_byte(),            // 0x05
            BmEBassType::PBass.to_byte(),                 // 0x06
            BmEGuitarType::LesPaul.to_byte(),             // 0x07
            ModelingSynthType::Brass.to_byte(),           // 0x08
            85,                                           // 0x09 tone_level
            AnalogPuToneSw::On.to_byte(),                 // 0x0A (= 0x00)
        ];
        // 0x0B..=0x10 string_level[6]
        payload.extend([50, 55, 60, 65, 70, 75]);
        // Strings 1..=6: step at 0x11/13/15/17/19/1B, fine at 0x12/14/16/18/1A/1C
        for s in 0..6 {
            payload.push(0x18 + s); // step (-12..-7 semitones)
            payload.push(0x32 + s); // fine (0..+5 cents-ish)
        }
        payload.push(OnOff::On.to_byte()); // 0x1D 12_string
        payload.push(0xF1); // 0x1E type-specific tail
        payload.push(0xF2); // 0x1F type-specific tail

        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x10, 0x00, 0x00],
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);

        let modeling = area
            .modeling
            .as_ref()
            .expect("modeling slot should populate");
        assert_eq!(
            modeling.gm_category,
            Some(GuitarModeCategory::ElectricGuitar)
        );
        assert_eq!(modeling.gm_egtr_type, Some(GmEGuitarType::Telecaster));
        assert_eq!(modeling.gm_acoustic_type, Some(GmAcousticType::Nylon));
        assert_eq!(modeling.gm_ebass_type, Some(GmEBassType::JazzBass));
        assert_eq!(modeling.gm_synth_type, Some(ModelingSynthType::Wave));
        assert_eq!(modeling.bm_category, Some(BassModeCategory::Synth));
        assert_eq!(modeling.bm_ebass_type, Some(BmEBassType::PBass));
        assert_eq!(modeling.bm_egtr_type, Some(BmEGuitarType::LesPaul));
        assert_eq!(modeling.bm_synth_type, Some(ModelingSynthType::Brass));
        assert_eq!(modeling.tone_level, Some(85));
        assert_eq!(modeling.tone_sw, Some(AnalogPuToneSw::On));
        assert_eq!(modeling.string_level[0], Some(50));
        assert_eq!(modeling.string_level[5], Some(75));
        assert_eq!(modeling.pitch_step[0], Some(0x18));
        assert_eq!(modeling.pitch_step[5], Some(0x1D));
        assert_eq!(modeling.pitch_fine[0], Some(0x32));
        assert_eq!(modeling.pitch_fine[5], Some(0x37));
        assert_eq!(modeling.twelve_string, Some(OnOff::On));
        // Tail bytes preserved in modeling.raw_tail keyed by linear offset.
        assert_eq!(modeling.raw_tail.get(&0x1E), Some(&0xF1));
        assert_eq!(modeling.raw_tail.get(&0x1F), Some(&0xF2));

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn pcm_tone_decodes_two_tones_each_spanning_two_pages() {
        // Per FloorBoard's soundSource_synth_a.cpp / _b.cpp, each PCM
        // tone has its header on page 0x20 (or 0x21) and its tone-
        // shaping tail (Filter/TVF/TVA/...) on page 0x30 (or 0x31).
        // Use different (bank, position) per tone + distinct
        // header/tail bytes so any cross-tone mixup would surface.
        let tone_per_slot = [
            (0_u8, 5_u8),  // slot 0 → tone 5
            (7_u8, 13_u8), // slot 1 → tone 909 (last populated)
        ];
        let mut frames = Vec::new();
        for (slot_idx, (header_page, tail_page)) in
            [(0_usize, (0x20_u8, 0x30_u8)), (1, (0x21, 0x31))]
        {
            let (bank, pos) = tone_per_slot[slot_idx];
            // Header bytes on header_page.
            let header_payload: Vec<u8> = vec![
                0x58 + slot_idx as u8,             // 0x00 synth_mode
                bank,                              // 0x01 tone bank
                pos,                               // 0x02 tone position
                AnalogPuToneSw::Off.to_byte(),     // 0x03 tone_sw
                100,                               // 0x04 tone_level
                0x41,                              // 0x05 octave
                OnOff::On.to_byte(),               // 0x06 chromatic
                OnOff::Off.to_byte(),              // 0x07 legato
                OnOff::On.to_byte(),               // 0x08 nuance_sw
                0x40,                              // 0x09 pan
                0x40,                              // 0x0A pitch_shift
                0x40,                              // 0x0B pitch_fine
                PortamentoSwitch::Tone.to_byte(),  // 0x0C
                5,                                 // 0x0D portamento_time
                8,                                 // 0x0E portamento_raw_0e
                TvaReleaseMode::Mode2.to_byte(),   // 0x0F
                90,                                // 0x10 string_level[0]
                85,                                // 0x11 string_level[1]
            ];
            frames.push(Frame::Dt1 {
                device_id: 0x10,
                address: [TEMP_MSB, header_page, 0x00, 0x00],
                data: Cow::Owned(header_payload),
            });
            // A few tail bytes on tail_page to verify routing.
            frames.push(Frame::Dt1 {
                device_id: 0x10,
                address: [TEMP_MSB, tail_page, 0x00, 0x00],
                data: Cow::Owned(vec![
                    0xA0 + slot_idx as u8, // 0x00 Filter Type (raw)
                    0xB0 + slot_idx as u8, // 0x01 Cutoff (raw)
                ]),
            });
        }
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);

        let expected_linear = [5_u16, 909];
        for (slot_idx, &expected) in expected_linear.iter().enumerate() {
            let pcm = area.pcm[slot_idx].as_ref().expect("slot should populate");
            assert_eq!(pcm.synth_mode, Some(0x58 + slot_idx as u8));
            assert_eq!(pcm.synth_tone.map(|t| t.get()), Some(expected));
            assert_eq!(pcm.tone_sw, Some(AnalogPuToneSw::Off));
            assert_eq!(pcm.tone_level, Some(100));
            assert_eq!(pcm.octave, Some(0x41));
            assert_eq!(pcm.chromatic, Some(OnOff::On));
            assert_eq!(pcm.legato, Some(OnOff::Off));
            assert_eq!(pcm.nuance_sw, Some(OnOff::On));
            assert_eq!(pcm.pan, Some(0x40));
            assert_eq!(pcm.pitch_shift, Some(0x40));
            assert_eq!(pcm.pitch_fine, Some(0x40));
            assert_eq!(pcm.portamento_sw, Some(PortamentoSwitch::Tone));
            assert_eq!(pcm.portamento_time, Some(5));
            assert_eq!(pcm.portamento_raw_0e, Some(8));
            assert_eq!(pcm.tva_release_mode, Some(TvaReleaseMode::Mode2));
            assert_eq!(pcm.string_level[0], Some(90));
            assert_eq!(pcm.string_level[1], Some(85));
            // Tail bytes landed in raw_tail (keyed by tail-page offset).
            assert_eq!(pcm.raw_tail.get(&0x00), Some(&(0xA0 + slot_idx as u8)));
            assert_eq!(pcm.raw_tail.get(&0x01), Some(&(0xB0 + slot_idx as u8)));
        }

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn pcm_tone_index_validates_range_and_round_trips_bytes() {
        // The first tone in the catalog.
        let first = PcmToneIndex::new(0).unwrap();
        assert_eq!(first.to_two_bytes(), [0, 0]);
        assert_eq!(PcmToneIndex::from_two_bytes(0, 0), Some(first));

        // A tone in the middle.
        let mid = PcmToneIndex::new(300).unwrap();
        assert_eq!(mid.to_two_bytes(), [2, 44]);
        assert_eq!(PcmToneIndex::from_two_bytes(2, 44), Some(mid));

        // The very last populated tone.
        let last = PcmToneIndex::new(909).unwrap();
        assert_eq!(last.to_two_bytes(), [7, 13]);
        assert_eq!(PcmToneIndex::from_two_bytes(7, 13), Some(last));

        // Out-of-range linear value.
        assert!(PcmToneIndex::new(910).is_none());

        // Out-of-range byte components.
        assert!(PcmToneIndex::from_two_bytes(8, 0).is_none()); // bank > 7
        assert!(PcmToneIndex::from_two_bytes(0, 0x80).is_none()); // pos > 0x7F
        assert!(PcmToneIndex::from_two_bytes(7, 14).is_none()); // valid bytes, linear > 909
    }

    #[test]
    fn pcm_tone_index_lookup_matches_catalog() {
        // First tone in the catalog.
        let first = PcmToneIndex::new(0).unwrap();
        assert_eq!(first.name(), "St.Piano 1");
        assert_eq!(first.category(), "Acoustic Piano");
        assert_eq!(first.display_number(), 1);

        // Tone 128 — first entry of bank 1 — sits right after the last
        // entry of bank 0. Spot-check the boundary is correct.
        let bank_1_start = PcmToneIndex::new(128).unwrap();
        assert_eq!(bank_1_start.name(), "Bell 2");

        // Last populated tone.
        let last = PcmToneIndex::new(909).unwrap();
        assert_eq!(last.name(), "Dance Kit 3");
        assert_eq!(last.category(), "Drums");
        assert_eq!(last.display_number(), 910);

        // The const table size matches what we promise.
        assert_eq!(crate::pcm_tones::PCM_TONE_COUNT, 910);
    }

    #[test]
    fn pcm_string_level_and_line_route_round_trip() {
        // Header page bytes 0x10..=0x15 = string_level[6], byte 0x16 =
        // line_route. Bytes 0x17/0x18 of the header page are above
        // line_route — they're not part of the documented header so
        // they fall through to PatchArea::unknown_bytes.
        let payload: Vec<u8> = vec![
            80, 75, 70, 65, 60, 55,        // 0x10..=0x15 string_level
            LineRoute::Mfx.to_byte(),      // 0x16 line_route
            0xE1, 0xE2,                    // 0x17/0x18 — fall through
        ];
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x20, 0x00, 0x10], // PCM Tone 1 header page
            data: Cow::Owned(payload),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        let pcm = area.pcm[0].as_ref().expect("Tone 1 should populate");
        assert_eq!(pcm.string_level[0], Some(80));
        assert_eq!(pcm.string_level[5], Some(55));
        assert_eq!(pcm.line_route, Some(LineRoute::Mfx));
        // raw_tail (the TAIL page) is empty — no bytes were sent there.
        assert!(pcm.raw_tail.is_empty());
        // The two unmapped header-page bytes ended up in
        // PatchArea::unknown_bytes since the header doesn't claim them.
        assert_eq!(area.unknown_bytes.get("20:00:17"), Some(&0xE1));
        assert_eq!(area.unknown_bytes.get("20:00:18"), Some(&0xE2));

        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn pcm_tone_partial_decode_parks_byte_for_round_trip() {
        // Only the bank byte arrives (no position). It should park in
        // `pending_bank` and NOT promote to a typed synth_tone.
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x20, 0x00, 0x01],
            data: Cow::Owned(vec![3]),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        let pcm = area.pcm[0].as_ref().expect("Tone 1 should exist");
        assert!(pcm.synth_tone.is_none());
        assert_eq!(pcm.pending_bank, Some(3));
        assert!(pcm.pending_pos.is_none());

        // Round-trip: the partial byte must survive (emitted from
        // pending_bank since synth_tone is None).
        let back = PatchArea::from_frames_at(&area.to_frames(0x10, TEMP_MSB).unwrap(), TEMP_MSB);
        assert_eq!(back, area);
    }

    #[test]
    fn pcm_iter_tail_params_pairs_bytes_with_table() {
        let mut pcm = Pcm::default();
        // Filter Type (0x00), Cutoff (0x01), Portamento Type (0x1B),
        // LFO2 Rate (0x22), and one out-of-range byte at 0x30.
        pcm.raw_tail.insert(0x00, 2);  // LPF
        pcm.raw_tail.insert(0x01, 64);
        pcm.raw_tail.insert(0x1B, 1);  // TIME
        pcm.raw_tail.insert(0x22, 50);
        pcm.raw_tail.insert(0x30, 0x99);

        let collected: Vec<_> = pcm.iter_tail_params().collect();
        let by_off: std::collections::BTreeMap<u8, (u8, Option<&str>, Option<crate::pcm_tail_params::PcmTailGroup>)> =
            collected
                .iter()
                .map(|(off, b, entry)| {
                    (
                        *off,
                        (*b, entry.map(|e| e.name), entry.map(|e| e.group)),
                    )
                })
                .collect();

        assert_eq!(
            by_off.get(&0x00),
            Some(&(2, Some("Filter Type"), Some(crate::pcm_tail_params::PcmTailGroup::Filter)))
        );
        assert_eq!(
            by_off.get(&0x1B),
            Some(&(1, Some("Portamento Type"), Some(crate::pcm_tail_params::PcmTailGroup::Portamento)))
        );
        assert_eq!(
            by_off.get(&0x22),
            Some(&(50, Some("LFO2 Rate"), Some(crate::pcm_tail_params::PcmTailGroup::Lfo)))
        );
        // 0x30 is beyond the documented range — metadata is None.
        assert_eq!(by_off.get(&0x30), Some(&(0x99, None, None)));
    }

    #[test]
    fn pcm_slot_page_mapping_is_correct() {
        // Each tone owns TWO pages: a header page (0x20/0x21) and a
        // tail page (0x30/0x31).
        assert_eq!(pcm_route_for_page(0x20), Some(PcmPageRole::Header(0)));
        assert_eq!(pcm_route_for_page(0x21), Some(PcmPageRole::Header(1)));
        assert_eq!(pcm_route_for_page(0x30), Some(PcmPageRole::Tail(0)));
        assert_eq!(pcm_route_for_page(0x31), Some(PcmPageRole::Tail(1)));
        assert_eq!(pcm_route_for_page(0x22), None);
        assert_eq!(pcm_route_for_page(0x32), None);
        assert_eq!(pcm_pages_for_slot(0), (0x20, 0x30));
        assert_eq!(pcm_pages_for_slot(1), (0x21, 0x31));
    }

    #[test]
    fn preamp_type_byte_symmetry() {
        for raw in 0x00_u8..=0x29 {
            let v = PreampType::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "mismatch for 0x{raw:02X}");
        }
        assert!(PreampType::from_byte(0x2A).is_none());
    }

    #[test]
    fn mfx_page_04_lands_in_the_same_single_mfx_slot() {
        // A byte at page 0x04 offset 0x05 lands at linear offset 0x85
        // of the same single MFX slot — page 0x04 is NOT a separate MFX,
        // it's the continuation of MFX 1's parameter region. The byte
        // at linear 0x85 belongs to the Flanger effect type per the
        // mfx_params table.
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x04, 0x00, 0x05],
            data: Cow::Owned(vec![0x42]),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        let mfx = area.mfx.as_ref().expect("single mfx slot should populate");
        assert_eq!(mfx.raw_tail.get(&0x85), Some(&0x42));
        // Verify the param table identifies this byte as Flanger-owned.
        let entry = &crate::mfx_params::MFX_PARAMS[0x85];
        assert_eq!(entry.page, 0x04);
        assert_eq!(entry.offset, 0x05);
        assert_eq!(
            entry.owning_type,
            Some(crate::mfx_params::MfxTypeOwner::Flanger)
        );
    }

    #[test]
    fn mfx_type_byte_symmetry() {
        for raw in 0x00_u8..=0x13 {
            let v = MfxType::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "mismatch for 0x{raw:02X}");
        }
        assert!(MfxType::from_byte(0x14).is_none());
    }

    #[test]
    fn analog_pu_tone_sw_wire_bytes_are_reversed_from_onoff() {
        // Sanity-check the wire reversal: byte 0x00 == AnalogPuToneSw::On,
        // byte 0x01 == AnalogPuToneSw::Off. This is the OPPOSITE of the
        // OnOff enum which uses 0x00=Off, 0x01=On.
        assert_eq!(AnalogPuToneSw::On.to_byte(), 0x00);
        assert_eq!(AnalogPuToneSw::Off.to_byte(), 0x01);
        assert_eq!(OnOff::Off.to_byte(), 0x00);
        assert_eq!(OnOff::On.to_byte(), 0x01);
    }

    #[test]
    fn master_assign_8_lives_on_page_02() {
        // Assign8 starts at flat offset 0x0C + 7*19 = 0x91 = page 0x02 0x11.
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x02, 0x00, 0x11],
            data: Cow::Owned(vec![OnOff::On.to_byte()]),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert!(area.master_assigns[7].is_some());
        assert_eq!(
            area.master_assigns[7].as_ref().unwrap().on_off,
            Some(OnOff::On)
        );
        for i in 0..7 {
            assert!(area.master_assigns[i].is_none());
        }
    }

    #[test]
    fn master_assign_6_lands_at_expected_offset() {
        // Assign6 starts at lo = 0x0C + 5*19 = 0x6B.
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: [TEMP_MSB, 0x01, 0x00, 0x6B],
            data: Cow::Owned(vec![OnOff::On.to_byte()]),
        }];
        let area = PatchArea::from_frames_at(&frames, TEMP_MSB);
        assert!(area.master_assigns[5].is_some());
        assert_eq!(area.master_assigns[5].as_ref().unwrap().on_off, Some(OnOff::On));
        for i in [0, 1, 2, 3, 4, 6, 7] {
            assert!(area.master_assigns[i].is_none(), "slot {i} should be None");
        }
    }

    #[test]
    fn assign_source_midi_cc_round_trips() {
        for cc in [1, 15, 31, 64, 80, 95] {
            let src = AssignSource::midi_cc(cc).unwrap();
            assert_eq!(AssignSource::from_byte(src.to_byte()), Some(src));
        }
        assert!(AssignSource::midi_cc(0).is_none());
        assert!(AssignSource::midi_cc(32).is_none()); // gap between CC#31 and CC#64
        assert!(AssignSource::midi_cc(63).is_none());
        assert!(AssignSource::midi_cc(96).is_none());
    }

    #[test]
    fn assign_internal_trigger_byte_symmetry() {
        for raw in 0x00_u8..=0x0A {
            let v = AssignInternalTrigger::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "mismatch for 0x{raw:02X}");
        }
        assert!(AssignInternalTrigger::from_byte(0x0B).is_none());
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
