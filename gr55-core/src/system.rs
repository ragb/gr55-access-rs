//! Typed GR-55 System Area model.
//!
//! v1 covers the confidently-modeled parameters at MSB `0x02` page 1, offsets
//! 0x00..=0x0D (GK Set, Output Select, Assign Hold, MIDI Channel, PC RX/TX
//! switches, V-Link channel, Guitar MIDI Out, MIDI Out Mode, Chromatic, String
//! Channel, Data Thin, CTL/EXP pedal CC#), plus the first byte of the Current
//! Patch parameter at MSB `0x01`. Every other addressable byte present in a
//! parsed System dump is preserved verbatim in `unknown_bytes` so round-trip is
//! lossless even before each field gets a typed accessor.
//!
//! Source of truth for offsets: `data/midi.xml` `<System>` section, surfaced as
//! `gr55_core::midi_map::SYSTEM_PARAMETERS`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::codec::CodecError;
use crate::sysex::Frame;

/// Address of the Current Patch parameter byte 0 (MSB 0x01).
pub const ADDR_CURRENT_PATCH: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

/// Address of GK Set (MSB 0x02 page 1 offset 0x00).
pub const ADDR_GK_SET: [u8; 4] = [0x02, 0x00, 0x00, 0x00];
/// Address of Output Select.
pub const ADDR_OUTPUT_SELECT: [u8; 4] = [0x02, 0x00, 0x00, 0x01];
/// Address of Assign Hold.
pub const ADDR_ASSIGN_HOLD: [u8; 4] = [0x02, 0x00, 0x00, 0x02];
/// Address of MIDI Channel.
pub const ADDR_MIDI_CHANNEL: [u8; 4] = [0x02, 0x00, 0x00, 0x03];
/// Address of Program-Change Receive Switch.
pub const ADDR_PC_RX: [u8; 4] = [0x02, 0x00, 0x00, 0x04];
/// Address of Program-Change Send Switch.
pub const ADDR_PC_TX: [u8; 4] = [0x02, 0x00, 0x00, 0x05];
/// Address of V-Link TX Channel.
pub const ADDR_VLINK_TX_CHANNEL: [u8; 4] = [0x02, 0x00, 0x00, 0x06];
/// Address of Guitar MIDI Out switch.
pub const ADDR_GUITAR_MIDI_OUT: [u8; 4] = [0x02, 0x00, 0x00, 0x07];
/// Address of MIDI Out Mode (Mono/Poly).
pub const ADDR_MIDI_OUT_MODE: [u8; 4] = [0x02, 0x00, 0x00, 0x08];
/// Address of Chromatic mode.
pub const ADDR_CHROMATIC: [u8; 4] = [0x02, 0x00, 0x00, 0x09];
/// Address of String Channel range.
pub const ADDR_STRING_CHANNEL: [u8; 4] = [0x02, 0x00, 0x00, 0x0A];
/// Address of Data Thin (event-rate thinning).
pub const ADDR_DATA_THIN: [u8; 4] = [0x02, 0x00, 0x00, 0x0B];
/// Address of CTL pedal CC# assignment.
pub const ADDR_CTL_PEDAL_CC: [u8; 4] = [0x02, 0x00, 0x00, 0x0C];
/// Address of EXP pedal CC# assignment.
pub const ADDR_EXP_PEDAL_CC: [u8; 4] = [0x02, 0x00, 0x00, 0x0D];

/// Reusable Off/On.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnOff {
    Off,
    On,
}

impl OnOff {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(OnOff::Off),
            1 => Some(OnOff::On),
            _ => None,
        }
    }
    fn to_byte(self) -> u8 {
        match self {
            OnOff::Off => 0,
            OnOff::On => 1,
        }
    }
}

/// GK Set (`<PARAM value="00" name="GK Set" abbr="Both Modes">` in midi.xml).
/// Selects which of 10 GK Pickup user setups applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GkSet {
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

impl GkSet {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0 => GkSet::User1,
            1 => GkSet::User2,
            2 => GkSet::User3,
            3 => GkSet::User4,
            4 => GkSet::User5,
            5 => GkSet::User6,
            6 => GkSet::User7,
            7 => GkSet::User8,
            8 => GkSet::User9,
            9 => GkSet::User10,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        match self {
            GkSet::User1 => 0,
            GkSet::User2 => 1,
            GkSet::User3 => 2,
            GkSet::User4 => 3,
            GkSet::User5 => 4,
            GkSet::User6 => 5,
            GkSet::User7 => 6,
            GkSet::User8 => 7,
            GkSet::User9 => 8,
            GkSet::User10 => 9,
        }
    }
}

