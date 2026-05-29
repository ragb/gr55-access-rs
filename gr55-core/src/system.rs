//! Typed GR-55 System Area model.
//!
//! MSB `0x02` page 0 — FloorBoard's System menu page — is fully typed,
//! including the three multi-byte audio levels (Player Level, USB Audio In,
//! USB Audio Out) whose 14-bit MSB-first encoding came out of FloorBoard's
//! `customKnob.cpp:112-117`.
//!
//! Coverage of MSB `0x02` page 0 (System + Master menus) is now complete.
//! The four stack-control discriminators on page 2 (`ctl_pedal_function`,
//! `exp_pedal_off_function`, `exp_pedal_on_function`,
//! `exp_pedal_switch_function`) are all typed.
//!
//! Still on `unknown_bytes` until typed:
//! - MSB `0x02` page 2 sub-fields keyed by each function (e.g. the Hold-mode
//!   parameters at `[02, 02, 0x01..0x0C]` when `ctl_pedal_function == Hold`).
//!   These are 100+ raw bytes that would need per-discriminator sum-type
//!   modeling to make ergonomic; round-trip remains lossless.
//! - GK setups 1..=10 on MSB `0x02` sub-LSBs `0x04..0x0D` (each is a few
//!   hundred parameters; untyped).
//!
//! Cross-references:
//! - **Field list**: FloorBoard's `menuPage_system.cpp`. The `(hex1, hex2,
//!   hex3)` triplet on each `addComboBox` / `addKnob` call is the wire address.
//! - **Multi-byte encoding**: FloorBoard's `customKnob.cpp:102-136` —
//!   `byte_hi = value / 128`, `byte_lo = value % 128`, written to consecutive
//!   addresses starting at `(hex1, hex2, hex3)`.
//! - **Per-field semantics**: `data/midi.xml` `<System>` section, exposed via
//!   `gr55_core::midi_map::SYSTEM_PARAMETERS`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::address::PatchSlot;
use crate::codec::CodecError;
use crate::sysex::Frame;

/// High byte of Current Patch (14-bit MSB-first). Low byte at `[0x01, 0x00, 0x00, 0x01]`.
/// Decoding: `linear_index = byte_hi * 128 + byte_lo` → `PatchSlot::from_linear_index`.
/// (The midi.xml structure shows 19 `<DATA value="00".."12">` siblings under
/// `<PARAM name="Current patch">`, but those represent enum-discriminators for
/// each possible high-byte value, not separate wire addresses. The wire only
/// carries two bytes.)
pub const ADDR_CURRENT_PATCH_HI: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
/// Low byte of Current Patch.
pub const ADDR_CURRENT_PATCH_LO: [u8; 4] = [0x01, 0x00, 0x00, 0x01];

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
/// Address of EXP pedal Bend range (-24..=+24 semitones).
pub const ADDR_EXP_PEDAL_BEND: [u8; 4] = [0x02, 0x00, 0x00, 0x0E];
/// Address of GK VOL CC# assignment.
pub const ADDR_GK_VOL_CC: [u8; 4] = [0x02, 0x00, 0x00, 0x0F];
/// Address of GK S1 CC# assignment.
pub const ADDR_GK_S1_CC: [u8; 4] = [0x02, 0x00, 0x00, 0x10];
/// Address of GK S2 CC# assignment.
pub const ADDR_GK_S2_CC: [u8; 4] = [0x02, 0x00, 0x00, 0x11];
/// Address of MIDI Map (Default Fixed / Programmable).
pub const ADDR_MIDI_MAP: [u8; 4] = [0x02, 0x00, 0x00, 0x12];
/// Address of Monitor Direct (Off / On).
pub const ADDR_MONITOR_DIRECT: [u8; 4] = [0x02, 0x00, 0x00, 0x15];
/// Address of Guitar Out Source.
pub const ADDR_GUITAR_OUT_SOURCE: [u8; 4] = [0x02, 0x00, 0x00, 0x16];
/// Address of Master Tune (435..=445 Hz).
pub const ADDR_MASTER_TUNE: [u8; 4] = [0x02, 0x00, 0x00, 0x17];
/// Address of Tuner Mute (Off / On).
pub const ADDR_TUNER_MUTE: [u8; 4] = [0x02, 0x00, 0x00, 0x18];
/// Address of Startup Mode (Guitar / Bass).
pub const ADDR_STARTUP_MODE: [u8; 4] = [0x02, 0x00, 0x00, 0x1A];
/// High byte of Player Level (two-byte 14-bit value, 0..=200). Low byte at 0x1C.
pub const ADDR_PLAYER_LEVEL_HI: [u8; 4] = [0x02, 0x00, 0x00, 0x1B];
/// Low byte of Player Level.
pub const ADDR_PLAYER_LEVEL_LO: [u8; 4] = [0x02, 0x00, 0x00, 0x1C];
/// High byte of USB Audio In Level (14-bit, 0..=200). Low byte at 0x1E.
pub const ADDR_USB_AUDIO_IN_HI: [u8; 4] = [0x02, 0x00, 0x00, 0x1D];
/// Low byte of USB Audio In Level.
pub const ADDR_USB_AUDIO_IN_LO: [u8; 4] = [0x02, 0x00, 0x00, 0x1E];
/// High byte of USB Audio Out Level (14-bit, 0..=200). Low byte at 0x20.
pub const ADDR_USB_AUDIO_OUT_HI: [u8; 4] = [0x02, 0x00, 0x00, 0x1F];
/// Low byte of USB Audio Out Level.
pub const ADDR_USB_AUDIO_OUT_LO: [u8; 4] = [0x02, 0x00, 0x00, 0x20];

/// High nibble of Patch Level (Master menu; 4-nibble BCD-like encoding,
/// 0..=200). Low nibble at 0x31. Encoding source: FloorBoard
/// `customDataKnob.cpp:106-128`.
pub const ADDR_PATCH_LEVEL_HI: [u8; 4] = [0x02, 0x00, 0x00, 0x30];
/// Low nibble of Patch Level.
pub const ADDR_PATCH_LEVEL_LO: [u8; 4] = [0x02, 0x00, 0x00, 0x31];

/// High nibble of Master BPM (Master menu; 4-nibble BCD-like encoding).
/// Low nibble at 0x3D. Exact BPM range is TBD (FloorBoard's range table
/// requires a `getRange("Tables", "00", "00", "06")` lookup that I haven't
/// fully reproduced; provisional 0..=255 here is the maximum the 4-nibble
/// 2-byte encoding can carry).
pub const ADDR_MASTER_BPM_HI: [u8; 4] = [0x02, 0x00, 0x00, 0x3C];
/// Low nibble of Master BPM.
pub const ADDR_MASTER_BPM_LO: [u8; 4] = [0x02, 0x00, 0x00, 0x3D];

// Master menu remaining single-byte parameters (mined from menuPage_master.cpp).
// midi.xml's `<LSB value="00" name="page 1">` only documents PARAMs up to
// offset 0x26, so the enums for 0x24 / 0x25 / 0x35 aren't formally listed —
// the bytes round-trip raw via Option<u8>.

/// GK Set Select (Master menu). Likely reuses the same 10-entry enum as
/// `gk_set` at offset `0x00`, but FloorBoard's addressing routes it through
/// `addComboBox(... "02", "00", "24")` without an enum hookup in `midi.xml`.
/// Stored raw.
pub const ADDR_GK_SET_SELECT: [u8; 4] = [0x02, 0x00, 0x00, 0x24];
/// Master Guitar Out — distinct from `guitar_out_source` at offset 0x16.
pub const ADDR_GUITAR_OUT: [u8; 4] = [0x02, 0x00, 0x00, 0x25];
/// V-LINK Palette.
pub const ADDR_VLINK_PALETTE: [u8; 4] = [0x02, 0x00, 0x00, 0x26];
/// V-LINK Clip.
pub const ADDR_VLINK_CLIP: [u8; 4] = [0x02, 0x00, 0x00, 0x27];
/// V-LINK Note Clip Change.
pub const ADDR_VLINK_NOTE_CLIP_CHANGE: [u8; 4] = [0x02, 0x00, 0x00, 0x28];
/// V-LINK EXP (pedal).
pub const ADDR_VLINK_EXP: [u8; 4] = [0x02, 0x00, 0x00, 0x29];
/// V-LINK EXP ON.
pub const ADDR_VLINK_EXP_ON: [u8; 4] = [0x02, 0x00, 0x00, 0x2A];
/// V-LINK GK VOL.
pub const ADDR_VLINK_GK_VOL: [u8; 4] = [0x02, 0x00, 0x00, 0x2B];
/// Alternate Tuning enable switch.
pub const ADDR_ALT_TUNING_SW: [u8; 4] = [0x02, 0x00, 0x00, 0x34];
/// Alternate Tuning Type.
pub const ADDR_ALT_TUNING_TYPE: [u8; 4] = [0x02, 0x00, 0x00, 0x35];
/// User Tuning Shift, strings 1..=6.
pub const ADDR_USER_TUNING_SHIFT_STRINGS: [[u8; 4]; 6] = [
    [0x02, 0x00, 0x00, 0x36],
    [0x02, 0x00, 0x00, 0x37],
    [0x02, 0x00, 0x00, 0x38],
    [0x02, 0x00, 0x00, 0x39],
    [0x02, 0x00, 0x00, 0x3A],
    [0x02, 0x00, 0x00, 0x3B],
];

/// EXP Pedal SWITCH Function selector (page 2, 4th and final discriminator).
/// 21 enum values; sub-fields at `[02, 02, 0x3C..0x78]`.
pub const ADDR_EXP_PEDAL_SWITCH_FUNCTION: [u8; 4] = [0x02, 0x02, 0x3B, 0x00];

// CTL Pedal sub-fields on page 2 (active when ctl_pedal_function selects them).
/// CTL Pedal Hold Type (1/2/3/4) — active when CTL function is Hold.
pub const ADDR_CTL_HOLD_TYPE: [u8; 4] = [0x02, 0x02, 0x01, 0x00];
/// CTL Pedal Switch Mode (Latch/Moment) — active when CTL function is Hold.
pub const ADDR_CTL_SWITCH_MODE: [u8; 4] = [0x02, 0x02, 0x02, 0x00];
/// CTL Pedal Hold PCM 1.
pub const ADDR_CTL_HOLD_PCM_1: [u8; 4] = [0x02, 0x02, 0x03, 0x00];
/// CTL Pedal Hold PCM 2.
pub const ADDR_CTL_HOLD_PCM_2: [u8; 4] = [0x02, 0x02, 0x04, 0x00];
/// CTL Tone-Sw OFF-action PCM 1.
pub const ADDR_CTL_TONE_SW_OFF_PCM_1: [u8; 4] = [0x02, 0x02, 0x05, 0x00];
/// CTL Tone-Sw OFF-action PCM 2.
pub const ADDR_CTL_TONE_SW_OFF_PCM_2: [u8; 4] = [0x02, 0x02, 0x06, 0x00];
/// CTL Tone-Sw OFF-action Modeling.
pub const ADDR_CTL_TONE_SW_OFF_MODELING: [u8; 4] = [0x02, 0x02, 0x07, 0x00];
/// CTL Tone-Sw OFF-action Normal PU.
pub const ADDR_CTL_TONE_SW_OFF_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x08, 0x00];
/// CTL Tone-Sw ON-action PCM 1.
pub const ADDR_CTL_TONE_SW_ON_PCM_1: [u8; 4] = [0x02, 0x02, 0x09, 0x00];
/// CTL Tone-Sw ON-action PCM 2.
pub const ADDR_CTL_TONE_SW_ON_PCM_2: [u8; 4] = [0x02, 0x02, 0x0A, 0x00];
/// CTL Tone-Sw ON-action Modeling.
pub const ADDR_CTL_TONE_SW_ON_MODELING: [u8; 4] = [0x02, 0x02, 0x0B, 0x00];
/// CTL Tone-Sw ON-action Normal PU.
pub const ADDR_CTL_TONE_SW_ON_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x0C, 0x00];

// EXP Pedal OFF sub-fields (active when exp_pedal_off_function selects them).
pub const ADDR_EXP_OFF_TONE_VOL_PCM_1: [u8; 4] = [0x02, 0x02, 0x0E, 0x00];
pub const ADDR_EXP_OFF_TONE_VOL_PCM_2: [u8; 4] = [0x02, 0x02, 0x0F, 0x00];
pub const ADDR_EXP_OFF_TONE_VOL_MODELING: [u8; 4] = [0x02, 0x02, 0x10, 0x00];
pub const ADDR_EXP_OFF_TONE_VOL_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x11, 0x00];
pub const ADDR_EXP_OFF_PITCH_BEND_DEPTH: [u8; 4] = [0x02, 0x02, 0x12, 0x00];
pub const ADDR_EXP_OFF_PITCH_BEND_PCM_1: [u8; 4] = [0x02, 0x02, 0x13, 0x00];
pub const ADDR_EXP_OFF_PITCH_BEND_PCM_2: [u8; 4] = [0x02, 0x02, 0x14, 0x00];
pub const ADDR_EXP_OFF_PITCH_BEND_MODELING: [u8; 4] = [0x02, 0x02, 0x15, 0x00];
pub const ADDR_EXP_OFF_MOD_MIN: [u8; 4] = [0x02, 0x02, 0x16, 0x00];
pub const ADDR_EXP_OFF_MOD_MAX: [u8; 4] = [0x02, 0x02, 0x17, 0x00];
pub const ADDR_EXP_OFF_MOD_PCM_1: [u8; 4] = [0x02, 0x02, 0x18, 0x00];
pub const ADDR_EXP_OFF_MOD_PCM_2: [u8; 4] = [0x02, 0x02, 0x19, 0x00];
pub const ADDR_EXP_OFF_CROSS_FADER_A: [u8; 4] = [0x02, 0x02, 0x1A, 0x00];
pub const ADDR_EXP_OFF_CROSS_FADER_B: [u8; 4] = [0x02, 0x02, 0x1B, 0x00];
pub const ADDR_EXP_OFF_CROSS_FADER_C: [u8; 4] = [0x02, 0x02, 0x1C, 0x00];
pub const ADDR_EXP_OFF_CROSS_FADER_D: [u8; 4] = [0x02, 0x02, 0x1D, 0x00];
pub const ADDR_EXP_OFF_DELAY_MIN: [u8; 4] = [0x02, 0x02, 0x1E, 0x00];
pub const ADDR_EXP_OFF_DELAY_MAX: [u8; 4] = [0x02, 0x02, 0x1F, 0x00];
pub const ADDR_EXP_OFF_REVERB_MIN: [u8; 4] = [0x02, 0x02, 0x20, 0x00];
pub const ADDR_EXP_OFF_REVERB_MAX: [u8; 4] = [0x02, 0x02, 0x21, 0x00];
pub const ADDR_EXP_OFF_CHORUS_MIN: [u8; 4] = [0x02, 0x02, 0x22, 0x00];
pub const ADDR_EXP_OFF_CHORUS_MAX: [u8; 4] = [0x02, 0x02, 0x23, 0x00];
pub const ADDR_EXP_OFF_MOD_CONTROL_A: [u8; 4] = [0x02, 0x02, 0x79, 0x00];
pub const ADDR_EXP_OFF_MOD_CONTROL_B: [u8; 4] = [0x02, 0x02, 0x7A, 0x00];

// EXP Pedal ON sub-fields (parallel to EXP OFF; midi.xml:3246-3327).
pub const ADDR_EXP_ON_TONE_VOL_PCM_1: [u8; 4] = [0x02, 0x02, 0x25, 0x00];
pub const ADDR_EXP_ON_TONE_VOL_PCM_2: [u8; 4] = [0x02, 0x02, 0x26, 0x00];
pub const ADDR_EXP_ON_TONE_VOL_MODELING: [u8; 4] = [0x02, 0x02, 0x27, 0x00];
pub const ADDR_EXP_ON_TONE_VOL_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x28, 0x00];
pub const ADDR_EXP_ON_PITCH_BEND_DEPTH: [u8; 4] = [0x02, 0x02, 0x29, 0x00];
pub const ADDR_EXP_ON_PITCH_BEND_PCM_1: [u8; 4] = [0x02, 0x02, 0x2A, 0x00];
pub const ADDR_EXP_ON_PITCH_BEND_PCM_2: [u8; 4] = [0x02, 0x02, 0x2B, 0x00];
pub const ADDR_EXP_ON_PITCH_BEND_MODELING: [u8; 4] = [0x02, 0x02, 0x2C, 0x00];
pub const ADDR_EXP_ON_MOD_MIN: [u8; 4] = [0x02, 0x02, 0x2D, 0x00];
pub const ADDR_EXP_ON_MOD_MAX: [u8; 4] = [0x02, 0x02, 0x2E, 0x00];
pub const ADDR_EXP_ON_MOD_PCM_1: [u8; 4] = [0x02, 0x02, 0x2F, 0x00];
pub const ADDR_EXP_ON_MOD_PCM_2: [u8; 4] = [0x02, 0x02, 0x30, 0x00];
pub const ADDR_EXP_ON_CROSS_FADER_A: [u8; 4] = [0x02, 0x02, 0x31, 0x00];
pub const ADDR_EXP_ON_CROSS_FADER_B: [u8; 4] = [0x02, 0x02, 0x32, 0x00];
pub const ADDR_EXP_ON_CROSS_FADER_C: [u8; 4] = [0x02, 0x02, 0x33, 0x00];
pub const ADDR_EXP_ON_CROSS_FADER_D: [u8; 4] = [0x02, 0x02, 0x34, 0x00];
pub const ADDR_EXP_ON_DELAY_MIN: [u8; 4] = [0x02, 0x02, 0x35, 0x00];
pub const ADDR_EXP_ON_DELAY_MAX: [u8; 4] = [0x02, 0x02, 0x36, 0x00];
pub const ADDR_EXP_ON_REVERB_MIN: [u8; 4] = [0x02, 0x02, 0x37, 0x00];
pub const ADDR_EXP_ON_REVERB_MAX: [u8; 4] = [0x02, 0x02, 0x38, 0x00];
pub const ADDR_EXP_ON_CHORUS_MIN: [u8; 4] = [0x02, 0x02, 0x39, 0x00];
pub const ADDR_EXP_ON_CHORUS_MAX: [u8; 4] = [0x02, 0x02, 0x3A, 0x00];
pub const ADDR_EXP_ON_MOD_CONTROL_A: [u8; 4] = [0x02, 0x02, 0x7B, 0x00];
pub const ADDR_EXP_ON_MOD_CONTROL_B: [u8; 4] = [0x02, 0x02, 0x7C, 0x00];

