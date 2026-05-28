//! Typed GR-55 SysEx address space + patch-slot identity.
//!
//! Address scheme reverse-engineered from FloorBoard's `MidiTable::patchRequest`
//! (banks/patches → wire address) and `globalVariables.h` constants.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Number of patches per bank — 3 on the GR-55 (positions A, B, C).
pub const PATCHES_PER_BANK: u8 = 3;

/// Number of USER banks the GR-55 exposes (USER patches: 1..=99, 3 positions each = 297 slots).
pub const USER_BANKS: u8 = 99;

/// Number of total banks tracked by FloorBoard (USER + PRESET banks).
/// FloorBoard's `bankTotalAll = 189`; corresponds to GR-55 firmware up to ~v1.50.
pub const TOTAL_BANKS: u8 = 189;

/// Linear gap inserted between USER and PRESET patch indices when computing
/// the wire address. See `MidiTable::patchRequest` in FloorBoard.
const USER_PRESET_INDEX_GAP: u32 = 87;

/// Each 128-slot address row is encoded as one MSB increment in the wire address.
const SLOTS_PER_ADDRESS_ROW: u32 = 128;

/// Base MSB for the first row of USER patches.
const PATCH_BASE_MSB: u8 = 0x20;

/// MSB of the live "current patch being edited" write area (`tempDataWrite` in FloorBoard).
pub const TEMP_WRITE_MSB: u8 = 0x18;

/// MSB of the bulk/individual edit-buffer read area.
pub const TEMP_BUFFER_MSB: u8 = 0x60;

/// MSBs that hold System-area parameters (`<System>` in midi.xml uses both).
pub const SYSTEM_MSBS: &[u8] = &[0x01, 0x02];

/// A USER or PRESET patch slot identified by bank (1..) and position (1..3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatchSlot {
    User { bank: u8, position: u8 },
    Preset { bank: u8, position: u8 },
}

impl PatchSlot {
    pub fn user(bank: u8, position: u8) -> Result<Self, PatchSlotError> {
        if !(1..=USER_BANKS).contains(&bank) {
            return Err(PatchSlotError::BankOutOfRange {
                bank,
                max: USER_BANKS,
            });
        }
        if !(1..=PATCHES_PER_BANK).contains(&position) {
            return Err(PatchSlotError::PositionOutOfRange { position });
        }
        Ok(PatchSlot::User { bank, position })
    }

    pub fn preset(bank: u8, position: u8) -> Result<Self, PatchSlotError> {
        if !((USER_BANKS + 1)..=TOTAL_BANKS).contains(&bank) {
            return Err(PatchSlotError::BankOutOfRange {
                bank,
                max: TOTAL_BANKS,
            });
        }
        if !(1..=PATCHES_PER_BANK).contains(&position) {
            return Err(PatchSlotError::PositionOutOfRange { position });
        }
        Ok(PatchSlot::Preset { bank, position })
    }

    pub fn bank(&self) -> u8 {
        match *self {
            PatchSlot::User { bank, .. } | PatchSlot::Preset { bank, .. } => bank,
        }
    }

    pub fn position(&self) -> u8 {
        match *self {
            PatchSlot::User { position, .. } | PatchSlot::Preset { position, .. } => position,
        }
    }

    /// Linear patch index used by FloorBoard's address math and by the
    /// System area's Current Patch encoding.
    /// USER slots: `(bank-1) * 3 + (position-1)`, range `0..=296`.
    /// PRESET slots: same formula plus the constant `USER_PRESET_INDEX_GAP`
    /// offset, so PRESET 100:1 lands at index `384`.
    pub fn linear_index(&self) -> u32 {
        let raw = (u32::from(self.bank()) - 1) * u32::from(PATCHES_PER_BANK)
            + u32::from(self.position())
            - 1;
        match self {
            PatchSlot::User { .. } => raw,
            PatchSlot::Preset { .. } => raw + USER_PRESET_INDEX_GAP,
        }
    }

    /// Inverse of [`linear_index`]: recover a `PatchSlot` from its global
    /// patch index. The gap `297..384` between USER and PRESET ranges yields
    /// `None` (matches FloorBoard's per-knob enum which fills it with `void`).
    pub fn from_linear_index(idx: u32) -> Option<Self> {
        let patches_per_bank = u32::from(PATCHES_PER_BANK);
        let user_slot_count = u32::from(USER_BANKS) * patches_per_bank;
        let preset_bank_count = u32::from(TOTAL_BANKS - USER_BANKS);
        let preset_slot_count = preset_bank_count * patches_per_bank;
        let preset_start = user_slot_count + USER_PRESET_INDEX_GAP;
        let preset_end = preset_start + preset_slot_count;

        if idx < user_slot_count {
            let bank = (idx / patches_per_bank + 1) as u8;
            let position = (idx % patches_per_bank + 1) as u8;
            Some(PatchSlot::User { bank, position })
        } else if (preset_start..preset_end).contains(&idx) {
            let preset_idx = idx - preset_start;
            let bank = (preset_idx / patches_per_bank) as u8 + USER_BANKS + 1;
            let position = (preset_idx % patches_per_bank + 1) as u8;
            Some(PatchSlot::Preset { bank, position })
        } else {
            None
        }
    }