/// Output-stage routing target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputSelect {
    LinePhones,
    Jc120Amp,
    SmallAmp,
    ComboAmp,
    StackAmp,
    Jc120Return,
    ComboReturn,
    StackReturn,
    BassAmpWithHorn,
    BassAmpNoHorn,
}

impl OutputSelect {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0 => OutputSelect::LinePhones,
            1 => OutputSelect::Jc120Amp,
            2 => OutputSelect::SmallAmp,
            3 => OutputSelect::ComboAmp,
            4 => OutputSelect::StackAmp,
            5 => OutputSelect::Jc120Return,
            6 => OutputSelect::ComboReturn,
            7 => OutputSelect::StackReturn,
            8 => OutputSelect::BassAmpWithHorn,
            9 => OutputSelect::BassAmpNoHorn,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        match self {
            OutputSelect::LinePhones => 0,
            OutputSelect::Jc120Amp => 1,
            OutputSelect::SmallAmp => 2,
            OutputSelect::ComboAmp => 3,
            OutputSelect::StackAmp => 4,
            OutputSelect::Jc120Return => 5,
            OutputSelect::ComboReturn => 6,
            OutputSelect::StackReturn => 7,
            OutputSelect::BassAmpWithHorn => 8,
            OutputSelect::BassAmpNoHorn => 9,
        }
    }
}

/// MIDI channel 1..=16, encoded on the wire as byte 0..=15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MidiChannel(u8);

impl MidiChannel {
    pub fn new(channel: u8) -> Option<Self> {
        if (1..=16).contains(&channel) {
            Some(MidiChannel(channel))
        } else {
            None
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        Self::new(b + 1)
    }
    fn to_byte(self) -> u8 {
        self.0 - 1
    }
}

/// MIDI Out Mode (Mono / Poly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MidiOutMode {
    Mono,
    Poly,
}

impl MidiOutMode {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(MidiOutMode::Mono),
            1 => Some(MidiOutMode::Poly),
            _ => None,
        }
    }
    fn to_byte(self) -> u8 {
        match self {
            MidiOutMode::Mono => 0,
            MidiOutMode::Poly => 1,
        }
    }
}

/// String Channel base (which 6 consecutive MIDI channels carry the 6 strings).
/// Encoded as the low end of the range: `0` = 1..6, `10` = 11..16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StringChannelBase(u8);

impl StringChannelBase {
    pub fn new(base: u8) -> Option<Self> {
        if base <= 10 {
            Some(StringChannelBase(base))
        } else {
            None
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
}

/// CC# assignment for the foot-pedal inputs.
/// `Off` disables the assignment; `Cc(n)` routes to standard CC# `n`.
/// Wire encoding: 0 = Off, 1..0x1F = CC#01..CC#31, 0x20..0x3F = CC#64..CC#95.
/// (The mapping has a jump because FloorBoard's enum skips the CC#32..63 range
/// to avoid LSB pairs.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PedalCc {
    Off,
    Cc(u8),
}

impl PedalCc {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(PedalCc::Off),
            n @ 1..=0x1F => Some(PedalCc::Cc(n)),
            n @ 0x20..=0x3F => Some(PedalCc::Cc(64 + (n - 0x20))),
            _ => None,
        }
    }
    fn to_byte(self) -> Option<u8> {
        match self {
            PedalCc::Off => Some(0),
            PedalCc::Cc(n @ 1..=31) => Some(n),
            PedalCc::Cc(n @ 64..=95) => Some(0x20 + (n - 64)),
            PedalCc::Cc(_) => None,
        }
    }
}