// EXP Pedal SWITCH sub-fields (Tone Sw action; midi.xml:3356-3423).
pub const ADDR_EXP_SW_TONE_SW_OFF_PCM_1: [u8; 4] = [0x02, 0x02, 0x40, 0x00];
pub const ADDR_EXP_SW_TONE_SW_OFF_PCM_2: [u8; 4] = [0x02, 0x02, 0x41, 0x00];
pub const ADDR_EXP_SW_TONE_SW_OFF_MODELING: [u8; 4] = [0x02, 0x02, 0x42, 0x00];
pub const ADDR_EXP_SW_TONE_SW_OFF_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x43, 0x00];
pub const ADDR_EXP_SW_TONE_SW_ON_PCM_1: [u8; 4] = [0x02, 0x02, 0x44, 0x00];
pub const ADDR_EXP_SW_TONE_SW_ON_PCM_2: [u8; 4] = [0x02, 0x02, 0x45, 0x00];
pub const ADDR_EXP_SW_TONE_SW_ON_MODELING: [u8; 4] = [0x02, 0x02, 0x46, 0x00];
pub const ADDR_EXP_SW_TONE_SW_ON_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x47, 0x00];

// GK Switch 1/2 Function discriminators + Tone Sw sub-fields (midi.xml:3484-3600).
pub const ADDR_GK_S1_FUNCTION: [u8; 4] = [0x02, 0x02, 0x5F, 0x00];
pub const ADDR_GK_S1_TONE_SW_OFF_PCM_1: [u8; 4] = [0x02, 0x02, 0x64, 0x00];
pub const ADDR_GK_S1_TONE_SW_OFF_PCM_2: [u8; 4] = [0x02, 0x02, 0x65, 0x00];
pub const ADDR_GK_S1_TONE_SW_OFF_MODELING: [u8; 4] = [0x02, 0x02, 0x66, 0x00];
pub const ADDR_GK_S1_TONE_SW_OFF_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x67, 0x00];
pub const ADDR_GK_S1_TONE_SW_ON_PCM_1: [u8; 4] = [0x02, 0x02, 0x68, 0x00];
pub const ADDR_GK_S1_TONE_SW_ON_PCM_2: [u8; 4] = [0x02, 0x02, 0x69, 0x00];
pub const ADDR_GK_S1_TONE_SW_ON_MODELING: [u8; 4] = [0x02, 0x02, 0x6A, 0x00];
pub const ADDR_GK_S1_TONE_SW_ON_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x6B, 0x00];
pub const ADDR_GK_S2_FUNCTION: [u8; 4] = [0x02, 0x02, 0x6C, 0x00];
pub const ADDR_GK_S2_TONE_SW_OFF_PCM_1: [u8; 4] = [0x02, 0x02, 0x71, 0x00];
pub const ADDR_GK_S2_TONE_SW_OFF_PCM_2: [u8; 4] = [0x02, 0x02, 0x72, 0x00];
pub const ADDR_GK_S2_TONE_SW_OFF_MODELING: [u8; 4] = [0x02, 0x02, 0x73, 0x00];
pub const ADDR_GK_S2_TONE_SW_OFF_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x74, 0x00];
pub const ADDR_GK_S2_TONE_SW_ON_PCM_1: [u8; 4] = [0x02, 0x02, 0x75, 0x00];
pub const ADDR_GK_S2_TONE_SW_ON_PCM_2: [u8; 4] = [0x02, 0x02, 0x76, 0x00];
pub const ADDR_GK_S2_TONE_SW_ON_MODELING: [u8; 4] = [0x02, 0x02, 0x77, 0x00];
pub const ADDR_GK_S2_TONE_SW_ON_NORMAL_PU: [u8; 4] = [0x02, 0x02, 0x78, 0x00];

/// GK VOL pedal Mod Control min/max (0..=120 per midi.xml).
pub const ADDR_VOL_MOD_CONTROL_A: [u8; 4] = [0x02, 0x02, 0x7D, 0x00];
pub const ADDR_VOL_MOD_CONTROL_B: [u8; 4] = [0x02, 0x02, 0x7E, 0x00];

/// MSB 0x02 sub-LSBs that hold GK setups 1..=10.
pub const GK_SETUP_SUB_LSBS: [u8; 10] =
    [0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D];

/// Number of bytes carried per GK setup. midi.xml declares offsets `0x00..=0x7F`
/// within each setup (with many unused / padding entries); we accept anything in
/// that range and round-trip it verbatim.
pub const GK_SETUP_BYTE_COUNT: usize = 128;

/// CTL Pedal Function selector (page 2). 22 enum values; chosen function
/// determines which sub-fields at `[02, 02, 0x01..0x0C]` are active.
pub const ADDR_CTL_PEDAL_FUNCTION: [u8; 4] = [0x02, 0x02, 0x00, 0x00];
/// EXP Pedal Function selector while EXP Pedal Switch is OFF (page 2).
/// 11 enum values; sub-fields at `[02, 02, 0x0E..0x23, 0x79, 0x7A]`.
pub const ADDR_EXP_PEDAL_OFF_FUNCTION: [u8; 4] = [0x02, 0x02, 0x0D, 0x00];
/// EXP Pedal Function selector while EXP Pedal Switch is ON (page 2).
/// Same enum as `ADDR_EXP_PEDAL_OFF_FUNCTION`; sub-fields at `[02, 02, 0x25..0x3A, 0x7B, 0x7C]`.
pub const ADDR_EXP_PEDAL_ON_FUNCTION: [u8; 4] = [0x02, 0x02, 0x24, 0x00];

/// Reusable Off/On.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "lowercase")]
pub enum OnOff {
    Off,
    On,
}

impl OnOff {
    pub(crate) fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(OnOff::Off),
            1 => Some(OnOff::On),
            _ => None,
        }
    }
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            OnOff::Off => 0,
            OnOff::On => 1,
        }
    }
}

/// GK Set (`<PARAM value="00" name="GK Set" abbr="Both Modes">` in midi.xml).
/// Selects which of 10 GK Pickup user setups applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
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

/// EXP pedal bend range in semitones (-24..=+24). Wire encoding adds 24
/// to land in the unsigned `0x00..=0x30` range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct ExpPedalBend(i8);

impl ExpPedalBend {
    pub fn new(semitones: i8) -> Option<Self> {
        if (-24..=24).contains(&semitones) {
            Some(ExpPedalBend(semitones))
        } else {
            None
        }
    }
    pub fn get(self) -> i8 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        if b <= 0x30 {
            Some(ExpPedalBend((b as i8) - 24))
        } else {
            None
        }
    }
    fn to_byte(self) -> u8 {
        (self.0 + 24) as u8
    }
}

/// MIDI Map mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum MidiMap {
    DefaultFixed,
    Programmable,
}

impl MidiMap {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(MidiMap::DefaultFixed),
            1 => Some(MidiMap::Programmable),
            _ => None,
        }
    }
    fn to_byte(self) -> u8 {
        match self {
            MidiMap::DefaultFixed => 0,
            MidiMap::Programmable => 1,
        }
    }
}

/// Source feeding the GR-55's Guitar Out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum GuitarOutSource {
    Patch,
    Off,
    NormalPickup,
    Modeling,
    Both,
}

impl GuitarOutSource {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0 => GuitarOutSource::Patch,
            1 => GuitarOutSource::Off,
            2 => GuitarOutSource::NormalPickup,
            3 => GuitarOutSource::Modeling,
            4 => GuitarOutSource::Both,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        match self {
            GuitarOutSource::Patch => 0,
            GuitarOutSource::Off => 1,
            GuitarOutSource::NormalPickup => 2,
            GuitarOutSource::Modeling => 3,
            GuitarOutSource::Both => 4,
        }
    }
}

/// Master tune in Hz (435..=445). Wire encoding: 0 = 435 Hz, 10 = 445 Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct MasterTuneHz(u16);

impl MasterTuneHz {
    pub fn new(hz: u16) -> Option<Self> {
        if (435..=445).contains(&hz) {
            Some(MasterTuneHz(hz))
        } else {
            None
        }
    }
    pub fn get(self) -> u16 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        if b <= 10 {
            Some(MasterTuneHz(435 + u16::from(b)))
        } else {
            None
        }
    }
    fn to_byte(self) -> u8 {
        (self.0 - 435) as u8
    }
}

/// Encode an arbitrary `u8` as two consecutive 7-bit bytes using the
/// 4-nibble (BCD-like) scheme FloorBoard's `customDataKnob.cpp:106-128`
/// applies whenever the UI control type is `addDataKnob`:
/// `byte_hi = (value >> 4) & 0x0F`, `byte_lo = value & 0x0F`.
pub(crate) fn encode_nibble_pair(value: u8) -> [u8; 2] {
    [(value >> 4) & 0x0F, value & 0x0F]
}

/// Inverse of [`encode_nibble_pair`]. Returns `None` when either byte has
/// any of bits 4–7 set (which means it isn't a valid nibble half).
pub(crate) fn decode_nibble_pair(hi: u8, lo: u8) -> Option<u8> {
    if hi > 0x0F || lo > 0x0F {
        return None;
    }
    Some((hi << 4) | lo)
}

/// A 0..=200 audio level encoded on the wire as two consecutive 7-bit bytes
/// (MSB-first) using FloorBoard's `customKnob.cpp:112-117` scheme:
/// `byte_hi = value / 128`, `byte_lo = value % 128`. Used for Player Level,
/// USB Audio In, and USB Audio Out at offsets `0x1B/0x1D/0x1F` of MSB `0x02`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct AudioLevel(u8);

impl AudioLevel {
    pub fn new(value: u8) -> Option<Self> {
        if value <= 200 {
            Some(AudioLevel(value))
        } else {
            None
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
    fn from_two_bytes(hi: u8, lo: u8) -> Option<Self> {
        if hi > 1 || lo > 0x7F {
            return None;
        }
        let value = u16::from(hi) * 128 + u16::from(lo);
        if value > 200 {
            return None;
        }
        Some(AudioLevel(value as u8))
    }
    fn to_two_bytes(self) -> [u8; 2] {
        let v = u16::from(self.0);
        [(v / 128) as u8, (v % 128) as u8]
    }
}

/// CTL Pedal Function — 22 actions assignable to the CTL footswitch.
/// Mined from `midi.xml` `<PARAM value="00" customdesc="Function">` under
/// `<LSB value="02" name="page 2">` on MSB `0x02`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum CtlPedalFunction {
    Off,
    PatchSetting,
    Hold,
    TapTempo,
    ToneSw,
    AmpSw,
    ModSw,
    MfxSw,
    DelaySw,
    ReverbSw,
    ChorusSw,
    SoundStyleInc,
    SoundStyleDec,
    BankNumberInc,
    BankNumberDec,
    PatchNumberInc,
    PatchNumberDec,
    AudioPlayerPlayStop,
    AudioPlayerSongInc,
    AudioPlayerSongDec,
    AudioPlayerSw,
    VLinkSw,
}

impl CtlPedalFunction {
    fn from_byte(b: u8) -> Option<Self> {
        use CtlPedalFunction::*;
        Some(match b {
            0x00 => Off,
            0x01 => PatchSetting,
            0x02 => Hold,
            0x03 => TapTempo,
            0x04 => ToneSw,
            0x05 => AmpSw,
            0x06 => ModSw,
            0x07 => MfxSw,
            0x08 => DelaySw,
            0x09 => ReverbSw,
            0x0A => ChorusSw,
            0x0B => SoundStyleInc,
            0x0C => SoundStyleDec,
            0x0D => BankNumberInc,
            0x0E => BankNumberDec,
            0x0F => PatchNumberInc,
            0x10 => PatchNumberDec,
            0x11 => AudioPlayerPlayStop,
            0x12 => AudioPlayerSongInc,
            0x13 => AudioPlayerSongDec,
            0x14 => AudioPlayerSw,
            0x15 => VLinkSw,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        use CtlPedalFunction::*;
        match self {
            Off => 0x00,
            PatchSetting => 0x01,
            Hold => 0x02,
            TapTempo => 0x03,
            ToneSw => 0x04,
            AmpSw => 0x05,
            ModSw => 0x06,
            MfxSw => 0x07,
            DelaySw => 0x08,
            ReverbSw => 0x09,
            ChorusSw => 0x0A,
            SoundStyleInc => 0x0B,
            SoundStyleDec => 0x0C,
            BankNumberInc => 0x0D,
            BankNumberDec => 0x0E,
            PatchNumberInc => 0x0F,
            PatchNumberDec => 0x10,
            AudioPlayerPlayStop => 0x11,
            AudioPlayerSongInc => 0x12,
            AudioPlayerSongDec => 0x13,
            AudioPlayerSw => 0x14,
            VLinkSw => 0x15,
        }
    }
}

/// EXP Pedal Function — 11 actions assignable to the EXP pedal in either
/// EXP-Switch-OFF (`[02, 02, 0x0D]`) or EXP-Switch-ON (`[02, 02, 0x24]`)
/// state. Mined from `midi.xml` `<PARAM value="0D" customdesc="Function">`.
/// (FloorBoard's XML shows two entries for `Modulation` — values `0x05` and
/// `0x0A` — which appears intentional; preserving as `Modulation` and
/// `ModControl` to make the variants distinct in Rust.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum ExpPedalFunction {
    Off,
    PatchSetting,
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

impl ExpPedalFunction {
    fn from_byte(b: u8) -> Option<Self> {
        use ExpPedalFunction::*;
        Some(match b {
            0x00 => Off,
            0x01 => PatchSetting,
            0x02 => PatchVolume,
            0x03 => ToneVolume,
            0x04 => PitchBend,
            0x05 => Modulation,
            0x06 => CrossFader,
            0x07 => DelayLevel,
            0x08 => ReverbLevel,
            0x09 => ChorusLevel,
            0x0A => ModControl,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        use ExpPedalFunction::*;
        match self {
            Off => 0x00,
            PatchSetting => 0x01,
            PatchVolume => 0x02,
            ToneVolume => 0x03,
            PitchBend => 0x04,
            Modulation => 0x05,
            CrossFader => 0x06,
            DelayLevel => 0x07,
            ReverbLevel => 0x08,
            ChorusLevel => 0x09,
            ModControl => 0x0A,
        }
    }
}

/// EXP Pedal SWITCH Function — 21 actions assignable to the EXP pedal's
/// onboard switch (separate from the EXP pedal sweep). Mined from
/// `midi.xml:3329` `<PARAM value="3B" abbr="EXP SW" customdesc="Function">`.
/// Differs from `CtlPedalFunction` only in that there's no `Hold` action
/// (a momentary footswitch can't hold).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum ExpPedalSwitchFunction {
    Off,
    PatchSetting,
    TapTempo,
    ToneSw,
    AmpSw,
    ModSw,
    MfxSw,
    DelaySw,
    ReverbSw,
    ChorusSw,
    SoundStyleInc,
    SoundStyleDec,
    BankNumberInc,
    BankNumberDec,
    PatchNumberInc,
    PatchNumberDec,
    AudioPlayerPlayStop,
    AudioPlayerSongInc,
    AudioPlayerSongDec,
    AudioPlayerSw,
    VLinkSw,
}

impl ExpPedalSwitchFunction {
    fn from_byte(b: u8) -> Option<Self> {
        use ExpPedalSwitchFunction::*;
        Some(match b {
            0x00 => Off,
            0x01 => PatchSetting,
            0x02 => TapTempo,
            0x03 => ToneSw,
            0x04 => AmpSw,
            0x05 => ModSw,
            0x06 => MfxSw,
            0x07 => DelaySw,
            0x08 => ReverbSw,
            0x09 => ChorusSw,
            0x0A => SoundStyleInc,
            0x0B => SoundStyleDec,
            0x0C => BankNumberInc,
            0x0D => BankNumberDec,
            0x0E => PatchNumberInc,
            0x0F => PatchNumberDec,
            0x10 => AudioPlayerPlayStop,
            0x11 => AudioPlayerSongInc,
            0x12 => AudioPlayerSongDec,
            0x13 => AudioPlayerSw,
            0x14 => VLinkSw,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        use ExpPedalSwitchFunction::*;
        match self {
            Off => 0x00,
            PatchSetting => 0x01,
            TapTempo => 0x02,
            ToneSw => 0x03,
            AmpSw => 0x04,
            ModSw => 0x05,
            MfxSw => 0x06,
            DelaySw => 0x07,
            ReverbSw => 0x08,
            ChorusSw => 0x09,
            SoundStyleInc => 0x0A,
            SoundStyleDec => 0x0B,
            BankNumberInc => 0x0C,
            BankNumberDec => 0x0D,
            PatchNumberInc => 0x0E,
            PatchNumberDec => 0x0F,
            AudioPlayerPlayStop => 0x10,
            AudioPlayerSongInc => 0x11,
            AudioPlayerSongDec => 0x12,
            AudioPlayerSw => 0x13,
            VLinkSw => 0x14,
        }
    }
}

/// Pitch Bend depth in semitones, range `-12..=+12`. Wire encoding offsets
/// by 24 (byte `0x18` = 0). Used for EXP-OFF and EXP-ON Pitch Bend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct PitchBendDepth(i8);

impl PitchBendDepth {
    pub fn new(semitones: i8) -> Option<Self> {
        if (-12..=12).contains(&semitones) {
            Some(PitchBendDepth(semitones))
        } else {
            None
        }
    }
    pub fn get(self) -> i8 {
        self.0
    }
    pub(crate) fn from_byte(b: u8) -> Option<Self> {
        if (0x0C..=0x24).contains(&b) {
            Some(PitchBendDepth((b as i8) - 24))
        } else {
            None
        }
    }
    pub(crate) fn to_byte(self) -> u8 {
        (self.0 + 24) as u8
    }
}

/// CTL Pedal Hold Type — 4 latch / momentary variants.
/// Mined from midi.xml:3087-3092 `<PARAM value="01" customdesc="Hold Type">`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum HoldType {
    Type1,
    Type2,
    Type3,
    Type4,
}

impl HoldType {
    pub(crate) fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0 => HoldType::Type1,
            1 => HoldType::Type2,
            2 => HoldType::Type3,
            3 => HoldType::Type4,
            _ => return None,
        })
    }
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            HoldType::Type1 => 0,
            HoldType::Type2 => 1,
            HoldType::Type3 => 2,
            HoldType::Type4 => 3,
        }
    }
}

