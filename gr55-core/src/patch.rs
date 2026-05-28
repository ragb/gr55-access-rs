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

/// Typed view of a single GR-55 patch payload. MSB-agnostic — the caller
/// supplies the base MSB when decoding or encoding.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchArea {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<PatchMode>,
    #[serde(default, skip_serializing_if = "PatchName::is_empty")]
    pub name: PatchName,
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
    fn name_too_long_rejected() {
        use std::str::FromStr;
        let err = PatchName::from_str("0123456789ABCDEFG").unwrap_err();
        assert!(matches!(err, PatchNameError::TooLong(17)));
    }
}