/// Typed view of the GR-55's System area state.
///
/// `unknown_bytes` captures every address that landed in the dump but isn't
/// yet modeled as a typed field — this keeps round-trip lossless before each
/// field gets an enum or struct.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SystemArea {
    /// Currently-selected patch byte 0 (LSB) of the multi-byte selector.
    /// Reported raw because the full multi-byte encoding has not been
    /// fully verified against a real device. Byte values 0..=127 map to
    /// `User 01:1` through `User 43:1` per FloorBoard's enum.
    pub current_patch_byte_0: Option<u8>,

    pub gk_set: Option<GkSet>,
    pub output_select: Option<OutputSelect>,
    pub assign_hold: Option<OnOff>,
    pub midi_channel: Option<MidiChannel>,
    pub pc_rx: Option<OnOff>,
    pub pc_tx: Option<OnOff>,
    pub v_link_tx_channel: Option<MidiChannel>,
    pub guitar_midi_out: Option<OnOff>,
    pub midi_out_mode: Option<MidiOutMode>,
    pub chromatic: Option<OnOff>,
    pub string_channel_base: Option<StringChannelBase>,
    pub data_thin: Option<OnOff>,
    pub ctl_pedal_cc: Option<PedalCc>,
    pub exp_pedal_cc: Option<PedalCc>,

    /// Every System-area byte not yet promoted to a typed field, keyed by its
    /// full 4-byte wire address. Preserves round-trip and surfaces unknowns to
    /// callers (e.g. `gr55 show` can list them).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unknown_bytes: BTreeMap<String, u8>,
}

impl SystemArea {
    /// Decode a sequence of DT1 frames into a `SystemArea`. RQ1 frames and
    /// frames outside the System area MSBs (`0x01`, `0x02`) are ignored.
    pub fn from_frames(frames: &[Frame<'_>]) -> Self {
        let mut bytes: BTreeMap<[u8; 4], u8> = BTreeMap::new();
        for frame in frames {
            let Frame::Dt1 { address, data, .. } = frame else {
                continue;
            };
            if !matches!(address[0], 0x01 | 0x02) {
                continue;
            }
            for (i, b) in data.iter().enumerate() {
                let mut addr = *address;
                let lsb = u32::from(addr[3]) + i as u32;
                if lsb > 0x7F {
                    break;
                }
                addr[3] = lsb as u8;
                bytes.insert(addr, *b);
            }
        }
        Self::from_byte_map(bytes)
    }

    fn from_byte_map(mut bytes: BTreeMap<[u8; 4], u8>) -> Self {
        let mut out = SystemArea::default();
        let take = |bytes: &mut BTreeMap<[u8; 4], u8>, addr: [u8; 4]| bytes.remove(&addr);

        out.current_patch_byte_0 = take(&mut bytes, ADDR_CURRENT_PATCH);

        out.gk_set = take(&mut bytes, ADDR_GK_SET).and_then(GkSet::from_byte);
        out.output_select = take(&mut bytes, ADDR_OUTPUT_SELECT).and_then(OutputSelect::from_byte);
        out.assign_hold = take(&mut bytes, ADDR_ASSIGN_HOLD).and_then(OnOff::from_byte);
        out.midi_channel = take(&mut bytes, ADDR_MIDI_CHANNEL).and_then(MidiChannel::from_byte);
        out.pc_rx = take(&mut bytes, ADDR_PC_RX).and_then(OnOff::from_byte);
        out.pc_tx = take(&mut bytes, ADDR_PC_TX).and_then(OnOff::from_byte);
        out.v_link_tx_channel =
            take(&mut bytes, ADDR_VLINK_TX_CHANNEL).and_then(MidiChannel::from_byte);
        out.guitar_midi_out = take(&mut bytes, ADDR_GUITAR_MIDI_OUT).and_then(OnOff::from_byte);
        out.midi_out_mode = take(&mut bytes, ADDR_MIDI_OUT_MODE).and_then(MidiOutMode::from_byte);
        out.chromatic = take(&mut bytes, ADDR_CHROMATIC).and_then(OnOff::from_byte);
        out.string_channel_base =
            take(&mut bytes, ADDR_STRING_CHANNEL).and_then(StringChannelBase::new);
        out.data_thin = take(&mut bytes, ADDR_DATA_THIN).and_then(OnOff::from_byte);
        out.ctl_pedal_cc = take(&mut bytes, ADDR_CTL_PEDAL_CC).and_then(PedalCc::from_byte);
        out.exp_pedal_cc = take(&mut bytes, ADDR_EXP_PEDAL_CC).and_then(PedalCc::from_byte);

        out.unknown_bytes = bytes
            .into_iter()
            .map(|(addr, byte)| (format_addr(&addr), byte))
            .collect();
        out
    }

    /// Encode this `SystemArea` back to DT1 frames, packing contiguous bytes
    /// into the smallest set of frames possible. Returns owned frames.
    pub fn to_frames(&self, device_id: u8) -> Result<Vec<Frame<'static>>, CodecError> {
        let bytes = self.to_byte_map()?;
        Ok(pack_dt1_frames(device_id, &bytes))
    }