/// CTL Pedal Switch Mode (Latch / Moment).
/// Mined from midi.xml:3093-3096 `<PARAM value="02" customdesc="Switch Mode">`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum SwitchMode {
    Latch,
    Moment,
}

impl SwitchMode {
    pub(crate) fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(SwitchMode::Latch),
            1 => Some(SwitchMode::Moment),
            _ => None,
        }
    }
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            SwitchMode::Latch => 0,
            SwitchMode::Moment => 1,
        }
    }
}

/// Master Patch Level (0..=200). Wire encoding is 4-nibble (NOT the 14-bit
/// scheme used by the Player / USB audio levels): the high nibble of the
/// value lands at `[02, 00, 0x30]` and the low nibble at `[02, 00, 0x31]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct PatchLevel(u8);

impl PatchLevel {
    pub fn new(value: u8) -> Option<Self> {
        if value <= 200 {
            Some(PatchLevel(value))
        } else {
            None
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
    pub(crate) fn from_two_bytes(hi: u8, lo: u8) -> Option<Self> {
        let v = decode_nibble_pair(hi, lo)?;
        if v > 200 {
            return None;
        }
        Some(PatchLevel(v))
    }
    pub(crate) fn to_two_bytes(self) -> [u8; 2] {
        encode_nibble_pair(self.0)
    }
}

/// Master tempo (BPM). 4-nibble 2-byte encoding at `[02, 00, 0x3C..0x3D]`.
/// The valid range isn't pinned down yet — it depends on a lookup against
/// FloorBoard's `<Tables>` range tables that I haven't reproduced; the type
/// accepts the full `0..=255` the 4-nibble encoding can carry. Real-device
/// behavior may reject values outside the GR-55's documented range
/// (Roland devices typically support 40..=250 BPM for master tempo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct MasterBpm(u8);

impl MasterBpm {
    /// Raw wire byte, 0..=255.
    pub fn new(value: u8) -> Self {
        MasterBpm(value)
    }
    /// Raw wire byte.
    pub fn raw(self) -> u8 {
        self.0
    }
    /// Integer FloorBoard's UI prints on the BPM knob — `1536 + raw`,
    /// range 1536..=1791. Not BPM in beats-per-minute; the Roland-internal
    /// interpretation isn't documented in FloorBoard itself.
    pub fn display_value(self) -> u16 {
        1536 + u16::from(self.0)
    }
    fn from_two_bytes(hi: u8, lo: u8) -> Option<Self> {
        decode_nibble_pair(hi, lo).map(MasterBpm)
    }
    fn to_two_bytes(self) -> [u8; 2] {
        encode_nibble_pair(self.0)
    }
}

/// Guitar/Bass mode (same enum as patch byte 0; reused for system Startup Mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Guitar,
    Bass,
}

impl Mode {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Mode::Guitar),
            1 => Some(Mode::Bass),
            _ => None,
        }
    }
    fn to_byte(self) -> u8 {
        match self {
            Mode::Guitar => 0,
            Mode::Bass => 1,
        }
    }
}

/// Typed view of the GR-55's System area state.
///
/// `unknown_bytes` captures every address that landed in the dump but isn't
/// yet modeled as a typed field — this keeps round-trip lossless before each
/// field gets an enum or struct.
/// One of the GR-55's 10 GK Pickup setups. Lives at sub-LSB `0x04 + index`
/// of MSB `0x02`.
///
/// Per-setup parameters lifted from `midi.xml:3641-…` under
/// `<LSB value="04" name="GK setup 1">`. The 8-char name owns offsets
/// `0x00..=0x07`; typed pickup-configuration fields cover `0x08..=0x0E`.
/// Remaining bytes round-trip verbatim through `raw_bytes` until each gets
/// promoted to its own typed accessor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case", default)]
pub struct GkSetup {
    /// Setup name. 8 ASCII chars (`abbr="Name1"`..`"Name8"` at offsets
    /// `0x00..=0x07` per midi.xml:3642-4666).
    #[serde(default, skip_serializing_if = "GkSetupName::is_empty")]
    pub name: GkSetupName,

    /// Hex Pickup model at offset `0x08`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hex_pu_type: Option<HexPuType>,
    /// `Scale` parameter at offset `0x09` — raw byte; midi.xml describes it
    /// as a `customdesc="Scale"` numeric range without enumerated names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<u8>,
    /// `LSB` parameter at offset `0x0A` — raw byte; midi.xml leaves the
    /// enum/range undocumented.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsb: Option<u8>,
    /// Hex Pickup phase at offset `0x0B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pu_phase: Option<PuPhase>,
    /// Hex Pickup direction at offset `0x0C`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pu_direction: Option<PuDirection>,
    /// GK switch S1 / S2 position at offset `0x0D`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s1_s2_position: Option<S1S2Position>,
    /// Normal Pickup Gain at offset `0x0E` (-20..=+20 dB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal_pu_gain: Option<NormalPuGain>,
    /// Piezo Low gain at offset `0x0F` (-10..=+10 dB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub piezo_low: Option<PiezoGain>,
    /// Piezo High gain at offset `0x10`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub piezo_high: Option<PiezoGain>,
    /// Hex pickup-bridge distance per string (`0x11..=0x16`, strings 1..6).
    /// midi.xml documents this as a numeric range but doesn't name the units;
    /// stored raw until verified.
    #[serde(default, skip_serializing_if = "string_distance_all_none")]
    pub string_distance_bridge: [Option<u8>; 6],
    /// Per-string sensitivity (`0x17..=0x1C`, strings 1..6). Raw byte.
    #[serde(default, skip_serializing_if = "string_distance_all_none")]
    pub string_sensitivity: [Option<u8>; 6],
    /// Play Feel at offset `0x1D` (1..=5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_feel: Option<PlayFeel>,
    /// Low velocity cut at offset `0x1E` (0..=10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_velocity_cut: Option<Rating0To10>,
    /// Velocity dynamics at offset `0x1F` (1..=10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity_dynamics: Option<Rating1To10>,
    /// Nuance dynamics at offset `0x20` (0..=10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nuance_dynamics: Option<Rating0To10>,
    /// Nuance trim at offset `0x21` (0..=10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nuance_trim: Option<Rating0To10>,
    /// Down shift at offset `0x22` (-5..=0 semitones).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down_shift: Option<DownShift>,

    // ---- Bass Mode parallel configuration (offsets 0x23..0x40) ----
    /// Bass Mode setup name (8 ASCII chars, offsets `0x23..=0x2A`).
    #[serde(default, skip_serializing_if = "GkSetupName::is_empty")]
    pub bass_name: GkSetupName,
    /// Bass-mode Hex Pickup at `0x2B`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_hex_pu_type: Option<BassHexPuType>,
    /// Bass Scale at `0x2C` (raw 0..=15 per midi.xml range; stored u8).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_scale: Option<u8>,
    /// Bass LSB at `0x2D` (raw 0..=15 per midi.xml range).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_lsb: Option<u8>,
    /// GK pickup position at `0x2E` — Bass-Mode-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_gk_pu_position: Option<GkPuPosition>,
    /// Bass PU phase at `0x2F`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_pu_phase: Option<PuPhase>,
    /// Bass PU direction at `0x30`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_pu_direction: Option<PuDirection>,
    /// Bass S1/S2 position at `0x31`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_s1_s2_position: Option<S1S2Position>,
    /// Bass Normal PU Gain at `0x32` (-20..=+20 dB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_normal_pu_gain: Option<NormalPuGain>,
    /// Bass Piezo Low gain at `0x33`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_piezo_low: Option<PiezoGain>,
    /// Bass Piezo High gain at `0x34`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_piezo_high: Option<PiezoGain>,
    /// Bass per-string distance from bridge at `0x35..=0x3A` (strings 1..=6).
    #[serde(default, skip_serializing_if = "string_distance_all_none")]
    pub bass_string_distance_bridge: [Option<u8>; 6],
    /// Bass per-string sensitivity at `0x3B..=0x40` (strings 1..=6).
    #[serde(default, skip_serializing_if = "string_distance_all_none")]
    pub bass_string_sensitivity: [Option<u8>; 6],
    /// Bass Play Feel at `0x41` (1..=5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_play_feel: Option<PlayFeel>,
    /// Bass Low Velocity Cut at `0x42` (0..=10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_low_velocity_cut: Option<Rating0To10>,
    /// Bass Velocity Dynamics at `0x43` (1..=10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_velocity_dynamics: Option<Rating1To10>,
    /// Bass Nuance Dynamics at `0x44` (0..=10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_nuance_dynamics: Option<Rating0To10>,
    /// Bass Nuance Trim at `0x45` (0..=10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_nuance_trim: Option<Rating0To10>,
    /// Bass Down Shift at `0x46` (-5..=0 semitones).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_down_shift: Option<DownShift>,

    // ---- Develop Low Cut (per-mode tail at 0x51/0x52) ----
    /// Guitar Mode "DevelopLowCut" at `0x51` (raw 0..=100 per midi.xml range).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub develop_low_cut_guitar: Option<u8>,
    /// Bass Mode "DevelopLowCut" at `0x52` (raw 0..=100 per midi.xml range).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub develop_low_cut_bass: Option<u8>,

    /// Every byte at offset `0x47..=0x50` or `>= 0x53` that isn't yet typed.
    /// Round-trip stays lossless even before each follow-up parameter gets
    /// promoted.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw_bytes: BTreeMap<u8, u8>,
}

/// Fixed-length 8-char ASCII name (printable range `0x20..=0x7E`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct GkSetupName(pub [u8; 8]);

impl Default for GkSetupName {
    fn default() -> Self {
        GkSetupName([0x20; 8])
    }
}

impl GkSetupName {
    pub const LEN: usize = 8;

    /// True when every character is the space pad byte `0x20` (the default).
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&b| b == 0x20)
    }

    /// Return the name as a `String`, replacing non-printable bytes with `?`.
    pub fn as_string(&self) -> String {
        self.0
            .iter()
            .map(|&b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    '?'
                }
            })
            .collect()
    }
}

impl std::str::FromStr for GkSetupName {
    type Err = GkSetupNameError;

    /// Pad shorter inputs with spaces, truncate longer ones. Returns `Err`
    /// for any character outside printable ASCII `0x20..=0x7E`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut out = [0x20_u8; 8];
        for (i, ch) in s.chars().take(8).enumerate() {
            let code = u32::from(ch);
            if !(0x20..=0x7E).contains(&code) {
                return Err(GkSetupNameError::NonPrintable {
                    char_index: i,
                    code,
                });
            }
            out[i] = code as u8;
        }
        Ok(GkSetupName(out))
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GkSetupNameError {
    #[error("non-printable character at index {char_index}: codepoint {code:#x}")]
    NonPrintable { char_index: usize, code: u32 },
}

/// Hex Pickup model — mined from midi.xml:4666-4673
/// `<PARAM value="08" abbr="Guitar Mode" customdesc="Hex PU Type">`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum HexPuType {
    Gk3,
    Gk2a,
    PiezoFlat,
    PiezoFishman,
    PiezoGraphtech,
    PiezoLrBaggs,
    PiezoRmc,
}

impl HexPuType {
    fn from_byte(b: u8) -> Option<Self> {
        use HexPuType::*;
        Some(match b {
            0x00 => Gk3,
            0x01 => Gk2a,
            0x02 => PiezoFlat,
            0x03 => PiezoFishman,
            0x04 => PiezoGraphtech,
            0x05 => PiezoLrBaggs,
            0x06 => PiezoRmc,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        use HexPuType::*;
        match self {
            Gk3 => 0x00,
            Gk2a => 0x01,
            PiezoFlat => 0x02,
            PiezoFishman => 0x03,
            PiezoGraphtech => 0x04,
            PiezoLrBaggs => 0x05,
            PiezoRmc => 0x06,
        }
    }
}

/// Hex Pickup phase — midi.xml:4681 `customdesc="PU Phase"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum PuPhase {
    Normal,
    Inverse,
}

impl PuPhase {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(PuPhase::Normal),
            1 => Some(PuPhase::Inverse),
            _ => None,
        }
    }
    fn to_byte(self) -> u8 {
        match self {
            PuPhase::Normal => 0,
            PuPhase::Inverse => 1,
        }
    }
}

/// Hex Pickup direction — midi.xml:4685 `customdesc="PU Direction"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum PuDirection {
    Normal,
    Reverse,
}

impl PuDirection {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(PuDirection::Normal),
            1 => Some(PuDirection::Reverse),
            _ => None,
        }
    }
    fn to_byte(self) -> u8 {
        match self {
            PuDirection::Normal => 0,
            PuDirection::Reverse => 1,
        }
    }
}

/// GK switch S1 / S2 position — midi.xml:4689 `customdesc="S1/S2 Position"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum S1S2Position {
    Normal,
    Reverse,
}

impl S1S2Position {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(S1S2Position::Normal),
            1 => Some(S1S2Position::Reverse),
            _ => None,
        }
    }
    fn to_byte(self) -> u8 {
        match self {
            S1S2Position::Normal => 0,
            S1S2Position::Reverse => 1,
        }
    }
}

/// Normal Pickup Gain in dB, range `-20..=+20`. Wire byte = `dB + 20`
/// (midi.xml:4693-4734 — `<PARAM value="0E" customdesc="Normal PU Gain">`
/// enumerates 41 values from `-20 dB` at byte `0x00` to `+20 dB` at `0x28`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct NormalPuGain(i8);

impl NormalPuGain {
    pub fn new(db: i8) -> Option<Self> {
        if (-20..=20).contains(&db) {
            Some(NormalPuGain(db))
        } else {
            None
        }
    }
    pub fn db(self) -> i8 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        if b <= 40 {
            Some(NormalPuGain((b as i8) - 20))
        } else {
            None
        }
    }
    fn to_byte(self) -> u8 {
        (self.0 + 20) as u8
    }
}

/// Piezo Low / Piezo High pickup gain in dB, `-10..=+10`. Wire byte = `dB + 10`.
/// midi.xml:4736-4781.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct PiezoGain(i8);

impl PiezoGain {
    pub fn new(db: i8) -> Option<Self> {
        if (-10..=10).contains(&db) {
            Some(PiezoGain(db))
        } else {
            None
        }
    }
    pub fn db(self) -> i8 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        if b <= 20 {
            Some(PiezoGain((b as i8) - 10))
        } else {
            None
        }
    }
    fn to_byte(self) -> u8 {
        (self.0 + 10) as u8
    }
}

/// Play feel — 1..=5. midi.xml:5058-5063.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct PlayFeel(u8);

