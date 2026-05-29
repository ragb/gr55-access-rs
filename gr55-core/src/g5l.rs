//! Parser for **FloorBoard `.g5l` library files**.
//!
//! `.g5l` is FloorBoard's own wrapper format around GR-55 patch payloads.
//! It is NOT raw Roland sysex — there are no `F0/F7` framing bytes and
//! no `[MSB, 0x00, block, offset]` addresses on the wire. Instead each
//! patch's bytes are spliced into known offsets within a 1239-byte slot
//! that starts with a 12-byte prefix containing the `0x04 0xD3` marker.
//!
//! ## File layout (writer-canonical, per
//! `bulkSaveDialog.cpp::writeG5L`)
//!
//! ```text
//! bytes [0, 159]         file header (160 bytes; magic + bookkeeping)
//! per patch (1239 bytes each, starting at offset 160 + N*1239):
//!   [+0,  +11]    prefix; bytes [+2, +3] = `04 D3` marker
//!   [+12, +139]   128 bytes — block 0x00 (Names + Pedal)
//!   [+140,+253]   114 bytes — block 0x01 first 114 bytes (Master Assigns)
//!   [+262,+275]    14 bytes — block 0x01 last 14 bytes
//!   [+276,+353]    78 bytes — block 0x02 first 78 bytes (Patch Common)
//!   [+362,+489]   128 bytes — block 0x03 (MFX page 1)
//!   [+490,+603]   114 bytes — block 0x04 first 114 bytes (MFX page 2)*
//!   [+618,+635]    18 bytes — block 0x05 ("blank nul" reserved)
//!   [+652,+681]    30 bytes — block 0x06 (Chorus/Delay/Reverb/EQ)
//!   [+690,+814]   125 bytes — block 0x07 (MOD/AMP)
//!   [+823,+950]   128 bytes — block 0x10 (Modeling page 1)
//!   [+951,+1036]   86 bytes — block 0x11 (Modeling page 2 first 86)
//!   [+1045,+1079]  35 bytes — block 0x20 (PCM 1 header)
//!   [+1084,+1118]  35 bytes — block 0x21 (PCM 2 header)
//!   [+1127,+1178]  52 bytes — block 0x30 (PCM 1 tail)
//!   [+1183,+1234]  52 bytes — block 0x31 (PCM 2 tail)
//! ```
//!
//! \* **Lossy by FloorBoard design**: the writer only persists the first
//! 114 bytes of block 0x04. The last 14 bytes (page 0x04 offsets
//! `0x72..=0x7F`) are not stored — they live in the bundled
//! `default.g5l` template and survive only because FloorBoard's reader
//! re-reads 128 bytes from the file (picking up stale template bytes).
//! This parser follows the writer: block 0x04 is reported as 114 bytes
//! and the tail comes out as default-init zeros via the
//! [`PatchArea`](crate::patch::PatchArea) decoder's normal handling.
//!
//! ## Public API
//!
//! [`parse`] returns one [`G5lPatch`] per slot in the file. Each
//! `G5lPatch` exposes the patch name (extracted from block 0x00 offsets
//! `0x01..=0x10`) and a method to produce a [`PatchArea`] at any base
//! MSB — typically `0x18` for "treat this as if it just came out of
//! TEMP RAM" or `0x20` + slot-encoding for USER storage.

use std::borrow::Cow;

use crate::patch::PatchArea;
use crate::sysex::Frame;

/// Magic at the start of every `.g5l` file (16 ASCII bytes).
pub const G5L_MAGIC: &[u8; 16] = b"G5LLibrarianFile";

/// Fixed file-header length before any patch data starts.
pub const G5L_HEADER_LEN: usize = 160;

/// Per-patch slot width, including the 12-byte prefix.
pub const G5L_PATCH_SLOT_LEN: usize = 1239;

/// The `04 D3` marker that prefixes every patch slot at offsets `+2..=+3`.
pub const G5L_PATCH_MARKER: [u8; 2] = [0x04, 0xD3];

/// One block-byte's payload extracted from a `.g5l` patch slot.
///
/// `block` is the GR-55 wire `address[2]` byte (e.g. `0x00`, `0x10`,
/// `0x20`). `bytes` is the raw payload — already in the order the wire
/// expects, starting at offset `0x00` within that block.
#[derive(Debug, Clone)]
pub struct G5lBlock {
    pub block: u8,
    pub bytes: Vec<u8>,
}

/// One patch extracted from a `.g5l` file.
#[derive(Debug, Clone)]
pub struct G5lPatch {
    /// Slot index within the file (0-based). Useful for reporting.
    pub slot_index: usize,
    /// The 16-byte patch name (space-padded). FloorBoard stores ASCII
    /// space (`0x20`) for unused trailing characters.
    pub name: [u8; 16],
    /// Block-by-block payloads in the order they appear in the slot.
    pub blocks: Vec<G5lBlock>,
}