    fn to_byte_map(&self) -> Result<BTreeMap<[u8; 4], u8>, CodecError> {
        let mut bytes = BTreeMap::new();
        if let Some(b) = self.current_patch_byte_0 {
            bytes.insert(ADDR_CURRENT_PATCH, b);
        }
        if let Some(v) = self.gk_set {
            bytes.insert(ADDR_GK_SET, v.to_byte());
        }
        if let Some(v) = self.output_select {
            bytes.insert(ADDR_OUTPUT_SELECT, v.to_byte());
        }
        if let Some(v) = self.assign_hold {
            bytes.insert(ADDR_ASSIGN_HOLD, v.to_byte());
        }
        if let Some(v) = self.midi_channel {
            bytes.insert(ADDR_MIDI_CHANNEL, v.to_byte());
        }
        if let Some(v) = self.pc_rx {
            bytes.insert(ADDR_PC_RX, v.to_byte());
        }
        if let Some(v) = self.pc_tx {
            bytes.insert(ADDR_PC_TX, v.to_byte());
        }
        if let Some(v) = self.v_link_tx_channel {
            bytes.insert(ADDR_VLINK_TX_CHANNEL, v.to_byte());
        }
        if let Some(v) = self.guitar_midi_out {
            bytes.insert(ADDR_GUITAR_MIDI_OUT, v.to_byte());
        }
        if let Some(v) = self.midi_out_mode {
            bytes.insert(ADDR_MIDI_OUT_MODE, v.to_byte());
        }
        if let Some(v) = self.chromatic {
            bytes.insert(ADDR_CHROMATIC, v.to_byte());
        }
        if let Some(v) = self.string_channel_base {
            bytes.insert(ADDR_STRING_CHANNEL, v.get());
        }
        if let Some(v) = self.data_thin {
            bytes.insert(ADDR_DATA_THIN, v.to_byte());
        }
        if let Some(v) = self.ctl_pedal_cc {
            let raw = match v {
                PedalCc::Cc(n) => n,
                PedalCc::Off => 0,
            };
            let byte = v.to_byte().ok_or(CodecError::PedalCcOutOfRange(raw))?;
            bytes.insert(ADDR_CTL_PEDAL_CC, byte);
        }
        if let Some(v) = self.exp_pedal_cc {
            let raw = match v {
                PedalCc::Cc(n) => n,
                PedalCc::Off => 0,
            };
            let byte = v.to_byte().ok_or(CodecError::PedalCcOutOfRange(raw))?;
            bytes.insert(ADDR_EXP_PEDAL_CC, byte);
        }
        for (k, b) in &self.unknown_bytes {
            let addr = parse_addr(k).ok_or_else(|| CodecError::BadStoredAddress(k.clone()))?;
            bytes.insert(addr, *b);
        }
        Ok(bytes)
    }
}

fn format_addr(addr: &[u8; 4]) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}",
        addr[0], addr[1], addr[2], addr[3]
    )
}