impl PlayFeel {
    pub fn new(value: u8) -> Option<Self> {
        if (1..=5).contains(&value) {
            Some(PlayFeel(value))
        } else {
            None
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        if b <= 4 {
            Some(PlayFeel(b + 1))
        } else {
            None
        }
    }
    fn to_byte(self) -> u8 {
        self.0 - 1
    }
}

/// 0..=10 rating (wire = user value). Shared by Low velocity cut, Nuance
/// dynamics, Nuance trim per midi.xml:5065-5114.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct Rating0To10(u8);

impl Rating0To10 {
    pub fn new(value: u8) -> Option<Self> {
        if value <= 10 {
            Some(Rating0To10(value))
        } else {
            None
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        Self::new(b)
    }
    fn to_byte(self) -> u8 {
        self.0
    }
}

/// 1..=10 rating. Used by Velocity dynamics per midi.xml:5078-5088.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct Rating1To10(u8);

impl Rating1To10 {
    pub fn new(value: u8) -> Option<Self> {
        if (1..=10).contains(&value) {
            Some(Rating1To10(value))
        } else {
            None
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        Self::new(b)
    }
    fn to_byte(self) -> u8 {
        self.0
    }
}

/// Down Shift in semitones, `-5..=0`. Wire byte = `-value`. midi.xml:5116-5122.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(transparent)]
pub struct DownShift(i8);

impl DownShift {
    pub fn new(semitones: i8) -> Option<Self> {
        if (-5..=0).contains(&semitones) {
            Some(DownShift(semitones))
        } else {
            None
        }
    }
    pub fn semitones(self) -> i8 {
        self.0
    }
    fn from_byte(b: u8) -> Option<Self> {
        if b <= 5 {
            Some(DownShift(-(b as i8)))
        } else {
            None
        }
    }
    fn to_byte(self) -> u8 {
        (-self.0) as u8
    }
}

/// Bass-mode Hex Pickup model. Distinct from `HexPuType` (Guitar Mode) —
/// only 5 entries, omits Piezo Fishman and Piezo L.R. Baggs.
/// midi.xml:6148-6153 `<PARAM value="2B" abbr="Bass Mode" customdesc="Hex PU Type">`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum BassHexPuType {
    Gk3b,
    Gk2b,
    PiezoFlat,
    PiezoGraphtech,
    PiezoRmc,
}

impl BassHexPuType {
    fn from_byte(b: u8) -> Option<Self> {
        use BassHexPuType::*;
        Some(match b {
            0x00 => Gk3b,
            0x01 => Gk2b,
            0x02 => PiezoFlat,
            0x03 => PiezoGraphtech,
            0x04 => PiezoRmc,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        use BassHexPuType::*;
        match self {
            Gk3b => 0x00,
            Gk2b => 0x01,
            PiezoFlat => 0x02,
            PiezoGraphtech => 0x03,
            PiezoRmc => 0x04,
        }
    }
}

/// GK pickup position — Bass-Mode-only; selects how the pickup is aligned
/// across the strings on the bass. midi.xml:6161-6169 (8 variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum GkPuPosition {
    FourString1,
    FourString2,
    FourString3,
    FiveStringLow1,
    FiveStringLow2,
    FiveStringHigh1,
    FiveStringHigh2,
    SixString,
}

impl GkPuPosition {
    fn from_byte(b: u8) -> Option<Self> {
        use GkPuPosition::*;
        Some(match b {
            0x00 => FourString1,
            0x01 => FourString2,
            0x02 => FourString3,
            0x03 => FiveStringLow1,
            0x04 => FiveStringLow2,
            0x05 => FiveStringHigh1,
            0x06 => FiveStringHigh2,
            0x07 => SixString,
            _ => return None,
        })
    }
    fn to_byte(self) -> u8 {
        use GkPuPosition::*;
        match self {
            FourString1 => 0x00,
            FourString2 => 0x01,
            FourString3 => 0x02,
            FiveStringLow1 => 0x03,
            FiveStringLow2 => 0x04,
            FiveStringHigh1 => 0x05,
            FiveStringHigh2 => 0x06,
            SixString => 0x07,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify_next::Tsify))]
#[serde(rename_all = "snake_case", default)]
pub struct SystemArea {
    /// Currently-selected patch, encoded on the wire as two consecutive 7-bit
    /// bytes (14-bit MSB-first) — same scheme as the audio levels. Decoded via
    /// [`PatchSlot::from_linear_index`]; the gap between USER (indices 0..=296)
    /// and PRESET (indices 384..) yields `None` since it represents reserved
    /// `void` slots that don't correspond to a real patch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_patch: Option<PatchSlot>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_set: Option<GkSet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_select: Option<OutputSelect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assign_hold: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_channel: Option<MidiChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pc_rx: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pc_tx: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_tx_channel: Option<MidiChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guitar_midi_out: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_out_mode: Option<MidiOutMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chromatic: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string_channel_base: Option<StringChannelBase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_thin: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_pedal_cc: Option<PedalCc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pedal_cc: Option<PedalCc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pedal_bend: Option<ExpPedalBend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_vol_cc: Option<PedalCc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_cc: Option<PedalCc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_cc: Option<PedalCc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_map: Option<MidiMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_direct: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guitar_out_source: Option<GuitarOutSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_tune: Option<MasterTuneHz>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tuner_mute: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_mode: Option<Mode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_level: Option<AudioLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usb_audio_in_level: Option<AudioLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usb_audio_out_level: Option<AudioLevel>,
    /// Master "Patch Level" knob — 4-nibble 2-byte encoding, range 0..=200.
    /// Distinct from the 14-bit `AudioLevel` used for the Player / USB knobs;
    /// FloorBoard's `addDataKnob` (Master menu) uses BCD-like packing while
    /// its `addKnob` (System menu) uses MSB-first 14-bit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_level: Option<PatchLevel>,
    /// Master BPM (tempo). 4-nibble 2-byte encoding; exact valid range TBD.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_bpm: Option<MasterBpm>,
    /// CTL footswitch function assignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_pedal_function: Option<CtlPedalFunction>,
    /// EXP Pedal function when the EXP Pedal Switch is OFF.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pedal_off_function: Option<ExpPedalFunction>,
    /// EXP Pedal function when the EXP Pedal Switch is ON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pedal_on_function: Option<ExpPedalFunction>,
    /// Action bound to the EXP pedal's onboard switch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_pedal_switch_function: Option<ExpPedalSwitchFunction>,

    // ---- CTL Pedal page 2 sub-parameters (active per CTL function) ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_hold_type: Option<HoldType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_switch_mode: Option<SwitchMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_hold_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_hold_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_off_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_off_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_off_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_off_normal_pu: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_on_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_on_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_on_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctl_tone_sw_on_normal_pu: Option<OnOff>,

    // ---- Master menu remainder (single-byte values; enums not in midi.xml) ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_set_select: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guitar_out: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_palette: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_clip: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_note_clip_change: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_exp: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_exp_on: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v_link_gk_vol: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_tuning_sw: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_tuning_type: Option<u8>,
    /// User Tuning Shift, one byte per string (index 0 = string 1).
    /// Roland devices typically encode this as a signed offset; the exact
    /// byte ranges aren't documented in `midi.xml`, so stored raw.
    #[serde(skip_serializing_if = "user_tuning_shift_all_none", default)]
    pub user_tuning_shift_strings: [Option<u8>; 6],

    // ---- EXP Pedal (Switch OFF) page 2 sub-parameters ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_tone_vol_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_tone_vol_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_tone_vol_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_tone_vol_normal_pu: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_pitch_bend_depth: Option<PitchBendDepth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_pitch_bend_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_pitch_bend_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_pitch_bend_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_mod_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_mod_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_mod_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_mod_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_cross_fader_a: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_cross_fader_b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_cross_fader_c: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_cross_fader_d: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_delay_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_delay_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_reverb_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_reverb_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_chorus_min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_chorus_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_mod_control_a: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_off_mod_control_b: Option<u8>,

    // ---- EXP Pedal (Switch ON) page 2 sub-parameters ----
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
    pub exp_on_cross_fader_a: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_cross_fader_b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_cross_fader_c: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_cross_fader_d: Option<u8>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_mod_control_a: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_on_mod_control_b: Option<u8>,

    // ---- EXP Pedal SWITCH Tone-Sw sub-parameters ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_off_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_off_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_off_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_off_normal_pu: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_on_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_on_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_on_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_sw_tone_sw_on_normal_pu: Option<OnOff>,

    // ---- GK S1 / GK S2 footswitch discriminators + Tone-Sw sub-fields ----
    /// GK S1 footswitch function (same 21-variant enum as EXP SWITCH).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_function: Option<ExpPedalSwitchFunction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_off_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_off_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_off_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_off_normal_pu: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_on_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_on_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_on_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s1_tone_sw_on_normal_pu: Option<OnOff>,
    /// GK S2 footswitch function (same 21-variant enum as EXP SWITCH).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_function: Option<ExpPedalSwitchFunction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_off_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_off_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_off_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_off_normal_pu: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_on_pcm_1: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_on_pcm_2: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_on_modeling: Option<OnOff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gk_s2_tone_sw_on_normal_pu: Option<OnOff>,

    /// GK VOL pedal Mod Control min (0..=120 range per midi.xml).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vol_mod_control_a: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vol_mod_control_b: Option<u8>,

    /// The 10 GK Pickup setups at sub-LSBs `[02, 0x04..=0x0D, *]`. Each entry
    /// is `Some` only when at least one of its bytes was present in the
    /// decoded dump. Order: index 0 = setup 1, …, index 9 = setup 10.
    #[serde(default, skip_serializing_if = "all_gk_setups_none")]
    pub gk_setups: [Option<GkSetup>; 10],

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

        out.current_patch =
            consume_current_patch(&mut bytes, ADDR_CURRENT_PATCH_HI, ADDR_CURRENT_PATCH_LO);

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
        out.exp_pedal_bend =
            take(&mut bytes, ADDR_EXP_PEDAL_BEND).and_then(ExpPedalBend::from_byte);
        out.gk_vol_cc = take(&mut bytes, ADDR_GK_VOL_CC).and_then(PedalCc::from_byte);
        out.gk_s1_cc = take(&mut bytes, ADDR_GK_S1_CC).and_then(PedalCc::from_byte);
        out.gk_s2_cc = take(&mut bytes, ADDR_GK_S2_CC).and_then(PedalCc::from_byte);
        out.midi_map = take(&mut bytes, ADDR_MIDI_MAP).and_then(MidiMap::from_byte);
        out.monitor_direct = take(&mut bytes, ADDR_MONITOR_DIRECT).and_then(OnOff::from_byte);
        out.guitar_out_source =
            take(&mut bytes, ADDR_GUITAR_OUT_SOURCE).and_then(GuitarOutSource::from_byte);
        out.master_tune = take(&mut bytes, ADDR_MASTER_TUNE).and_then(MasterTuneHz::from_byte);
        out.tuner_mute = take(&mut bytes, ADDR_TUNER_MUTE).and_then(OnOff::from_byte);
        out.startup_mode = take(&mut bytes, ADDR_STARTUP_MODE).and_then(Mode::from_byte);

        out.player_level =
            consume_audio_level(&mut bytes, ADDR_PLAYER_LEVEL_HI, ADDR_PLAYER_LEVEL_LO);
        out.usb_audio_in_level =
            consume_audio_level(&mut bytes, ADDR_USB_AUDIO_IN_HI, ADDR_USB_AUDIO_IN_LO);
        out.usb_audio_out_level =
            consume_audio_level(&mut bytes, ADDR_USB_AUDIO_OUT_HI, ADDR_USB_AUDIO_OUT_LO);
        out.patch_level = consume_two_byte(
            &mut bytes,
            ADDR_PATCH_LEVEL_HI,
            ADDR_PATCH_LEVEL_LO,
            PatchLevel::from_two_bytes,
        );
        out.master_bpm = consume_two_byte(
            &mut bytes,
            ADDR_MASTER_BPM_HI,
            ADDR_MASTER_BPM_LO,
            MasterBpm::from_two_bytes,
        );
        out.ctl_pedal_function =
            take(&mut bytes, ADDR_CTL_PEDAL_FUNCTION).and_then(CtlPedalFunction::from_byte);
        out.exp_pedal_off_function =
            take(&mut bytes, ADDR_EXP_PEDAL_OFF_FUNCTION).and_then(ExpPedalFunction::from_byte);
        out.exp_pedal_on_function =
            take(&mut bytes, ADDR_EXP_PEDAL_ON_FUNCTION).and_then(ExpPedalFunction::from_byte);
        out.exp_pedal_switch_function = take(&mut bytes, ADDR_EXP_PEDAL_SWITCH_FUNCTION)
            .and_then(ExpPedalSwitchFunction::from_byte);

        out.ctl_hold_type = take(&mut bytes, ADDR_CTL_HOLD_TYPE).and_then(HoldType::from_byte);
        out.ctl_switch_mode =
            take(&mut bytes, ADDR_CTL_SWITCH_MODE).and_then(SwitchMode::from_byte);
        out.ctl_hold_pcm_1 = take(&mut bytes, ADDR_CTL_HOLD_PCM_1).and_then(OnOff::from_byte);
        out.ctl_hold_pcm_2 = take(&mut bytes, ADDR_CTL_HOLD_PCM_2).and_then(OnOff::from_byte);
        out.ctl_tone_sw_off_pcm_1 =
            take(&mut bytes, ADDR_CTL_TONE_SW_OFF_PCM_1).and_then(OnOff::from_byte);
        out.ctl_tone_sw_off_pcm_2 =
            take(&mut bytes, ADDR_CTL_TONE_SW_OFF_PCM_2).and_then(OnOff::from_byte);
        out.ctl_tone_sw_off_modeling =
            take(&mut bytes, ADDR_CTL_TONE_SW_OFF_MODELING).and_then(OnOff::from_byte);
        out.ctl_tone_sw_off_normal_pu =
            take(&mut bytes, ADDR_CTL_TONE_SW_OFF_NORMAL_PU).and_then(OnOff::from_byte);
        out.ctl_tone_sw_on_pcm_1 =
            take(&mut bytes, ADDR_CTL_TONE_SW_ON_PCM_1).and_then(OnOff::from_byte);
        out.ctl_tone_sw_on_pcm_2 =
            take(&mut bytes, ADDR_CTL_TONE_SW_ON_PCM_2).and_then(OnOff::from_byte);
        out.ctl_tone_sw_on_modeling =
            take(&mut bytes, ADDR_CTL_TONE_SW_ON_MODELING).and_then(OnOff::from_byte);
        out.ctl_tone_sw_on_normal_pu =
            take(&mut bytes, ADDR_CTL_TONE_SW_ON_NORMAL_PU).and_then(OnOff::from_byte);

        out.exp_off_tone_vol_pcm_1 =
            take(&mut bytes, ADDR_EXP_OFF_TONE_VOL_PCM_1).and_then(OnOff::from_byte);
        out.exp_off_tone_vol_pcm_2 =
            take(&mut bytes, ADDR_EXP_OFF_TONE_VOL_PCM_2).and_then(OnOff::from_byte);
        out.exp_off_tone_vol_modeling =
            take(&mut bytes, ADDR_EXP_OFF_TONE_VOL_MODELING).and_then(OnOff::from_byte);
        out.exp_off_tone_vol_normal_pu =
            take(&mut bytes, ADDR_EXP_OFF_TONE_VOL_NORMAL_PU).and_then(OnOff::from_byte);
        out.exp_off_pitch_bend_depth =
            take(&mut bytes, ADDR_EXP_OFF_PITCH_BEND_DEPTH).and_then(PitchBendDepth::from_byte);
        out.exp_off_pitch_bend_pcm_1 =
            take(&mut bytes, ADDR_EXP_OFF_PITCH_BEND_PCM_1).and_then(OnOff::from_byte);
        out.exp_off_pitch_bend_pcm_2 =
            take(&mut bytes, ADDR_EXP_OFF_PITCH_BEND_PCM_2).and_then(OnOff::from_byte);
        out.exp_off_pitch_bend_modeling =
            take(&mut bytes, ADDR_EXP_OFF_PITCH_BEND_MODELING).and_then(OnOff::from_byte);
        out.exp_off_mod_min = take(&mut bytes, ADDR_EXP_OFF_MOD_MIN);
        out.exp_off_mod_max = take(&mut bytes, ADDR_EXP_OFF_MOD_MAX);
        out.exp_off_mod_pcm_1 = take(&mut bytes, ADDR_EXP_OFF_MOD_PCM_1).and_then(OnOff::from_byte);
        out.exp_off_mod_pcm_2 = take(&mut bytes, ADDR_EXP_OFF_MOD_PCM_2).and_then(OnOff::from_byte);
        out.exp_off_cross_fader_a = take(&mut bytes, ADDR_EXP_OFF_CROSS_FADER_A);
        out.exp_off_cross_fader_b = take(&mut bytes, ADDR_EXP_OFF_CROSS_FADER_B);
        out.exp_off_cross_fader_c = take(&mut bytes, ADDR_EXP_OFF_CROSS_FADER_C);
        out.exp_off_cross_fader_d = take(&mut bytes, ADDR_EXP_OFF_CROSS_FADER_D);
        out.exp_off_delay_min = take(&mut bytes, ADDR_EXP_OFF_DELAY_MIN);
        out.exp_off_delay_max = take(&mut bytes, ADDR_EXP_OFF_DELAY_MAX);
        out.exp_off_reverb_min = take(&mut bytes, ADDR_EXP_OFF_REVERB_MIN);
        out.exp_off_reverb_max = take(&mut bytes, ADDR_EXP_OFF_REVERB_MAX);
        out.exp_off_chorus_min = take(&mut bytes, ADDR_EXP_OFF_CHORUS_MIN);
        out.exp_off_chorus_max = take(&mut bytes, ADDR_EXP_OFF_CHORUS_MAX);
        out.exp_off_mod_control_a = take(&mut bytes, ADDR_EXP_OFF_MOD_CONTROL_A);
        out.exp_off_mod_control_b = take(&mut bytes, ADDR_EXP_OFF_MOD_CONTROL_B);