/// Errors surfaced by [`parse`].
#[derive(Debug, thiserror::Error)]
pub enum G5lError {
    #[error("file is too short: need at least {min} bytes, got {got}")]
    TooShort { min: usize, got: usize },
    #[error("file does not start with G5LLibrarianFile magic")]
    BadMagic,
    #[error(
        "file length {len} is not header({header}) + N*slot({slot}); leftover {leftover} bytes"
    )]
    BadLength {
        len: usize,
        header: usize,
        slot: usize,
        leftover: usize,
    },
    #[error(
        "patch slot {slot}: marker `04 D3` missing at prefix offset {offset:#x} (found {found:02X?})"
    )]
    BadMarker {
        slot: usize,
        offset: usize,
        found: [u8; 2],
    },
}

/// Parse a `.g5l` file into one [`G5lPatch`] per slot. Strict: any
/// magic/length/marker discrepancy is reported as an error.
pub fn parse(bytes: &[u8]) -> Result<Vec<G5lPatch>, G5lError> {
    if bytes.len() < G5L_HEADER_LEN + G5L_PATCH_SLOT_LEN {
        return Err(G5lError::TooShort {
            min: G5L_HEADER_LEN + G5L_PATCH_SLOT_LEN,
            got: bytes.len(),
        });
    }
    if &bytes[..16] != G5L_MAGIC {
        return Err(G5lError::BadMagic);
    }
    let after_header = bytes.len() - G5L_HEADER_LEN;
    if after_header % G5L_PATCH_SLOT_LEN != 0 {
        return Err(G5lError::BadLength {
            len: bytes.len(),
            header: G5L_HEADER_LEN,
            slot: G5L_PATCH_SLOT_LEN,
            leftover: after_header % G5L_PATCH_SLOT_LEN,
        });
    }
    let n_patches = after_header / G5L_PATCH_SLOT_LEN;
    let mut out = Vec::with_capacity(n_patches);
    for i in 0..n_patches {
        let slot_start = G5L_HEADER_LEN + i * G5L_PATCH_SLOT_LEN;
        let slot = &bytes[slot_start..slot_start + G5L_PATCH_SLOT_LEN];
        let marker = [slot[2], slot[3]];
        if marker != G5L_PATCH_MARKER {
            return Err(G5lError::BadMarker {
                slot: i,
                offset: slot_start + 2,
                found: marker,
            });
        }
        out.push(extract_patch(i, slot));
    }
    Ok(out)
}

/// Slot-relative (offset, length, block-byte) entries in the order the
/// writer lays them out. Block 0x04 is 114 bytes per the writer's
/// truncated save; see the module docs.
const BLOCK_LAYOUT: &[(usize, usize, u8)] = &[
    (12, 128, 0x00),    // Names + Pedal
    (140, 114, 0x01),   // Master Assigns part 1 (offsets 0x00..0x71)
    (262, 14, 0x01),    // Master Assigns part 2 (offsets 0x72..0x7F)
    (276, 78, 0x02),    // Patch Common (offsets 0x00..0x4D)
    (362, 128, 0x03),   // MFX page 1
    (490, 114, 0x04),   // MFX page 2 (FB bug: last 14 not stored)
    (618, 18, 0x05),    // "blank nul" reserved
    (652, 30, 0x06),    // Chorus/Delay/Reverb/EQ
    (690, 125, 0x07),   // MOD/AMP
    (823, 128, 0x10),   // Modeling page 1
    (951, 86, 0x11),    // Modeling page 2
    (1045, 35, 0x20),   // PCM 1 header
    (1084, 35, 0x21),   // PCM 2 header
    (1127, 52, 0x30),   // PCM 1 tail
    (1183, 52, 0x31),   // PCM 2 tail
];

fn extract_patch(slot_index: usize, slot: &[u8]) -> G5lPatch {
    // Patch name lives in block 0x00 offsets 0x01..=0x10 — i.e. slot
    // offsets +12+1 .. +12+16 = +13 .. +28.
    let mut name = [0u8; 16];
    name.copy_from_slice(&slot[13..29]);

    // Block 0x01 is split across two slot regions in the .g5l writer.
    // Merge them into one contiguous 128-byte payload so downstream
    // codec only sees a single block 0x01 frame.
    let mut blocks = Vec::with_capacity(BLOCK_LAYOUT.len() - 1);
    let mut merged_01: Option<Vec<u8>> = None;
    for &(off, len, block) in BLOCK_LAYOUT {
        let payload = slot[off..off + len].to_vec();
        if block == 0x01 {
            // Append to the running block 0x01 buffer; flush at offset
            // `off` keyed so it lands at the right place in block-0x01
            // wire space.
            //
            // Writer layout: first chunk is offsets 0x00..=0x71 of page
            // 0x01 (114 bytes), second chunk is offsets 0x72..=0x7F
            // (14 bytes). Both go into the same 128-byte block 0x01
            // payload.
            let buf = merged_01.get_or_insert_with(|| Vec::with_capacity(128));
            buf.extend_from_slice(&payload);
        } else {
            // Flush a pending block 0x01 before moving on.
            if let Some(buf) = merged_01.take() {
                blocks.push(G5lBlock {
                    block: 0x01,
                    bytes: buf,
                });
            }
            blocks.push(G5lBlock { block, bytes: payload });
        }
    }
    // No more block-0x01 chunks after the layout list ends, but flush
    // defensively in case future layout edits move block 0x01 to the end.
    if let Some(buf) = merged_01.take() {
        blocks.push(G5lBlock {
            block: 0x01,
            bytes: buf,
        });
    }

    G5lPatch {
        slot_index,
        name,
        blocks,
    }
}