fn parse_addr(s: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut out = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        out[i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(out)
}

/// Pack contiguous-address bytes into the minimal sequence of DT1 frames.
fn pack_dt1_frames(device_id: u8, bytes: &BTreeMap<[u8; 4], u8>) -> Vec<Frame<'static>> {
    let mut frames = Vec::new();
    let mut iter = bytes.iter().peekable();
    while let Some((&start_addr, &start_byte)) = iter.next() {
        let mut payload = vec![start_byte];
        let mut last_addr = start_addr;
        while let Some(&(&next_addr, &next_byte)) = iter.peek() {
            if next_addr[..3] == last_addr[..3]
                && next_addr[3] == last_addr[3].wrapping_add(1)
                && next_addr[3] > last_addr[3]
            {
                payload.push(next_byte);
                last_addr = next_addr;
                iter.next();
            } else {
                break;
            }
        }
        frames.push(Frame::Dt1 {
            device_id,
            address: start_addr,
            data: Cow::Owned(payload),
        });
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysex::parse_frames_unchecked;

    #[test]
    fn empty_system_area_roundtrips() {
        let area = SystemArea::default();
        let frames = area.to_frames(0x10).unwrap();
        assert!(frames.is_empty());
        let back = SystemArea::from_frames(&frames);
        assert_eq!(back, area);
    }

    #[test]
    fn typed_fields_roundtrip() {
        let area = SystemArea {
            current_patch_byte_0: Some(0x05),
            gk_set: Some(GkSet::User3),
            output_select: Some(OutputSelect::Jc120Amp),
            assign_hold: Some(OnOff::On),
            midi_channel: Some(MidiChannel::new(10).unwrap()),
            pc_rx: Some(OnOff::On),
            pc_tx: Some(OnOff::Off),
            v_link_tx_channel: Some(MidiChannel::new(16).unwrap()),
            guitar_midi_out: Some(OnOff::Off),
            midi_out_mode: Some(MidiOutMode::Poly),
            chromatic: Some(OnOff::Off),
            string_channel_base: Some(StringChannelBase::new(10).unwrap()),
            data_thin: Some(OnOff::On),
            ctl_pedal_cc: Some(PedalCc::Cc(7)),
            exp_pedal_cc: Some(PedalCc::Cc(64)),
            unknown_bytes: BTreeMap::new(),
        };
        let frames = area.to_frames(0x10).unwrap();
        let back = SystemArea::from_frames(&frames);
        assert_eq!(back, area);
    }

    #[test]
    fn unknown_bytes_roundtrip_via_map() {
        let mut unknown = BTreeMap::new();
        unknown.insert("02:00:00:1F".to_string(), 0x42_u8);
        unknown.insert("02:00:02:00".to_string(), 0x11_u8);
        let area = SystemArea {
            current_patch_byte_0: Some(0x00),
            unknown_bytes: unknown.clone(),
            ..SystemArea::default()
        };
        let frames = area.to_frames(0x10).unwrap();
        let back = SystemArea::from_frames(&frames);
        assert_eq!(back.unknown_bytes, unknown);
        assert_eq!(back.current_patch_byte_0, Some(0x00));
    }

    #[test]
    fn decodes_floorboard_system_syx_first_frame() {
        let bytes: &[u8] = include_bytes!("../tests/fixtures/floorboard_system_area.syx");
        let frames: Vec<Frame<'_>> = parse_frames_unchecked(bytes)
            .map(|r| r.unwrap().0)
            .collect();
        let area = SystemArea::from_frames(&frames);
        // First frame of system.syx writes [01, 00, 00, 00] = 0x00 and
        // [01, 00, 00, 01] = 0x39. Byte 0 is the current-patch low byte (0 = User 01:1).
        assert_eq!(area.current_patch_byte_0, Some(0x00));
        // Byte 1 isn't yet modeled — it should live under unknown_bytes.
        assert_eq!(area.unknown_bytes.get("01:00:00:01"), Some(&0x39));
    }

    #[test]
    fn yaml_roundtrip() {
        let area = SystemArea {
            current_patch_byte_0: Some(0x00),
            midi_channel: Some(MidiChannel::new(1).unwrap()),
            output_select: Some(OutputSelect::LinePhones),
            ..SystemArea::default()
        };
        let yaml = serde_yaml::to_string(&area).unwrap();
        let back: SystemArea = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, area);
    }

    #[test]
    fn dt1_frames_pack_contiguous_bytes() {
        let area = SystemArea {
            midi_channel: Some(MidiChannel::new(5).unwrap()), // [02,00,00,03] = 4
            pc_rx: Some(OnOff::On),                           // [02,00,00,04] = 1
            pc_tx: Some(OnOff::Off),                          // [02,00,00,05] = 0
            ..SystemArea::default()
        };
        let frames = area.to_frames(0x10).unwrap();
        assert_eq!(
            frames.len(),
            1,
            "three contiguous addresses should pack into one DT1"
        );
        if let Frame::Dt1 { address, data, .. } = &frames[0] {
            assert_eq!(*address, [0x02, 0x00, 0x00, 0x03]);
            assert_eq!(data.as_ref(), &[4, 1, 0]);
        } else {
            panic!("expected DT1");
        }
    }
}