        out.exp_on_tone_vol_pcm_1 =
            take(&mut bytes, ADDR_EXP_ON_TONE_VOL_PCM_1).and_then(OnOff::from_byte);
        out.exp_on_tone_vol_pcm_2 =
            take(&mut bytes, ADDR_EXP_ON_TONE_VOL_PCM_2).and_then(OnOff::from_byte);
        out.exp_on_tone_vol_modeling =
            take(&mut bytes, ADDR_EXP_ON_TONE_VOL_MODELING).and_then(OnOff::from_byte);
        out.exp_on_tone_vol_normal_pu =
            take(&mut bytes, ADDR_EXP_ON_TONE_VOL_NORMAL_PU).and_then(OnOff::from_byte);
        out.exp_on_pitch_bend_depth =
            take(&mut bytes, ADDR_EXP_ON_PITCH_BEND_DEPTH).and_then(PitchBendDepth::from_byte);
        out.exp_on_pitch_bend_pcm_1 =
            take(&mut bytes, ADDR_EXP_ON_PITCH_BEND_PCM_1).and_then(OnOff::from_byte);
        out.exp_on_pitch_bend_pcm_2 =
            take(&mut bytes, ADDR_EXP_ON_PITCH_BEND_PCM_2).and_then(OnOff::from_byte);
        out.exp_on_pitch_bend_modeling =
            take(&mut bytes, ADDR_EXP_ON_PITCH_BEND_MODELING).and_then(OnOff::from_byte);
        out.exp_on_mod_min = take(&mut bytes, ADDR_EXP_ON_MOD_MIN);
        out.exp_on_mod_max = take(&mut bytes, ADDR_EXP_ON_MOD_MAX);
        out.exp_on_mod_pcm_1 = take(&mut bytes, ADDR_EXP_ON_MOD_PCM_1).and_then(OnOff::from_byte);
        out.exp_on_mod_pcm_2 = take(&mut bytes, ADDR_EXP_ON_MOD_PCM_2).and_then(OnOff::from_byte);
        out.exp_on_cross_fader_a = take(&mut bytes, ADDR_EXP_ON_CROSS_FADER_A);
        out.exp_on_cross_fader_b = take(&mut bytes, ADDR_EXP_ON_CROSS_FADER_B);
        out.exp_on_cross_fader_c = take(&mut bytes, ADDR_EXP_ON_CROSS_FADER_C);
        out.exp_on_cross_fader_d = take(&mut bytes, ADDR_EXP_ON_CROSS_FADER_D);
        out.exp_on_delay_min = take(&mut bytes, ADDR_EXP_ON_DELAY_MIN);
        out.exp_on_delay_max = take(&mut bytes, ADDR_EXP_ON_DELAY_MAX);
        out.exp_on_reverb_min = take(&mut bytes, ADDR_EXP_ON_REVERB_MIN);
        out.exp_on_reverb_max = take(&mut bytes, ADDR_EXP_ON_REVERB_MAX);
        out.exp_on_chorus_min = take(&mut bytes, ADDR_EXP_ON_CHORUS_MIN);
        out.exp_on_chorus_max = take(&mut bytes, ADDR_EXP_ON_CHORUS_MAX);
        out.exp_on_mod_control_a = take(&mut bytes, ADDR_EXP_ON_MOD_CONTROL_A);
        out.exp_on_mod_control_b = take(&mut bytes, ADDR_EXP_ON_MOD_CONTROL_B);

        out.exp_sw_tone_sw_off_pcm_1 =
            take(&mut bytes, ADDR_EXP_SW_TONE_SW_OFF_PCM_1).and_then(OnOff::from_byte);
        out.exp_sw_tone_sw_off_pcm_2 =
            take(&mut bytes, ADDR_EXP_SW_TONE_SW_OFF_PCM_2).and_then(OnOff::from_byte);
        out.exp_sw_tone_sw_off_modeling =
            take(&mut bytes, ADDR_EXP_SW_TONE_SW_OFF_MODELING).and_then(OnOff::from_byte);
        out.exp_sw_tone_sw_off_normal_pu =
            take(&mut bytes, ADDR_EXP_SW_TONE_SW_OFF_NORMAL_PU).and_then(OnOff::from_byte);
        out.exp_sw_tone_sw_on_pcm_1 =
            take(&mut bytes, ADDR_EXP_SW_TONE_SW_ON_PCM_1).and_then(OnOff::from_byte);
        out.exp_sw_tone_sw_on_pcm_2 =
            take(&mut bytes, ADDR_EXP_SW_TONE_SW_ON_PCM_2).and_then(OnOff::from_byte);
        out.exp_sw_tone_sw_on_modeling =
            take(&mut bytes, ADDR_EXP_SW_TONE_SW_ON_MODELING).and_then(OnOff::from_byte);
        out.exp_sw_tone_sw_on_normal_pu =
            take(&mut bytes, ADDR_EXP_SW_TONE_SW_ON_NORMAL_PU).and_then(OnOff::from_byte);
        out.gk_s1_function =
            take(&mut bytes, ADDR_GK_S1_FUNCTION).and_then(ExpPedalSwitchFunction::from_byte);
        out.gk_s1_tone_sw_off_pcm_1 =
            take(&mut bytes, ADDR_GK_S1_TONE_SW_OFF_PCM_1).and_then(OnOff::from_byte);
        out.gk_s1_tone_sw_off_pcm_2 =
            take(&mut bytes, ADDR_GK_S1_TONE_SW_OFF_PCM_2).and_then(OnOff::from_byte);
        out.gk_s1_tone_sw_off_modeling =
            take(&mut bytes, ADDR_GK_S1_TONE_SW_OFF_MODELING).and_then(OnOff::from_byte);
        out.gk_s1_tone_sw_off_normal_pu =
            take(&mut bytes, ADDR_GK_S1_TONE_SW_OFF_NORMAL_PU).and_then(OnOff::from_byte);
        out.gk_s1_tone_sw_on_pcm_1 =
            take(&mut bytes, ADDR_GK_S1_TONE_SW_ON_PCM_1).and_then(OnOff::from_byte);
        out.gk_s1_tone_sw_on_pcm_2 =
            take(&mut bytes, ADDR_GK_S1_TONE_SW_ON_PCM_2).and_then(OnOff::from_byte);
        out.gk_s1_tone_sw_on_modeling =
            take(&mut bytes, ADDR_GK_S1_TONE_SW_ON_MODELING).and_then(OnOff::from_byte);
        out.gk_s1_tone_sw_on_normal_pu =
            take(&mut bytes, ADDR_GK_S1_TONE_SW_ON_NORMAL_PU).and_then(OnOff::from_byte);
        out.gk_s2_function =
            take(&mut bytes, ADDR_GK_S2_FUNCTION).and_then(ExpPedalSwitchFunction::from_byte);
        out.gk_s2_tone_sw_off_pcm_1 =
            take(&mut bytes, ADDR_GK_S2_TONE_SW_OFF_PCM_1).and_then(OnOff::from_byte);
        out.gk_s2_tone_sw_off_pcm_2 =
            take(&mut bytes, ADDR_GK_S2_TONE_SW_OFF_PCM_2).and_then(OnOff::from_byte);
        out.gk_s2_tone_sw_off_modeling =
            take(&mut bytes, ADDR_GK_S2_TONE_SW_OFF_MODELING).and_then(OnOff::from_byte);
        out.gk_s2_tone_sw_off_normal_pu =
            take(&mut bytes, ADDR_GK_S2_TONE_SW_OFF_NORMAL_PU).and_then(OnOff::from_byte);
        out.gk_s2_tone_sw_on_pcm_1 =
            take(&mut bytes, ADDR_GK_S2_TONE_SW_ON_PCM_1).and_then(OnOff::from_byte);
        out.gk_s2_tone_sw_on_pcm_2 =
            take(&mut bytes, ADDR_GK_S2_TONE_SW_ON_PCM_2).and_then(OnOff::from_byte);
        out.gk_s2_tone_sw_on_modeling =
            take(&mut bytes, ADDR_GK_S2_TONE_SW_ON_MODELING).and_then(OnOff::from_byte);
        out.gk_s2_tone_sw_on_normal_pu =
            take(&mut bytes, ADDR_GK_S2_TONE_SW_ON_NORMAL_PU).and_then(OnOff::from_byte);
        out.vol_mod_control_a = take(&mut bytes, ADDR_VOL_MOD_CONTROL_A);
        out.vol_mod_control_b = take(&mut bytes, ADDR_VOL_MOD_CONTROL_B);

        out.gk_set_select = take(&mut bytes, ADDR_GK_SET_SELECT);
        out.guitar_out = take(&mut bytes, ADDR_GUITAR_OUT);
        out.v_link_palette = take(&mut bytes, ADDR_VLINK_PALETTE);
        out.v_link_clip = take(&mut bytes, ADDR_VLINK_CLIP);
        out.v_link_note_clip_change = take(&mut bytes, ADDR_VLINK_NOTE_CLIP_CHANGE);
        out.v_link_exp = take(&mut bytes, ADDR_VLINK_EXP);
        out.v_link_exp_on = take(&mut bytes, ADDR_VLINK_EXP_ON);
        out.v_link_gk_vol = take(&mut bytes, ADDR_VLINK_GK_VOL);
        out.alt_tuning_sw = take(&mut bytes, ADDR_ALT_TUNING_SW).and_then(OnOff::from_byte);
        out.alt_tuning_type = take(&mut bytes, ADDR_ALT_TUNING_TYPE);
        for (i, addr) in ADDR_USER_TUNING_SHIFT_STRINGS.iter().enumerate() {
            out.user_tuning_shift_strings[i] = take(&mut bytes, *addr);
        }