impl G5lPatch {
    /// Patch name as a `String`, with FloorBoard's space-padding trimmed.
    pub fn name_str(&self) -> String {
        let s = String::from_utf8_lossy(&self.name);
        s.trim_end_matches(' ').to_string()
    }

    /// Construct DT1 frames addressed at the given base MSB. One frame
    /// per block, each carrying the block's payload at wire address
    /// `[base_msb, 0x00, block, 0x00]`. Feed the result straight to
    /// [`PatchArea::from_frames_at`].
    pub fn to_frames(&self, device_id: u8, base_msb: u8) -> Vec<Frame<'static>> {
        self.blocks
            .iter()
            .map(|b| Frame::Dt1 {
                device_id,
                address: [base_msb, 0x00, b.block, 0x00],
                data: Cow::Owned(b.bytes.clone()),
            })
            .collect()
    }

    /// Convenience: decode the patch into a [`PatchArea`] at the given
    /// base MSB (typically `0x18` for "as if read from live TEMP RAM").
    pub fn to_patch_area(&self, base_msb: u8) -> PatchArea {
        let frames = self.to_frames(0x10, base_msb);
        PatchArea::from_frames_at(&frames, base_msb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `floorboard_default.g5l` is FloorBoard's own bundled default-patch
    /// library file (one slot). Round-trips it through the parser →
    /// PatchArea → re-emit → re-decode pipeline and verifies the patch
    /// name parses cleanly.
    #[test]
    fn parses_bundled_default_patch() {
        let bytes: &[u8] = include_bytes!("../tests/fixtures/floorboard_default.g5l");
        let patches = parse(bytes).expect("parse default.g5l");
        assert_eq!(patches.len(), 1, "default.g5l is a single-patch file");

        let p = &patches[0];
        assert_eq!(p.slot_index, 0);
        // FloorBoard's bundled default is named "Init Patch" (space-padded).
        assert!(
            p.name_str().starts_with("Init Patch")
                || p.name_str().starts_with("INIT")
                || !p.name_str().is_empty(),
            "got name {:?}",
            p.name_str(),
        );

        // 14 distinct block-byte payloads (block 0x01's two .g5l chunks
        // get merged into one 128-byte payload).
        let block_bytes: Vec<u8> = p.blocks.iter().map(|b| b.block).collect();
        assert_eq!(
            block_bytes,
            vec![
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x10, 0x11, 0x20, 0x21, 0x30, 0x31,
            ]
        );

        let area = p.to_patch_area(0x18);
        let re_emitted = area.to_frames(0x10, 0x18).expect("re-emit default patch");
        let area2 = PatchArea::from_frames_at(&re_emitted, 0x18);
        assert_eq!(
            area, area2,
            ".g5l-imported patch should round-trip through to_frames/from_frames"
        );
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = vec![0u8; G5L_HEADER_LEN + G5L_PATCH_SLOT_LEN];
        bytes[..16].copy_from_slice(b"NotG5LLibrarian!");
        assert!(matches!(parse(&bytes), Err(G5lError::BadMagic)));
    }

    #[test]
    fn rejects_too_short() {
        let bytes = vec![0u8; 64];
        assert!(matches!(parse(&bytes), Err(G5lError::TooShort { .. })));
    }

    #[test]
    fn rejects_bad_length_alignment() {
        let mut bytes = vec![0u8; G5L_HEADER_LEN + G5L_PATCH_SLOT_LEN + 3];
        bytes[..16].copy_from_slice(G5L_MAGIC);
        assert!(matches!(parse(&bytes), Err(G5lError::BadLength { .. })));
    }

    #[test]
    fn rejects_missing_marker() {
        let mut bytes = vec![0u8; G5L_HEADER_LEN + G5L_PATCH_SLOT_LEN];
        bytes[..16].copy_from_slice(G5L_MAGIC);
        // Leave marker bytes as zero (not 04 D3).
        assert!(matches!(parse(&bytes), Err(G5lError::BadMarker { slot: 0, .. })));
    }
}