    /// Wire address of this slot's base. Address bytes 2 and 3 are always `0x00`.
    pub fn address(&self) -> [u8; 4] {
        let idx = self.linear_index();
        let row = idx / SLOTS_PER_ADDRESS_ROW;
        let col = idx % SLOTS_PER_ADDRESS_ROW;
        [PATCH_BASE_MSB + row as u8, col as u8, 0x00, 0x00]
    }

    /// Inverse of [`address`]: recover a `PatchSlot` from its wire address.
    /// Returns `None` for addresses outside the patch area.
    pub fn from_address(address: [u8; 4]) -> Option<Self> {
        if address[0] < PATCH_BASE_MSB {
            return None;
        }
        if address[2] != 0 || address[3] != 0 || address[1] >= 0x80 {
            return None;
        }
        let row = u32::from(address[0] - PATCH_BASE_MSB);
        let col = u32::from(address[1]);
        let idx = row * SLOTS_PER_ADDRESS_ROW + col;
        Self::from_linear_index(idx)
    }
}

/// `User 01:1`, `User 99:3`, `Preset 100:1`, etc. Matches FloorBoard's
/// `<PARAM name="User 01:1"/>` convention in midi.xml's Current-Patch enum.
impl fmt::Display for PatchSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            PatchSlot::User { .. } => "User",
            PatchSlot::Preset { .. } => "Preset",
        };
        write!(f, "{kind} {:02}:{}", self.bank(), self.position())
    }
}