        // Group GK setup bytes by their sub-LSB before they hit unknown_bytes.
        for (i, sub_lsb) in GK_SETUP_SUB_LSBS.iter().enumerate() {
            let mut setup = GkSetup::default();
            let setup_addrs: Vec<[u8; 4]> = bytes
                .keys()
                .filter(|a| a[0] == 0x02 && a[1] == *sub_lsb && a[2] == 0x00)
                .copied()
                .collect();
            if setup_addrs.is_empty() {
                continue;
            }
            for addr in setup_addrs {
                if let Some(b) = bytes.remove(&addr) {
                    let offset = addr[3];
                    match offset {
                        0x00..=0x07 => setup.name.0[offset as usize] = b,
                        0x08 => setup.hex_pu_type = HexPuType::from_byte(b),
                        0x09 => setup.scale = Some(b),
                        0x0A => setup.lsb = Some(b),
                        0x0B => setup.pu_phase = PuPhase::from_byte(b),
                        0x0C => setup.pu_direction = PuDirection::from_byte(b),
                        0x0D => setup.s1_s2_position = S1S2Position::from_byte(b),
                        0x0E => setup.normal_pu_gain = NormalPuGain::from_byte(b),
                        0x0F => setup.piezo_low = PiezoGain::from_byte(b),
                        0x10 => setup.piezo_high = PiezoGain::from_byte(b),
                        0x11..=0x16 => {
                            setup.string_distance_bridge[(offset - 0x11) as usize] = Some(b);
                        }
                        0x17..=0x1C => {
                            setup.string_sensitivity[(offset - 0x17) as usize] = Some(b);
                        }
                        0x1D => setup.play_feel = PlayFeel::from_byte(b),
                        0x1E => setup.low_velocity_cut = Rating0To10::from_byte(b),
                        0x1F => setup.velocity_dynamics = Rating1To10::from_byte(b),
                        0x20 => setup.nuance_dynamics = Rating0To10::from_byte(b),
                        0x21 => setup.nuance_trim = Rating0To10::from_byte(b),
                        0x22 => setup.down_shift = DownShift::from_byte(b),
                        0x23..=0x2A => setup.bass_name.0[(offset - 0x23) as usize] = b,
                        0x2B => setup.bass_hex_pu_type = BassHexPuType::from_byte(b),
                        0x2C => setup.bass_scale = Some(b),
                        0x2D => setup.bass_lsb = Some(b),
                        0x2E => setup.bass_gk_pu_position = GkPuPosition::from_byte(b),
                        0x2F => setup.bass_pu_phase = PuPhase::from_byte(b),
                        0x30 => setup.bass_pu_direction = PuDirection::from_byte(b),
                        0x31 => setup.bass_s1_s2_position = S1S2Position::from_byte(b),
                        0x32 => setup.bass_normal_pu_gain = NormalPuGain::from_byte(b),
                        0x33 => setup.bass_piezo_low = PiezoGain::from_byte(b),
                        0x34 => setup.bass_piezo_high = PiezoGain::from_byte(b),
                        0x35..=0x3A => {
                            setup.bass_string_distance_bridge[(offset - 0x35) as usize] = Some(b);
                        }
                        0x3B..=0x40 => {
                            setup.bass_string_sensitivity[(offset - 0x3B) as usize] = Some(b);
                        }
                        0x41 => setup.bass_play_feel = PlayFeel::from_byte(b),
                        0x42 => setup.bass_low_velocity_cut = Rating0To10::from_byte(b),
                        0x43 => setup.bass_velocity_dynamics = Rating1To10::from_byte(b),
                        0x44 => setup.bass_nuance_dynamics = Rating0To10::from_byte(b),
                        0x45 => setup.bass_nuance_trim = Rating0To10::from_byte(b),
                        0x46 => setup.bass_down_shift = DownShift::from_byte(b),
                        0x51 if b <= 100 => setup.develop_low_cut_guitar = Some(b),
                        0x52 if b <= 100 => setup.develop_low_cut_bass = Some(b),
                        _ => {
                            setup.raw_bytes.insert(offset, b);
                        }
                    }
                }
            }
            out.gk_setups[i] = Some(setup);
        }

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
        if let Some(slot) = self.current_patch {
            let idx = slot.linear_index();
            bytes.insert(ADDR_CURRENT_PATCH_HI, (idx / 128) as u8);
            bytes.insert(ADDR_CURRENT_PATCH_LO, (idx % 128) as u8);
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
        if let Some(v) = self.exp_pedal_bend {
            bytes.insert(ADDR_EXP_PEDAL_BEND, v.to_byte());
        }
        for (addr, value) in [
            (ADDR_GK_VOL_CC, self.gk_vol_cc),
            (ADDR_GK_S1_CC, self.gk_s1_cc),
            (ADDR_GK_S2_CC, self.gk_s2_cc),
        ] {
            if let Some(v) = value {
                let raw = match v {
                    PedalCc::Cc(n) => n,
                    PedalCc::Off => 0,
                };
                let byte = v.to_byte().ok_or(CodecError::PedalCcOutOfRange(raw))?;
                bytes.insert(addr, byte);
            }
        }
        if let Some(v) = self.midi_map {
            bytes.insert(ADDR_MIDI_MAP, v.to_byte());
        }
        if let Some(v) = self.monitor_direct {
            bytes.insert(ADDR_MONITOR_DIRECT, v.to_byte());
        }
        if let Some(v) = self.guitar_out_source {
            bytes.insert(ADDR_GUITAR_OUT_SOURCE, v.to_byte());
        }
        if let Some(v) = self.master_tune {
            bytes.insert(ADDR_MASTER_TUNE, v.to_byte());
        }
        if let Some(v) = self.tuner_mute {
            bytes.insert(ADDR_TUNER_MUTE, v.to_byte());
        }
        if let Some(v) = self.startup_mode {
            bytes.insert(ADDR_STARTUP_MODE, v.to_byte());
        }
        for (level, hi_addr, lo_addr) in [
            (
                self.player_level,
                ADDR_PLAYER_LEVEL_HI,
                ADDR_PLAYER_LEVEL_LO,
            ),
            (
                self.usb_audio_in_level,
                ADDR_USB_AUDIO_IN_HI,
                ADDR_USB_AUDIO_IN_LO,
            ),
            (
                self.usb_audio_out_level,
                ADDR_USB_AUDIO_OUT_HI,
                ADDR_USB_AUDIO_OUT_LO,
            ),
        ] {
            if let Some(v) = level {
                let [hi, lo] = v.to_two_bytes();
                bytes.insert(hi_addr, hi);
                bytes.insert(lo_addr, lo);
            }
        }
        if let Some(v) = self.patch_level {
            let [hi, lo] = v.to_two_bytes();
            bytes.insert(ADDR_PATCH_LEVEL_HI, hi);
            bytes.insert(ADDR_PATCH_LEVEL_LO, lo);
        }
        if let Some(v) = self.master_bpm {
            let [hi, lo] = v.to_two_bytes();
            bytes.insert(ADDR_MASTER_BPM_HI, hi);
            bytes.insert(ADDR_MASTER_BPM_LO, lo);
        }
        if let Some(v) = self.ctl_pedal_function {
            bytes.insert(ADDR_CTL_PEDAL_FUNCTION, v.to_byte());
        }
        if let Some(v) = self.exp_pedal_off_function {
            bytes.insert(ADDR_EXP_PEDAL_OFF_FUNCTION, v.to_byte());
        }
        if let Some(v) = self.exp_pedal_on_function {
            bytes.insert(ADDR_EXP_PEDAL_ON_FUNCTION, v.to_byte());
        }
        if let Some(v) = self.exp_pedal_switch_function {
            bytes.insert(ADDR_EXP_PEDAL_SWITCH_FUNCTION, v.to_byte());
        }
        if let Some(v) = self.ctl_hold_type {
            bytes.insert(ADDR_CTL_HOLD_TYPE, v.to_byte());
        }
        if let Some(v) = self.ctl_switch_mode {
            bytes.insert(ADDR_CTL_SWITCH_MODE, v.to_byte());
        }
        for (addr, value) in [
            (ADDR_CTL_HOLD_PCM_1, self.ctl_hold_pcm_1),
            (ADDR_CTL_HOLD_PCM_2, self.ctl_hold_pcm_2),
            (ADDR_CTL_TONE_SW_OFF_PCM_1, self.ctl_tone_sw_off_pcm_1),
            (ADDR_CTL_TONE_SW_OFF_PCM_2, self.ctl_tone_sw_off_pcm_2),
            (ADDR_CTL_TONE_SW_OFF_MODELING, self.ctl_tone_sw_off_modeling),
            (
                ADDR_CTL_TONE_SW_OFF_NORMAL_PU,
                self.ctl_tone_sw_off_normal_pu,
            ),
            (ADDR_CTL_TONE_SW_ON_PCM_1, self.ctl_tone_sw_on_pcm_1),
            (ADDR_CTL_TONE_SW_ON_PCM_2, self.ctl_tone_sw_on_pcm_2),
            (ADDR_CTL_TONE_SW_ON_MODELING, self.ctl_tone_sw_on_modeling),
            (ADDR_CTL_TONE_SW_ON_NORMAL_PU, self.ctl_tone_sw_on_normal_pu),
            (ADDR_EXP_OFF_TONE_VOL_PCM_1, self.exp_off_tone_vol_pcm_1),
            (ADDR_EXP_OFF_TONE_VOL_PCM_2, self.exp_off_tone_vol_pcm_2),
            (
                ADDR_EXP_OFF_TONE_VOL_MODELING,
                self.exp_off_tone_vol_modeling,
            ),
            (
                ADDR_EXP_OFF_TONE_VOL_NORMAL_PU,
                self.exp_off_tone_vol_normal_pu,
            ),
            (ADDR_EXP_OFF_PITCH_BEND_PCM_1, self.exp_off_pitch_bend_pcm_1),
            (ADDR_EXP_OFF_PITCH_BEND_PCM_2, self.exp_off_pitch_bend_pcm_2),
            (
                ADDR_EXP_OFF_PITCH_BEND_MODELING,
                self.exp_off_pitch_bend_modeling,
            ),
            (ADDR_EXP_OFF_MOD_PCM_1, self.exp_off_mod_pcm_1),
            (ADDR_EXP_OFF_MOD_PCM_2, self.exp_off_mod_pcm_2),
            (ADDR_EXP_ON_TONE_VOL_PCM_1, self.exp_on_tone_vol_pcm_1),
            (ADDR_EXP_ON_TONE_VOL_PCM_2, self.exp_on_tone_vol_pcm_2),
            (ADDR_EXP_ON_TONE_VOL_MODELING, self.exp_on_tone_vol_modeling),
            (
                ADDR_EXP_ON_TONE_VOL_NORMAL_PU,
                self.exp_on_tone_vol_normal_pu,
            ),
            (ADDR_EXP_ON_PITCH_BEND_PCM_1, self.exp_on_pitch_bend_pcm_1),
            (ADDR_EXP_ON_PITCH_BEND_PCM_2, self.exp_on_pitch_bend_pcm_2),
            (
                ADDR_EXP_ON_PITCH_BEND_MODELING,
                self.exp_on_pitch_bend_modeling,
            ),
            (ADDR_EXP_ON_MOD_PCM_1, self.exp_on_mod_pcm_1),
            (ADDR_EXP_ON_MOD_PCM_2, self.exp_on_mod_pcm_2),
            (ADDR_EXP_SW_TONE_SW_OFF_PCM_1, self.exp_sw_tone_sw_off_pcm_1),
            (ADDR_EXP_SW_TONE_SW_OFF_PCM_2, self.exp_sw_tone_sw_off_pcm_2),
            (
                ADDR_EXP_SW_TONE_SW_OFF_MODELING,
                self.exp_sw_tone_sw_off_modeling,
            ),
            (
                ADDR_EXP_SW_TONE_SW_OFF_NORMAL_PU,
                self.exp_sw_tone_sw_off_normal_pu,
            ),
            (ADDR_EXP_SW_TONE_SW_ON_PCM_1, self.exp_sw_tone_sw_on_pcm_1),
            (ADDR_EXP_SW_TONE_SW_ON_PCM_2, self.exp_sw_tone_sw_on_pcm_2),
            (
                ADDR_EXP_SW_TONE_SW_ON_MODELING,
                self.exp_sw_tone_sw_on_modeling,
            ),
            (
                ADDR_EXP_SW_TONE_SW_ON_NORMAL_PU,
                self.exp_sw_tone_sw_on_normal_pu,
            ),
            (ADDR_GK_S1_TONE_SW_OFF_PCM_1, self.gk_s1_tone_sw_off_pcm_1),
            (ADDR_GK_S1_TONE_SW_OFF_PCM_2, self.gk_s1_tone_sw_off_pcm_2),
            (
                ADDR_GK_S1_TONE_SW_OFF_MODELING,
                self.gk_s1_tone_sw_off_modeling,
            ),
            (
                ADDR_GK_S1_TONE_SW_OFF_NORMAL_PU,
                self.gk_s1_tone_sw_off_normal_pu,
            ),
            (ADDR_GK_S1_TONE_SW_ON_PCM_1, self.gk_s1_tone_sw_on_pcm_1),
            (ADDR_GK_S1_TONE_SW_ON_PCM_2, self.gk_s1_tone_sw_on_pcm_2),
            (
                ADDR_GK_S1_TONE_SW_ON_MODELING,
                self.gk_s1_tone_sw_on_modeling,
            ),
            (
                ADDR_GK_S1_TONE_SW_ON_NORMAL_PU,
                self.gk_s1_tone_sw_on_normal_pu,
            ),
            (ADDR_GK_S2_TONE_SW_OFF_PCM_1, self.gk_s2_tone_sw_off_pcm_1),
            (ADDR_GK_S2_TONE_SW_OFF_PCM_2, self.gk_s2_tone_sw_off_pcm_2),
            (
                ADDR_GK_S2_TONE_SW_OFF_MODELING,
                self.gk_s2_tone_sw_off_modeling,
            ),
            (
                ADDR_GK_S2_TONE_SW_OFF_NORMAL_PU,
                self.gk_s2_tone_sw_off_normal_pu,
            ),
            (ADDR_GK_S2_TONE_SW_ON_PCM_1, self.gk_s2_tone_sw_on_pcm_1),
            (ADDR_GK_S2_TONE_SW_ON_PCM_2, self.gk_s2_tone_sw_on_pcm_2),
            (
                ADDR_GK_S2_TONE_SW_ON_MODELING,
                self.gk_s2_tone_sw_on_modeling,
            ),
            (
                ADDR_GK_S2_TONE_SW_ON_NORMAL_PU,
                self.gk_s2_tone_sw_on_normal_pu,
            ),
        ] {
            if let Some(v) = value {
                bytes.insert(addr, v.to_byte());
            }
        }
        if let Some(v) = self.exp_off_pitch_bend_depth {
            bytes.insert(ADDR_EXP_OFF_PITCH_BEND_DEPTH, v.to_byte());
        }
        if let Some(v) = self.exp_on_pitch_bend_depth {
            bytes.insert(ADDR_EXP_ON_PITCH_BEND_DEPTH, v.to_byte());
        }
        if let Some(v) = self.gk_s1_function {
            bytes.insert(ADDR_GK_S1_FUNCTION, v.to_byte());
        }
        if let Some(v) = self.gk_s2_function {
            bytes.insert(ADDR_GK_S2_FUNCTION, v.to_byte());
        }
        for (addr, value) in [
            (ADDR_EXP_OFF_MOD_MIN, self.exp_off_mod_min),
            (ADDR_EXP_OFF_MOD_MAX, self.exp_off_mod_max),
            (ADDR_EXP_OFF_CROSS_FADER_A, self.exp_off_cross_fader_a),
            (ADDR_EXP_OFF_CROSS_FADER_B, self.exp_off_cross_fader_b),
            (ADDR_EXP_OFF_CROSS_FADER_C, self.exp_off_cross_fader_c),
            (ADDR_EXP_OFF_CROSS_FADER_D, self.exp_off_cross_fader_d),
            (ADDR_EXP_OFF_DELAY_MIN, self.exp_off_delay_min),
            (ADDR_EXP_OFF_DELAY_MAX, self.exp_off_delay_max),
            (ADDR_EXP_OFF_REVERB_MIN, self.exp_off_reverb_min),
            (ADDR_EXP_OFF_REVERB_MAX, self.exp_off_reverb_max),
            (ADDR_EXP_OFF_CHORUS_MIN, self.exp_off_chorus_min),
            (ADDR_EXP_OFF_CHORUS_MAX, self.exp_off_chorus_max),
            (ADDR_EXP_OFF_MOD_CONTROL_A, self.exp_off_mod_control_a),
            (ADDR_EXP_OFF_MOD_CONTROL_B, self.exp_off_mod_control_b),
            (ADDR_EXP_ON_MOD_MIN, self.exp_on_mod_min),
            (ADDR_EXP_ON_MOD_MAX, self.exp_on_mod_max),
            (ADDR_EXP_ON_CROSS_FADER_A, self.exp_on_cross_fader_a),
            (ADDR_EXP_ON_CROSS_FADER_B, self.exp_on_cross_fader_b),
            (ADDR_EXP_ON_CROSS_FADER_C, self.exp_on_cross_fader_c),
            (ADDR_EXP_ON_CROSS_FADER_D, self.exp_on_cross_fader_d),
            (ADDR_EXP_ON_DELAY_MIN, self.exp_on_delay_min),
            (ADDR_EXP_ON_DELAY_MAX, self.exp_on_delay_max),
            (ADDR_EXP_ON_REVERB_MIN, self.exp_on_reverb_min),
            (ADDR_EXP_ON_REVERB_MAX, self.exp_on_reverb_max),
            (ADDR_EXP_ON_CHORUS_MIN, self.exp_on_chorus_min),
            (ADDR_EXP_ON_CHORUS_MAX, self.exp_on_chorus_max),
            (ADDR_EXP_ON_MOD_CONTROL_A, self.exp_on_mod_control_a),
            (ADDR_EXP_ON_MOD_CONTROL_B, self.exp_on_mod_control_b),
            (ADDR_VOL_MOD_CONTROL_A, self.vol_mod_control_a),
            (ADDR_VOL_MOD_CONTROL_B, self.vol_mod_control_b),
        ] {
            if let Some(v) = value {
                bytes.insert(addr, v);
            }
        }
        if let Some(v) = self.gk_set_select {
            bytes.insert(ADDR_GK_SET_SELECT, v);
        }
        if let Some(v) = self.guitar_out {
            bytes.insert(ADDR_GUITAR_OUT, v);
        }
        for (addr, value) in [
            (ADDR_VLINK_PALETTE, self.v_link_palette),
            (ADDR_VLINK_CLIP, self.v_link_clip),
            (ADDR_VLINK_NOTE_CLIP_CHANGE, self.v_link_note_clip_change),
            (ADDR_VLINK_EXP, self.v_link_exp),
            (ADDR_VLINK_EXP_ON, self.v_link_exp_on),
            (ADDR_VLINK_GK_VOL, self.v_link_gk_vol),
        ] {
            if let Some(v) = value {
                bytes.insert(addr, v);
            }
        }
        if let Some(v) = self.alt_tuning_sw {
            bytes.insert(ADDR_ALT_TUNING_SW, v.to_byte());
        }
        if let Some(v) = self.alt_tuning_type {
            bytes.insert(ADDR_ALT_TUNING_TYPE, v);
        }
        for (i, addr) in ADDR_USER_TUNING_SHIFT_STRINGS.iter().enumerate() {
            if let Some(v) = self.user_tuning_shift_strings[i] {
                bytes.insert(*addr, v);
            }
        }
        for (i, slot) in self.gk_setups.iter().enumerate() {
            let Some(setup) = slot else {
                continue;
            };
            let sub_lsb = GK_SETUP_SUB_LSBS[i];
            // Offsets land in `address[3]` to match the multi-byte frame
            // convention (`address[3]` is incremented by from_frames).
            if !setup.name.is_empty() {
                for (offset, b) in setup.name.0.iter().enumerate() {
                    bytes.insert([0x02, sub_lsb, 0x00, offset as u8], *b);
                }
            }
            if let Some(v) = setup.hex_pu_type {
                bytes.insert([0x02, sub_lsb, 0x00, 0x08], v.to_byte());
            }
            if let Some(v) = setup.scale {
                bytes.insert([0x02, sub_lsb, 0x00, 0x09], v);
            }
            if let Some(v) = setup.lsb {
                bytes.insert([0x02, sub_lsb, 0x00, 0x0A], v);
            }
            if let Some(v) = setup.pu_phase {
                bytes.insert([0x02, sub_lsb, 0x00, 0x0B], v.to_byte());
            }
            if let Some(v) = setup.pu_direction {
                bytes.insert([0x02, sub_lsb, 0x00, 0x0C], v.to_byte());
            }
            if let Some(v) = setup.s1_s2_position {
                bytes.insert([0x02, sub_lsb, 0x00, 0x0D], v.to_byte());
            }
            if let Some(v) = setup.normal_pu_gain {
                bytes.insert([0x02, sub_lsb, 0x00, 0x0E], v.to_byte());
            }
            if let Some(v) = setup.piezo_low {
                bytes.insert([0x02, sub_lsb, 0x00, 0x0F], v.to_byte());
            }
            if let Some(v) = setup.piezo_high {
                bytes.insert([0x02, sub_lsb, 0x00, 0x10], v.to_byte());
            }
            for (i, v) in setup.string_distance_bridge.iter().enumerate() {
                if let Some(b) = v {
                    bytes.insert([0x02, sub_lsb, 0x00, 0x11 + i as u8], *b);
                }
            }
            for (i, v) in setup.string_sensitivity.iter().enumerate() {
                if let Some(b) = v {
                    bytes.insert([0x02, sub_lsb, 0x00, 0x17 + i as u8], *b);
                }
            }
            if let Some(v) = setup.play_feel {
                bytes.insert([0x02, sub_lsb, 0x00, 0x1D], v.to_byte());
            }
            if let Some(v) = setup.low_velocity_cut {
                bytes.insert([0x02, sub_lsb, 0x00, 0x1E], v.to_byte());
            }
            if let Some(v) = setup.velocity_dynamics {
                bytes.insert([0x02, sub_lsb, 0x00, 0x1F], v.to_byte());
            }
            if let Some(v) = setup.nuance_dynamics {
                bytes.insert([0x02, sub_lsb, 0x00, 0x20], v.to_byte());
            }
            if let Some(v) = setup.nuance_trim {
                bytes.insert([0x02, sub_lsb, 0x00, 0x21], v.to_byte());
            }
            if let Some(v) = setup.down_shift {
                bytes.insert([0x02, sub_lsb, 0x00, 0x22], v.to_byte());
            }
            // ---- Bass Mode parallel block (0x23..=0x40) ----
            for (i, b) in setup.bass_name.0.iter().enumerate() {
                if *b != 0x20 {
                    bytes.insert([0x02, sub_lsb, 0x00, 0x23 + i as u8], *b);
                }
            }
            if let Some(v) = setup.bass_hex_pu_type {
                bytes.insert([0x02, sub_lsb, 0x00, 0x2B], v.to_byte());
            }
            if let Some(v) = setup.bass_scale {
                bytes.insert([0x02, sub_lsb, 0x00, 0x2C], v);
            }
            if let Some(v) = setup.bass_lsb {
                bytes.insert([0x02, sub_lsb, 0x00, 0x2D], v);
            }
            if let Some(v) = setup.bass_gk_pu_position {
                bytes.insert([0x02, sub_lsb, 0x00, 0x2E], v.to_byte());
            }
            if let Some(v) = setup.bass_pu_phase {
                bytes.insert([0x02, sub_lsb, 0x00, 0x2F], v.to_byte());
            }
            if let Some(v) = setup.bass_pu_direction {
                bytes.insert([0x02, sub_lsb, 0x00, 0x30], v.to_byte());
            }
            if let Some(v) = setup.bass_s1_s2_position {
                bytes.insert([0x02, sub_lsb, 0x00, 0x31], v.to_byte());
            }
            if let Some(v) = setup.bass_normal_pu_gain {
                bytes.insert([0x02, sub_lsb, 0x00, 0x32], v.to_byte());
            }
            if let Some(v) = setup.bass_piezo_low {
                bytes.insert([0x02, sub_lsb, 0x00, 0x33], v.to_byte());
            }
            if let Some(v) = setup.bass_piezo_high {
                bytes.insert([0x02, sub_lsb, 0x00, 0x34], v.to_byte());
            }
            for (i, v) in setup.bass_string_distance_bridge.iter().enumerate() {
                if let Some(b) = v {
                    bytes.insert([0x02, sub_lsb, 0x00, 0x35 + i as u8], *b);
                }
            }
            for (i, v) in setup.bass_string_sensitivity.iter().enumerate() {
                if let Some(b) = v {
                    bytes.insert([0x02, sub_lsb, 0x00, 0x3B + i as u8], *b);
                }
            }
            if let Some(v) = setup.bass_play_feel {
                bytes.insert([0x02, sub_lsb, 0x00, 0x41], v.to_byte());
            }
            if let Some(v) = setup.bass_low_velocity_cut {
                bytes.insert([0x02, sub_lsb, 0x00, 0x42], v.to_byte());
            }
            if let Some(v) = setup.bass_velocity_dynamics {
                bytes.insert([0x02, sub_lsb, 0x00, 0x43], v.to_byte());
            }
            if let Some(v) = setup.bass_nuance_dynamics {
                bytes.insert([0x02, sub_lsb, 0x00, 0x44], v.to_byte());
            }
            if let Some(v) = setup.bass_nuance_trim {
                bytes.insert([0x02, sub_lsb, 0x00, 0x45], v.to_byte());
            }
            if let Some(v) = setup.bass_down_shift {
                bytes.insert([0x02, sub_lsb, 0x00, 0x46], v.to_byte());
            }
            if let Some(v) = setup.develop_low_cut_guitar {
                bytes.insert([0x02, sub_lsb, 0x00, 0x51], v);
            }
            if let Some(v) = setup.develop_low_cut_bass {
                bytes.insert([0x02, sub_lsb, 0x00, 0x52], v);
            }
            for (offset, b) in &setup.raw_bytes {
                bytes.insert([0x02, sub_lsb, 0x00, *offset], *b);
            }
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

/// Pull both bytes of a 14-bit audio level out of the byte map iff both are
/// present and the resulting value is in range; otherwise leave them in place
/// so they fall through to `unknown_bytes` for lossless round-trip.
fn consume_audio_level(
    bytes: &mut BTreeMap<[u8; 4], u8>,
    hi: [u8; 4],
    lo: [u8; 4],
) -> Option<AudioLevel> {
    let hi_byte = *bytes.get(&hi)?;
    let lo_byte = *bytes.get(&lo)?;
    let level = AudioLevel::from_two_bytes(hi_byte, lo_byte)?;
    bytes.remove(&hi);
    bytes.remove(&lo);
    Some(level)
}

fn user_tuning_shift_all_none(arr: &[Option<u8>; 6]) -> bool {
    arr.iter().all(Option::is_none)
}

fn all_gk_setups_none(arr: &[Option<GkSetup>; 10]) -> bool {
    arr.iter().all(Option::is_none)
}

fn string_distance_all_none(arr: &[Option<u8>; 6]) -> bool {
    arr.iter().all(Option::is_none)
}

/// Generic two-byte consumer: only takes both bytes if both are present
/// and the supplied decoder accepts them.
fn consume_two_byte<T>(
    bytes: &mut BTreeMap<[u8; 4], u8>,
    hi: [u8; 4],
    lo: [u8; 4],
    decoder: impl Fn(u8, u8) -> Option<T>,
) -> Option<T> {
    let hi_byte = *bytes.get(&hi)?;
    let lo_byte = *bytes.get(&lo)?;
    let value = decoder(hi_byte, lo_byte)?;
    bytes.remove(&hi);
    bytes.remove(&lo);
    Some(value)
}

/// Pull both bytes of the Current Patch 14-bit selector out of the byte map
/// iff both are present and the resulting linear index maps to a real patch
/// slot (skipping the USER↔PRESET reserved gap).
fn consume_current_patch(
    bytes: &mut BTreeMap<[u8; 4], u8>,
    hi: [u8; 4],
    lo: [u8; 4],
) -> Option<PatchSlot> {
    let hi_byte = *bytes.get(&hi)?;
    let lo_byte = *bytes.get(&lo)?;
    if hi_byte > 0x7F || lo_byte > 0x7F {
        return None;
    }
    let idx = u32::from(hi_byte) * 128 + u32::from(lo_byte);
    let slot = PatchSlot::from_linear_index(idx)?;
    bytes.remove(&hi);
    bytes.remove(&lo);
    Some(slot)
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
            current_patch: Some(PatchSlot::user(20, 1).unwrap()),
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
            exp_pedal_bend: Some(ExpPedalBend::new(-12).unwrap()),
            gk_vol_cc: Some(PedalCc::Cc(11)),
            gk_s1_cc: Some(PedalCc::Off),
            gk_s2_cc: Some(PedalCc::Cc(80)),
            midi_map: Some(MidiMap::Programmable),
            monitor_direct: Some(OnOff::On),
            guitar_out_source: Some(GuitarOutSource::Modeling),
            master_tune: Some(MasterTuneHz::new(442).unwrap()),
            tuner_mute: Some(OnOff::Off),
            startup_mode: Some(Mode::Bass),
            player_level: Some(AudioLevel::new(100).unwrap()),
            usb_audio_in_level: Some(AudioLevel::new(128).unwrap()),
            usb_audio_out_level: Some(AudioLevel::new(200).unwrap()),
            patch_level: Some(PatchLevel::new(75).unwrap()),
            master_bpm: Some(MasterBpm::new(120)),
            ctl_pedal_function: Some(CtlPedalFunction::TapTempo),
            exp_pedal_off_function: Some(ExpPedalFunction::PatchVolume),
            exp_pedal_on_function: Some(ExpPedalFunction::Modulation),
            exp_pedal_switch_function: Some(ExpPedalSwitchFunction::VLinkSw),
            gk_set_select: Some(3),
            guitar_out: Some(2),
            v_link_palette: Some(5),
            v_link_clip: Some(6),
            v_link_note_clip_change: Some(7),
            v_link_exp: Some(8),
            v_link_exp_on: Some(9),
            v_link_gk_vol: Some(10),
            alt_tuning_sw: Some(OnOff::On),
            alt_tuning_type: Some(4),
            user_tuning_shift_strings: [Some(64), Some(65), Some(66), Some(67), Some(68), Some(69)],
            ctl_hold_type: Some(HoldType::Type2),
            ctl_switch_mode: Some(SwitchMode::Moment),
            ctl_hold_pcm_1: Some(OnOff::On),
            ctl_hold_pcm_2: Some(OnOff::Off),
            ctl_tone_sw_off_pcm_1: Some(OnOff::On),
            ctl_tone_sw_off_pcm_2: Some(OnOff::Off),
            ctl_tone_sw_off_modeling: Some(OnOff::On),
            ctl_tone_sw_off_normal_pu: Some(OnOff::Off),
            ctl_tone_sw_on_pcm_1: Some(OnOff::Off),
            ctl_tone_sw_on_pcm_2: Some(OnOff::On),
            ctl_tone_sw_on_modeling: Some(OnOff::Off),
            ctl_tone_sw_on_normal_pu: Some(OnOff::On),
            exp_off_tone_vol_pcm_1: Some(OnOff::On),
            exp_off_tone_vol_pcm_2: Some(OnOff::Off),
            exp_off_tone_vol_modeling: Some(OnOff::On),
            exp_off_tone_vol_normal_pu: Some(OnOff::Off),
            exp_off_pitch_bend_depth: Some(PitchBendDepth::new(-7).unwrap()),
            exp_off_pitch_bend_pcm_1: Some(OnOff::On),
            exp_off_pitch_bend_pcm_2: Some(OnOff::Off),
            exp_off_pitch_bend_modeling: Some(OnOff::On),
            exp_off_mod_min: Some(0x10),
            exp_off_mod_max: Some(0x70),
            exp_off_mod_pcm_1: Some(OnOff::On),
            exp_off_mod_pcm_2: Some(OnOff::Off),
            exp_off_cross_fader_a: Some(0x20),
            exp_off_cross_fader_b: Some(0x30),
            exp_off_cross_fader_c: Some(0x40),
            exp_off_cross_fader_d: Some(0x50),
            exp_off_delay_min: Some(0x10),
            exp_off_delay_max: Some(0x60),
            exp_off_reverb_min: Some(0x20),
            exp_off_reverb_max: Some(0x70),
            exp_off_chorus_min: Some(0x10),
            exp_off_chorus_max: Some(0x50),
            exp_off_mod_control_a: Some(0x40),
            exp_off_mod_control_b: Some(0x60),
            exp_on_tone_vol_pcm_1: Some(OnOff::Off),
            exp_on_tone_vol_pcm_2: Some(OnOff::On),
            exp_on_tone_vol_modeling: Some(OnOff::Off),
            exp_on_tone_vol_normal_pu: Some(OnOff::On),
            exp_on_pitch_bend_depth: Some(PitchBendDepth::new(7).unwrap()),
            exp_on_pitch_bend_pcm_1: Some(OnOff::Off),
            exp_on_pitch_bend_pcm_2: Some(OnOff::On),
            exp_on_pitch_bend_modeling: Some(OnOff::Off),
            exp_on_mod_min: Some(0x05),
            exp_on_mod_max: Some(0x65),
            exp_on_mod_pcm_1: Some(OnOff::Off),
            exp_on_mod_pcm_2: Some(OnOff::On),
            exp_on_cross_fader_a: Some(0x15),
            exp_on_cross_fader_b: Some(0x25),
            exp_on_cross_fader_c: Some(0x35),
            exp_on_cross_fader_d: Some(0x45),
            exp_on_delay_min: Some(0x05),
            exp_on_delay_max: Some(0x55),
            exp_on_reverb_min: Some(0x15),
            exp_on_reverb_max: Some(0x65),
            exp_on_chorus_min: Some(0x05),
            exp_on_chorus_max: Some(0x45),
            exp_on_mod_control_a: Some(0x35),
            exp_on_mod_control_b: Some(0x55),
            exp_sw_tone_sw_off_pcm_1: Some(OnOff::On),
            exp_sw_tone_sw_off_pcm_2: Some(OnOff::Off),
            exp_sw_tone_sw_off_modeling: Some(OnOff::On),
            exp_sw_tone_sw_off_normal_pu: Some(OnOff::Off),
            exp_sw_tone_sw_on_pcm_1: Some(OnOff::Off),
            exp_sw_tone_sw_on_pcm_2: Some(OnOff::On),
            exp_sw_tone_sw_on_modeling: Some(OnOff::Off),
            exp_sw_tone_sw_on_normal_pu: Some(OnOff::On),
            gk_s1_function: Some(ExpPedalSwitchFunction::DelaySw),
            gk_s1_tone_sw_off_pcm_1: Some(OnOff::On),
            gk_s1_tone_sw_off_pcm_2: Some(OnOff::Off),
            gk_s1_tone_sw_off_modeling: Some(OnOff::On),
            gk_s1_tone_sw_off_normal_pu: Some(OnOff::Off),
            gk_s1_tone_sw_on_pcm_1: Some(OnOff::Off),
            gk_s1_tone_sw_on_pcm_2: Some(OnOff::On),
            gk_s1_tone_sw_on_modeling: Some(OnOff::Off),
            gk_s1_tone_sw_on_normal_pu: Some(OnOff::On),
            gk_s2_function: Some(ExpPedalSwitchFunction::ReverbSw),
            gk_s2_tone_sw_off_pcm_1: Some(OnOff::Off),
            gk_s2_tone_sw_off_pcm_2: Some(OnOff::On),
            gk_s2_tone_sw_off_modeling: Some(OnOff::Off),
            gk_s2_tone_sw_off_normal_pu: Some(OnOff::On),
            gk_s2_tone_sw_on_pcm_1: Some(OnOff::On),
            gk_s2_tone_sw_on_pcm_2: Some(OnOff::Off),
            gk_s2_tone_sw_on_modeling: Some(OnOff::On),
            gk_s2_tone_sw_on_normal_pu: Some(OnOff::Off),
            vol_mod_control_a: Some(0x30),
            vol_mod_control_b: Some(0x70),
            gk_setups: {
                let mut s = GkSetup::default();
                s.name.0[0] = b'M';
                s.name.0[1] = b'y';
                s.name.0[2] = b' ';
                s.name.0[3] = b'G';
                s.name.0[4] = b'K';
                s.raw_bytes.insert(0x47, 0x42);
                s.raw_bytes.insert(0x48, 0x07);
                let mut arr: [Option<GkSetup>; 10] = Default::default();
                arr[0] = Some(s);
                arr
            },
            unknown_bytes: BTreeMap::new(),
        };
        let frames = area.to_frames(0x10).unwrap();
        let back = SystemArea::from_frames(&frames);
        assert_eq!(back, area);
    }

    #[test]
    fn audio_level_wire_bytes_match_floorboard() {
        // FloorBoard customKnob.cpp:112-117: byte_hi = value/128, byte_lo = value%128.
        for (val, hi, lo) in [
            (0_u8, 0_u8, 0_u8),
            (127, 0, 0x7F),
            (128, 1, 0),
            (200, 1, 0x48),
        ] {
            let level = AudioLevel::new(val).unwrap();
            assert_eq!(level.to_two_bytes(), [hi, lo], "encode {val}");
            assert_eq!(
                AudioLevel::from_two_bytes(hi, lo),
                Some(level),
                "decode {hi:#x},{lo:#x}"
            );
        }
    }

    #[test]
    fn audio_level_roundtrip_full_range() {
        for v in 0..=200_u8 {
            let level = AudioLevel::new(v).unwrap();
            let area = SystemArea {
                player_level: Some(level),
                ..SystemArea::default()
            };
            let frames = area.to_frames(0x10).unwrap();
            let back = SystemArea::from_frames(&frames);
            assert_eq!(back.player_level, Some(level), "round-trip failed for {v}");
            // Both bytes should be consumed -- nothing leaks to unknown_bytes.
            assert!(
                back.unknown_bytes.is_empty(),
                "value {v} leaked: {:?}",
                back.unknown_bytes
            );
        }
        assert!(AudioLevel::new(201).is_none());
    }

    #[test]
    fn audio_level_rejects_invalid_bytes() {
        // hi byte > 1 is not a legal encoding.
        assert!(AudioLevel::from_two_bytes(2, 0).is_none());
        // hi byte high bit set (not a 7-bit data byte) is rejected.
        assert!(AudioLevel::from_two_bytes(0x80, 0).is_none());
        // hi=1, lo=0x49 would decode as 201 -- out of range.
        assert!(AudioLevel::from_two_bytes(1, 0x49).is_none());
    }

    #[test]
    fn audio_level_partial_bytes_fall_through_to_unknown() {
        // If only the high byte is present in a dump (no low byte), the high byte
        // must land in unknown_bytes rather than being silently dropped.
        let frames = vec![Frame::Dt1 {
            device_id: 0x10,
            address: ADDR_PLAYER_LEVEL_HI,
            data: std::borrow::Cow::Borrowed(&[0x01]),
        }];
        let area = SystemArea::from_frames(&frames);
        assert_eq!(area.player_level, None);
        assert_eq!(area.unknown_bytes.get("02:00:00:1B"), Some(&0x01));
    }

    #[test]
    fn exp_pedal_bend_handles_full_range() {
        for s in -24..=24 {
            let bend = ExpPedalBend::new(s).unwrap();
            assert_eq!(bend.get(), s);
            // Round-trip through byte
            let area = SystemArea {
                exp_pedal_bend: Some(bend),
                ..SystemArea::default()
            };
            let frames = area.to_frames(0x10).unwrap();
            let back = SystemArea::from_frames(&frames);
            assert_eq!(back.exp_pedal_bend, Some(bend));
        }
        assert!(ExpPedalBend::new(-25).is_none());
        assert!(ExpPedalBend::new(25).is_none());
    }

    #[test]
    fn master_tune_covers_435_to_445_hz() {
        for hz in 435..=445 {
            let mt = MasterTuneHz::new(hz).unwrap();
            assert_eq!(mt.get(), hz);
            let area = SystemArea {
                master_tune: Some(mt),
                ..SystemArea::default()
            };
            let frames = area.to_frames(0x10).unwrap();
            let back = SystemArea::from_frames(&frames);
            assert_eq!(back.master_tune, Some(mt));
        }
        assert!(MasterTuneHz::new(434).is_none());
        assert!(MasterTuneHz::new(446).is_none());
    }

    #[test]
    fn unknown_bytes_roundtrip_via_map() {
        let mut unknown = BTreeMap::new();
        unknown.insert("02:00:02:00".to_string(), 0x11_u8);
        let area = SystemArea {
            current_patch: Some(PatchSlot::user(1, 1).unwrap()),
            unknown_bytes: unknown.clone(),
            ..SystemArea::default()
        };
        let frames = area.to_frames(0x10).unwrap();
        let back = SystemArea::from_frames(&frames);
        assert_eq!(back.unknown_bytes, unknown);
        assert_eq!(back.current_patch, Some(PatchSlot::user(1, 1).unwrap()));
    }

    #[test]
    fn decodes_floorboard_system_syx_first_frame_as_user_20_1() {
        let _fixture_bytes = crate::test_support::fb_fixture_required("system.syx");
        let bytes: &[u8] = &_fixture_bytes;
        let frames: Vec<Frame<'_>> = parse_frames_unchecked(bytes)
            .map(|r| r.unwrap().0)
            .collect();
        let area = SystemArea::from_frames(&frames);
        // First frame: [01, 00, 00, 00] = 0x00 hi, [01, 00, 00, 01] = 0x39 lo.
        // 0*128 + 0x39 = 57 = (bank-1)*3 + (position-1) for User 20:1.
        assert_eq!(area.current_patch, Some(PatchSlot::user(20, 1).unwrap()));
    }

    #[test]
    fn yaml_roundtrip() {
        let area = SystemArea {
            current_patch: Some(PatchSlot::user(1, 1).unwrap()),
            midi_channel: Some(MidiChannel::new(1).unwrap()),
            output_select: Some(OutputSelect::LinePhones),
            ..SystemArea::default()
        };
        let yaml = serde_yaml::to_string(&area).unwrap();
        let back: SystemArea = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, area);
    }

