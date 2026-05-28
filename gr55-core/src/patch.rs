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
use crate::system::{HoldType, OnOff, SwitchMode};

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
                // Increment little-endian within a page; carry into `hi`
                // when `lo` wraps, then into the next page if hi also wraps.
                lo = lo.wrapping_add(1);
                if lo == 0 {
                    hi = hi.wrapping_add(1);
                    if hi == 0 {
                        page = page.wrapping_add(1);
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