impl FromStr for PatchSlot {
    type Err = PatchSlotError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind, rest) = s
            .split_once(' ')
            .ok_or_else(|| PatchSlotError::Parse(s.to_string()))?;
        let (bank_s, pos_s) = rest
            .split_once(':')
            .ok_or_else(|| PatchSlotError::Parse(s.to_string()))?;
        let bank: u8 = bank_s
            .parse()
            .map_err(|_| PatchSlotError::Parse(s.to_string()))?;
        let position: u8 = pos_s
            .parse()
            .map_err(|_| PatchSlotError::Parse(s.to_string()))?;
        match kind {
            "User" => PatchSlot::user(bank, position),
            "Preset" => PatchSlot::preset(bank, position),
            _ => Err(PatchSlotError::Parse(s.to_string())),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PatchSlotError {
    #[error("bank {bank} out of range (max {max})")]
    BankOutOfRange { bank: u8, max: u8 },

    #[error("patch position {position} out of range (must be 1..=3)")]
    PositionOutOfRange { position: u8 },

    #[error("could not parse patch slot from {0:?}")]
    Parse(String),
}

/// Classification of a 4-byte SysEx address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "area")]
pub enum AddressSpace {
    /// System parameters (MSB 0x01 or 0x02).
    System,
    /// One of the addressable USER patches at this base.
    UserPatch(PatchSlot),
    /// One of the addressable PRESET patches at this base.
    PresetPatch(PatchSlot),
    /// `tempDataWrite` — the GR-55's live current-patch write/edit area (MSB 0x18).
    TempWriteBuffer,
    /// `Temporary Buffer (Bulk)` / `(Individual)` — read mirror of current patch (MSB 0x60).
    TempReadBuffer,
    /// MSB outside any documented region.
    Unknown,
}

impl AddressSpace {
    pub fn classify(address: [u8; 4]) -> Self {
        if SYSTEM_MSBS.contains(&address[0]) {
            return AddressSpace::System;
        }
        if address[0] == TEMP_WRITE_MSB {
            return AddressSpace::TempWriteBuffer;
        }
        if address[0] == TEMP_BUFFER_MSB {
            return AddressSpace::TempReadBuffer;
        }
        if let Some(slot) = PatchSlot::from_address(address) {
            return match slot {
                PatchSlot::User { .. } => AddressSpace::UserPatch(slot),
                PatchSlot::Preset { .. } => AddressSpace::PresetPatch(slot),
            };
        }
        AddressSpace::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_user_slot_addresses_zero() {
        let slot = PatchSlot::user(1, 1).unwrap();
        assert_eq!(slot.address(), [0x20, 0x00, 0x00, 0x00]);
        assert_eq!(slot.to_string(), "User 01:1");
    }

    #[test]
    fn user_43_2_crosses_msb_boundary() {
        // patchOffset for (43, 2) = (43-1)*3 + 2 - 1 = 127.
        // 127 < 128 → still on MSB 0x20.
        let slot = PatchSlot::user(43, 2).unwrap();
        assert_eq!(slot.address(), [0x20, 0x7F, 0x00, 0x00]);
        // (43, 3) → patchOffset 128 → spills onto MSB 0x21.
        let slot = PatchSlot::user(43, 3).unwrap();
        assert_eq!(slot.address(), [0x21, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn last_user_slot_addresses_correctly() {
        // patchOffset for (99, 3) = (99-1)*3 + 3 - 1 = 296 = 2*128 + 40.
        let slot = PatchSlot::user(99, 3).unwrap();
        assert_eq!(slot.address(), [0x22, 0x28, 0x00, 0x00]);
    }

    #[test]
    fn first_preset_slot_addresses_after_gap() {
        // patchOffset for (100, 1) = 99*3 + 87 = 297 + 87 = 384 = 3*128 + 0.
        let slot = PatchSlot::preset(100, 1).unwrap();
        assert_eq!(slot.address(), [0x23, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn round_trip_via_address() {
        let cases = [
            PatchSlot::User {
                bank: 1,
                position: 1,
            },
            PatchSlot::User {
                bank: 43,
                position: 2,
            },
            PatchSlot::User {
                bank: 43,
                position: 3,
            },
            PatchSlot::User {
                bank: 99,
                position: 3,
            },
            PatchSlot::Preset {
                bank: 100,
                position: 1,
            },
            PatchSlot::Preset {
                bank: 150,
                position: 2,
            },
            PatchSlot::Preset {
                bank: 189,
                position: 3,
            },
        ];
        for slot in cases {
            let addr = slot.address();
            assert_eq!(
                PatchSlot::from_address(addr),
                Some(slot),
                "round-trip failed for {slot} → {addr:02X?}"
            );
        }
    }

    #[test]
    fn from_address_returns_none_for_non_patch_areas() {
        assert!(PatchSlot::from_address([0x01, 0x00, 0x00, 0x00]).is_none());
        assert!(PatchSlot::from_address([0x18, 0x00, 0x00, 0x00]).is_none());
        assert!(PatchSlot::from_address([0x60, 0x00, 0x00, 0x00]).is_none());
        // Address bytes 2/3 non-zero → not a patch base.
        assert!(PatchSlot::from_address([0x20, 0x00, 0x00, 0x01]).is_none());
    }

    #[test]
    fn parse_roundtrip() {
        for slot in [
            PatchSlot::User {
                bank: 1,
                position: 1,
            },
            PatchSlot::User {
                bank: 99,
                position: 3,
            },
            PatchSlot::Preset {
                bank: 100,
                position: 1,
            },
        ] {
            let s = slot.to_string();
            let parsed: PatchSlot = s.parse().unwrap();
            assert_eq!(parsed, slot);
        }
    }

    #[test]
    fn user_constructors_reject_bad_bank() {
        assert!(PatchSlot::user(0, 1).is_err());
        assert!(PatchSlot::user(100, 1).is_err());
        assert!(PatchSlot::user(1, 0).is_err());
        assert!(PatchSlot::user(1, 4).is_err());
    }

    #[test]
    fn preset_constructors_reject_user_range() {
        assert!(PatchSlot::preset(99, 1).is_err());
        assert!(PatchSlot::preset(100, 1).is_ok());
        assert!(PatchSlot::preset(190, 1).is_err());
    }

    #[test]
    fn classify_known_addresses() {
        assert_eq!(
            AddressSpace::classify([0x01, 0x00, 0x00, 0x00]),
            AddressSpace::System
        );
        assert_eq!(
            AddressSpace::classify([0x02, 0x00, 0x02, 0x00]),
            AddressSpace::System
        );
        assert_eq!(
            AddressSpace::classify([0x18, 0x00, 0x00, 0x00]),
            AddressSpace::TempWriteBuffer
        );
        assert_eq!(
            AddressSpace::classify([0x60, 0x00, 0x00, 0x00]),
            AddressSpace::TempReadBuffer
        );
        assert!(matches!(
            AddressSpace::classify([0x20, 0x00, 0x00, 0x00]),
            AddressSpace::UserPatch(PatchSlot::User {
                bank: 1,
                position: 1
            })
        ));
        assert!(matches!(
            AddressSpace::classify([0x23, 0x00, 0x00, 0x00]),
            AddressSpace::PresetPatch(PatchSlot::Preset {
                bank: 100,
                position: 1
            })
        ));
        assert_eq!(
            AddressSpace::classify([0x7F, 0x00, 0x00, 0x00]),
            AddressSpace::Unknown
        );
    }
}