    #[test]
    fn current_patch_wire_bytes_match_floorboard_enum() {
        // Spot-check against the midi.xml `<PARAM name="Current patch">` enum:
        // byte 0 = 0x00 with byte 1 = 0x00 → User 01:1 (the first entry).
        let area = SystemArea {
            current_patch: Some(PatchSlot::user(1, 1).unwrap()),
            ..SystemArea::default()
        };
        let frames = area.to_frames(0x10).unwrap();
        // Should encode as one DT1 with two payload bytes [0x00, 0x00].
        assert_eq!(frames.len(), 1);
        if let Frame::Dt1 { address, data, .. } = &frames[0] {
            assert_eq!(*address, ADDR_CURRENT_PATCH_HI);
            assert_eq!(data.as_ref(), &[0, 0]);
        } else {
            panic!("expected DT1");
        }

        // byte 0 = 0x00, byte 1 = 0x7F → idx 127 → User 43:2.
        let high_user = SystemArea {
            current_patch: Some(PatchSlot::user(43, 2).unwrap()),
            ..SystemArea::default()
        };
        let frames = high_user.to_frames(0x10).unwrap();
        if let Frame::Dt1 { data, .. } = &frames[0] {
            assert_eq!(data.as_ref(), &[0x00, 0x7F]);
        } else {
            panic!("expected DT1");
        }

        // byte 0 = 0x01, byte 1 = 0x00 → idx 128 → User 43:3.
        let cross_boundary = SystemArea {
            current_patch: Some(PatchSlot::user(43, 3).unwrap()),
            ..SystemArea::default()
        };
        let frames = cross_boundary.to_frames(0x10).unwrap();
        if let Frame::Dt1 { data, .. } = &frames[0] {
            assert_eq!(data.as_ref(), &[0x01, 0x00]);
        } else {
            panic!("expected DT1");
        }
    }

    #[test]
    fn current_patch_roundtrips_full_user_range() {
        for bank in 1..=99_u8 {
            for position in 1..=3_u8 {
                let slot = PatchSlot::user(bank, position).unwrap();
                let area = SystemArea {
                    current_patch: Some(slot),
                    ..SystemArea::default()
                };
                let frames = area.to_frames(0x10).unwrap();
                let back = SystemArea::from_frames(&frames);
                assert_eq!(back.current_patch, Some(slot), "{slot}");
            }
        }
    }

    #[test]
    fn nibble_pair_pins_floorboard_bcd_encoding() {
        // Per customDataKnob.cpp:106-128, value 0xC8 (=200) splits to
        // byte_hi = 0x0C (= the high hex digit, zero-padded as one byte)
        // byte_lo = 0x08 (= the low hex digit, zero-padded as one byte).
        for (v, hi, lo) in [
            (0_u8, 0_u8, 0_u8),
            (15, 0x00, 0x0F),
            (16, 0x01, 0x00),
            (75, 0x04, 0x0B),
            (200, 0x0C, 0x08),
            (255, 0x0F, 0x0F),
        ] {
            assert_eq!(encode_nibble_pair(v), [hi, lo], "encode {v}");
            assert_eq!(
                decode_nibble_pair(hi, lo),
                Some(v),
                "decode {hi:#x},{lo:#x}"
            );
        }
        // Reject bytes with bits 4..=7 set (not legal nibble bytes).
        assert!(decode_nibble_pair(0x10, 0x00).is_none());
        assert!(decode_nibble_pair(0x00, 0x80).is_none());
    }

    #[test]
    fn patch_level_roundtrip_full_range() {
        for v in 0..=200_u8 {
            let level = PatchLevel::new(v).unwrap();
            let area = SystemArea {
                patch_level: Some(level),
                ..SystemArea::default()
            };
            let frames = area.to_frames(0x10).unwrap();
            let back = SystemArea::from_frames(&frames);
            assert_eq!(back.patch_level, Some(level), "round-trip failed for {v}");
        }
        assert!(PatchLevel::new(201).is_none());
    }

    #[test]
    fn master_bpm_display_matches_floorboard_table() {
        // FloorBoard's <Tables> DATA value="06" PARAM range "00/FF/1536/1791":
        // display = 1536 + raw_byte (linear interpolation per
        // MidiTable.cpp:rangeToValue).
        assert_eq!(MasterBpm::new(0).display_value(), 1536);
        assert_eq!(MasterBpm::new(0x78).display_value(), 1656);
        assert_eq!(MasterBpm::new(0xFF).display_value(), 1791);
    }

    #[test]
    fn master_bpm_uses_distinct_wire_bytes_from_audio_level() {
        // patch_level (4-nibble) and player_level (14-bit) at value 200 produce
        // different wire bytes — this guards against accidentally swapping
        // the two encodings in a future refactor.
        let nibble = SystemArea {
            patch_level: Some(PatchLevel::new(200).unwrap()),
            ..SystemArea::default()
        };
        let mut nibble_bytes = std::collections::BTreeMap::new();
        for frame in nibble.to_frames(0x10).unwrap() {
            if let Frame::Dt1 { address, data, .. } = frame {
                for (i, b) in data.iter().enumerate() {
                    let mut a = address;
                    a[3] = address[3].wrapping_add(i as u8);
                    nibble_bytes.insert(a, *b);
                }
            }
        }
        assert_eq!(nibble_bytes.get(&ADDR_PATCH_LEVEL_HI), Some(&0x0C));
        assert_eq!(nibble_bytes.get(&ADDR_PATCH_LEVEL_LO), Some(&0x08));

        let mb = SystemArea {
            master_bpm: Some(MasterBpm::new(120)),
            ..SystemArea::default()
        };
        let mut bpm_bytes = std::collections::BTreeMap::new();
        for frame in mb.to_frames(0x10).unwrap() {
            if let Frame::Dt1 { address, data, .. } = frame {
                for (i, b) in data.iter().enumerate() {
                    let mut a = address;
                    a[3] = address[3].wrapping_add(i as u8);
                    bpm_bytes.insert(a, *b);
                }
            }
        }
        // 120 = 0x78 → high nibble 7, low nibble 8.
        assert_eq!(bpm_bytes.get(&ADDR_MASTER_BPM_HI), Some(&0x07));
        assert_eq!(bpm_bytes.get(&ADDR_MASTER_BPM_LO), Some(&0x08));
    }

    #[test]
    fn ctl_pedal_function_byte_symmetry() {
        // All 22 values 0x00..=0x15 must round-trip through to_byte/from_byte
        // and survive a SystemArea encode/decode cycle.
        for raw in 0x00_u8..=0x15 {
            let v = CtlPedalFunction::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "to_byte mismatch for 0x{raw:02X}");
            let area = SystemArea {
                ctl_pedal_function: Some(v),
                ..SystemArea::default()
            };
            let frames = area.to_frames(0x10).unwrap();
            let back = SystemArea::from_frames(&frames);
            assert_eq!(back.ctl_pedal_function, Some(v));
        }
        assert!(CtlPedalFunction::from_byte(0x16).is_none());
    }

    #[test]
    fn gk_setup_name_from_str_round_trips_typed() {
        use std::str::FromStr;
        let n = GkSetupName::from_str("Lead").unwrap();
        assert_eq!(n.as_string(), "Lead    ");
        let truncated = GkSetupName::from_str("0123456789").unwrap();
        assert_eq!(truncated.as_string(), "01234567");
        let bad = GkSetupName::from_str("hi\n").unwrap_err();
        assert_eq!(
            bad,
            GkSetupNameError::NonPrintable {
                char_index: 2,
                code: 0x0A
            }
        );
    }

    #[test]
    fn normal_pu_gain_roundtrips_full_range() {
        for db in -20_i8..=20 {
            let g = NormalPuGain::new(db).unwrap();
            assert_eq!(g.db(), db);
            assert_eq!(NormalPuGain::from_byte(g.to_byte()), Some(g));
        }
        assert!(NormalPuGain::new(-21).is_none());
        assert!(NormalPuGain::new(21).is_none());
        assert!(NormalPuGain::from_byte(41).is_none());
    }

    #[test]
    fn hex_pu_type_byte_symmetry() {
        for raw in 0x00_u8..=0x06 {
            let v = HexPuType::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw);
        }
        assert!(HexPuType::from_byte(0x07).is_none());
    }

    #[test]
    fn gk_setup_decode_splits_name_and_raw_bytes() {
        // Synthesize a system dump with GK setup 1 (Guitar + Bass fields
        // populated) and GK setup 3 (single raw byte). The decoder should
        // route each offset to its typed accessor and only fall through to
        // `raw_bytes` for offsets that are not yet typed (>= 0x41).
        let mut frames = Vec::new();
        let mut payload = vec![b'T', b'e', b's', b't'];
        payload.extend(std::iter::repeat(0x20).take(4)); // 8 chars total
        payload.push(HexPuType::Gk3.to_byte()); // 0x08 hex_pu_type
        payload.push(0x42); // 0x09 scale (raw)
        payload.push(0x11); // 0x0A lsb (raw)
        payload.push(PuPhase::Inverse.to_byte()); // 0x0B pu_phase
        payload.push(PuDirection::Reverse.to_byte()); // 0x0C pu_direction
        payload.push(S1S2Position::Reverse.to_byte()); // 0x0D s1_s2_position
        payload.push(NormalPuGain::new(-5).unwrap().to_byte()); // 0x0E
        payload.push(PiezoGain::new(3).unwrap().to_byte()); // 0x0F
        payload.push(PiezoGain::new(-2).unwrap().to_byte()); // 0x10
        payload.push(0x11); // 0x11 string_distance_bridge[0]
        payload.push(0x22); // 0x12 string_distance_bridge[1]
        payload.extend([0x33, 0x44, 0x55, 0x66]); // 0x13..0x16 (strings 3..6)
        payload.extend([0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC]); // 0x17..0x1C string_sensitivity
        payload.push(PlayFeel::new(3).unwrap().to_byte()); // 0x1D
        payload.push(Rating0To10::new(5).unwrap().to_byte()); // 0x1E low_velocity_cut
        payload.push(Rating1To10::new(7).unwrap().to_byte()); // 0x1F velocity_dynamics
        payload.push(Rating0To10::new(8).unwrap().to_byte()); // 0x20 nuance_dynamics
        payload.push(Rating0To10::new(2).unwrap().to_byte()); // 0x21 nuance_trim
        payload.push(DownShift::new(-3).unwrap().to_byte()); // 0x22 down_shift
                                                             // ---- Bass Mode block ----
        payload.extend([b'B', b'a', b's', b's']); // 0x23..0x26 bass name
        payload.extend([0x20, 0x20, 0x20, 0x20]); // 0x27..0x2A bass name pad
        payload.push(BassHexPuType::Gk2b.to_byte()); // 0x2B
        payload.push(0x09); // 0x2C bass scale (raw)
        payload.push(0x07); // 0x2D bass lsb (raw)
        payload.push(GkPuPosition::FiveStringLow2.to_byte()); // 0x2E
        payload.push(PuPhase::Inverse.to_byte()); // 0x2F bass pu_phase
        payload.push(PuDirection::Reverse.to_byte()); // 0x30 bass pu_direction
        payload.push(S1S2Position::Reverse.to_byte()); // 0x31 bass s1_s2
        payload.push(NormalPuGain::new(4).unwrap().to_byte()); // 0x32
        payload.push(PiezoGain::new(-1).unwrap().to_byte()); // 0x33 bass piezo_low
        payload.push(PiezoGain::new(2).unwrap().to_byte()); // 0x34 bass piezo_high
        payload.extend([0x10, 0x20, 0x30, 0x40, 0x50, 0x60]); // 0x35..0x3A bass distance
        payload.extend([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]); // 0x3B..0x40 bass sensitivity
        payload.push(PlayFeel::new(2).unwrap().to_byte()); // 0x41 bass play_feel
        payload.push(Rating0To10::new(4).unwrap().to_byte()); // 0x42 bass low_vel_cut
        payload.push(Rating1To10::new(6).unwrap().to_byte()); // 0x43 bass vel_dyn
        payload.push(Rating0To10::new(7).unwrap().to_byte()); // 0x44 bass nuance_dyn
        payload.push(Rating0To10::new(1).unwrap().to_byte()); // 0x45 bass nuance_trim
        payload.push(DownShift::new(-2).unwrap().to_byte()); // 0x46 bass down_shift
        payload.push(0xEE); // 0x47 still-raw straggler (unmapped slot)
        frames.push(Frame::Dt1 {
            device_id: 0x10,
            address: [0x02, 0x04, 0x00, 0x00],
            data: std::borrow::Cow::Owned(payload),
        });
        // Develop Low Cut tail for setup 1.
        frames.push(Frame::Dt1 {
            device_id: 0x10,
            address: [0x02, 0x04, 0x00, 0x51],
            data: std::borrow::Cow::Owned(vec![0x32, 0x14]), // 0x51=50, 0x52=20
        });
        // GK setup 3 (sub-LSB 0x06): just one byte at offset 0x47 (still raw).
        // Offsets land in address[3] per the page-0 frame convention.
        frames.push(Frame::Dt1 {
            device_id: 0x10,
            address: [0x02, 0x06, 0x00, 0x47],
            data: std::borrow::Cow::Owned(vec![0x55]),
        });
        let area = SystemArea::from_frames(&frames);

        let setup_1 = area.gk_setups[0].as_ref().expect("setup 1 should decode");
        assert_eq!(setup_1.name.as_string().trim(), "Test");
        assert_eq!(setup_1.hex_pu_type, Some(HexPuType::Gk3));
        assert_eq!(setup_1.scale, Some(0x42));
        assert_eq!(setup_1.lsb, Some(0x11));
        assert_eq!(setup_1.pu_phase, Some(PuPhase::Inverse));
        assert_eq!(setup_1.pu_direction, Some(PuDirection::Reverse));
        assert_eq!(setup_1.s1_s2_position, Some(S1S2Position::Reverse));
        assert_eq!(setup_1.normal_pu_gain.unwrap().db(), -5);
        assert_eq!(setup_1.piezo_low.unwrap().db(), 3);
        assert_eq!(setup_1.piezo_high.unwrap().db(), -2);
        assert_eq!(setup_1.string_distance_bridge[0], Some(0x11));
        assert_eq!(setup_1.string_distance_bridge[5], Some(0x66));
        assert_eq!(setup_1.string_sensitivity[0], Some(0x77));
        assert_eq!(setup_1.string_sensitivity[5], Some(0xCC));
        assert_eq!(setup_1.play_feel.unwrap().get(), 3);
        assert_eq!(setup_1.low_velocity_cut.unwrap().get(), 5);
        assert_eq!(setup_1.velocity_dynamics.unwrap().get(), 7);
        assert_eq!(setup_1.nuance_dynamics.unwrap().get(), 8);
        assert_eq!(setup_1.nuance_trim.unwrap().get(), 2);
        assert_eq!(setup_1.down_shift.unwrap().semitones(), -3);
        // Bass Mode assertions
        assert_eq!(setup_1.bass_name.as_string().trim(), "Bass");
        assert_eq!(setup_1.bass_hex_pu_type, Some(BassHexPuType::Gk2b));
        assert_eq!(setup_1.bass_scale, Some(0x09));
        assert_eq!(setup_1.bass_lsb, Some(0x07));
        assert_eq!(
            setup_1.bass_gk_pu_position,
            Some(GkPuPosition::FiveStringLow2)
        );
        assert_eq!(setup_1.bass_pu_phase, Some(PuPhase::Inverse));
        assert_eq!(setup_1.bass_pu_direction, Some(PuDirection::Reverse));
        assert_eq!(setup_1.bass_s1_s2_position, Some(S1S2Position::Reverse));
        assert_eq!(setup_1.bass_normal_pu_gain.unwrap().db(), 4);
        assert_eq!(setup_1.bass_piezo_low.unwrap().db(), -1);
        assert_eq!(setup_1.bass_piezo_high.unwrap().db(), 2);
        assert_eq!(setup_1.bass_string_distance_bridge[0], Some(0x10));
        assert_eq!(setup_1.bass_string_distance_bridge[5], Some(0x60));
        assert_eq!(setup_1.bass_string_sensitivity[0], Some(0x01));
        assert_eq!(setup_1.bass_string_sensitivity[5], Some(0x06));
        assert_eq!(setup_1.bass_play_feel.unwrap().get(), 2);
        assert_eq!(setup_1.bass_low_velocity_cut.unwrap().get(), 4);
        assert_eq!(setup_1.bass_velocity_dynamics.unwrap().get(), 6);
        assert_eq!(setup_1.bass_nuance_dynamics.unwrap().get(), 7);
        assert_eq!(setup_1.bass_nuance_trim.unwrap().get(), 1);
        assert_eq!(setup_1.bass_down_shift.unwrap().semitones(), -2);
        assert_eq!(setup_1.develop_low_cut_guitar, Some(50));
        assert_eq!(setup_1.develop_low_cut_bass, Some(20));
        assert_eq!(setup_1.raw_bytes.get(&0x47), Some(&0xEE));

        // Setup 2 (index 1, sub-LSB 0x05) wasn't in the dump.
        assert!(area.gk_setups[1].is_none());

        let setup_3 = area.gk_setups[2].as_ref().expect("setup 3 should decode");
        // Only a single byte landed in raw_bytes; name stayed at the default pad.
        assert!(setup_3.name.is_empty());
        assert!(setup_3.bass_name.is_empty());
        assert_eq!(setup_3.raw_bytes.get(&0x47), Some(&0x55));

        // Round-trip: re-encode the area and verify the bytes survive.
        let back = SystemArea::from_frames(&area.to_frames(0x10).unwrap());
        assert_eq!(back.gk_setups, area.gk_setups);
    }

    #[test]
    fn exp_pedal_switch_function_byte_symmetry() {
        for raw in 0x00_u8..=0x14 {
            let v = ExpPedalSwitchFunction::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "to_byte mismatch for 0x{raw:02X}");
            let area = SystemArea {
                exp_pedal_switch_function: Some(v),
                ..SystemArea::default()
            };
            let frames = area.to_frames(0x10).unwrap();
            let back = SystemArea::from_frames(&frames);
            assert_eq!(back.exp_pedal_switch_function, Some(v));
        }
        assert!(ExpPedalSwitchFunction::from_byte(0x15).is_none());
    }

    #[test]
    fn exp_pedal_function_byte_symmetry() {
        for raw in 0x00_u8..=0x0A {
            let v = ExpPedalFunction::from_byte(raw).expect("from_byte");
            assert_eq!(v.to_byte(), raw, "to_byte mismatch for 0x{raw:02X}");
            let area = SystemArea {
                exp_pedal_off_function: Some(v),
                exp_pedal_on_function: Some(v),
                ..SystemArea::default()
            };
            let frames = area.to_frames(0x10).unwrap();
            let back = SystemArea::from_frames(&frames);
            assert_eq!(back.exp_pedal_off_function, Some(v));
            assert_eq!(back.exp_pedal_on_function, Some(v));
        }
        assert!(ExpPedalFunction::from_byte(0x0B).is_none());
    }

    #[test]
    fn current_patch_roundtrips_first_and_last_preset() {
        for slot in [
            PatchSlot::preset(100, 1).unwrap(),
            PatchSlot::preset(189, 3).unwrap(),
        ] {
            let area = SystemArea {
                current_patch: Some(slot),
                ..SystemArea::default()
            };
            let frames = area.to_frames(0x10).unwrap();
            let back = SystemArea::from_frames(&frames);
            assert_eq!(back.current_patch, Some(slot), "{slot}");
        }
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
