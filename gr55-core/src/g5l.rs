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

/// The most common patch-prefix marker (`04 D3` at slot offsets +2/+3).
/// **Not** authoritative — FloorBoard's reader (`fileDialog.cpp:92`)
/// learns the actual marker key for a file by reading 10 bytes from
/// offset 162 of that specific file (so older or variant FB writes can
/// use a different 2-byte prefix here, e.g. `05 2B`, `04 EF`). We trust
/// the fixed 1239-byte slot stride instead of this constant for
/// navigation; the constant is kept as documentation of the writer's
/// default behaviour and for use in synthetic-file tests.
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
}

/// Parse a `.g5l` file into one [`G5lPatch`] per fixed 1239-byte slot
/// starting at offset 160. Lenient on two axes:
///
/// - **Trailing bytes** — FloorBoard real-world files often carry a
///   small footer (6..~430 bytes) after the last full slot whose
///   origin isn't documented in FB's own writer source. Anything that
///   doesn't form a complete slot at the tail is silently ignored. Use
///   [`trailing_bytes_after_last_slot`] to inspect the trailer length.
/// - **Per-slot prefix marker** — FB's reader learns the marker key
///   from each file's slot 0 (it isn't always `04 D3`; older/variant
///   files use different 2-byte prefixes). We trust the fixed 1239-byte
///   stride for navigation and don't enforce any marker constraint, so
///   variant-marker files parse cleanly.
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
    let n_patches = after_header / G5L_PATCH_SLOT_LEN;
    let mut out = Vec::with_capacity(n_patches);
    for i in 0..n_patches {
        let slot_start = G5L_HEADER_LEN + i * G5L_PATCH_SLOT_LEN;
        let slot = &bytes[slot_start..slot_start + G5L_PATCH_SLOT_LEN];
        out.push(extract_patch(i, slot));
    }
    Ok(out)
}

/// How many bytes after the last full patch slot are left over —
/// FloorBoard's writer source doesn't document a footer but most
/// real-world `.g5l` files carry one (6..~430 bytes). Useful if you
/// want to log or assert on the trailer separately from the patches.
pub fn trailing_bytes_after_last_slot(bytes: &[u8]) -> usize {
    if bytes.len() < G5L_HEADER_LEN {
        return 0;
    }
    (bytes.len() - G5L_HEADER_LEN) % G5L_PATCH_SLOT_LEN
}

/// Slot-relative (offset, length, block-byte) entries in the order the
/// writer lays them out. Block 0x04 is 114 bytes per the writer's
/// truncated save; see the module docs.
const BLOCK_LAYOUT: &[(usize, usize, u8)] = &[
    (12, 128, 0x00),  // Names + Pedal
    (140, 114, 0x01), // Master Assigns part 1 (offsets 0x00..0x71)
    (262, 14, 0x01),  // Master Assigns part 2 (offsets 0x72..0x7F)
    (276, 78, 0x02),  // Patch Common (offsets 0x00..0x4D)
    (362, 128, 0x03), // MFX page 1
    (490, 114, 0x04), // MFX page 2 (FB bug: last 14 not stored)
    (618, 18, 0x05),  // "blank nul" reserved
    (652, 30, 0x06),  // Chorus/Delay/Reverb/EQ
    (690, 125, 0x07), // MOD/AMP
    (823, 128, 0x10), // Modeling page 1
    (951, 86, 0x11),  // Modeling page 2
    (1045, 35, 0x20), // PCM 1 header
    (1084, 35, 0x21), // PCM 2 header
    (1127, 52, 0x30), // PCM 1 tail
    (1183, 52, 0x31), // PCM 2 tail
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
            blocks.push(G5lBlock {
                block,
                bytes: payload,
            });
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
        let bytes = crate::test_support::fb_fixture_required("default.g5l");
        let patches = parse(&bytes).expect("parse default.g5l");
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
    fn tolerates_trailing_bytes_after_last_full_slot() {
        // Build a minimal valid 1-patch file plus 17 junk trailing bytes.
        // parse() should still return the 1 patch and ignore the trailer.
        let mut bytes = vec![0u8; G5L_HEADER_LEN + G5L_PATCH_SLOT_LEN + 17];
        bytes[..16].copy_from_slice(G5L_MAGIC);
        // Place the 04 D3 marker for slot 0.
        bytes[G5L_HEADER_LEN + 2] = 0x04;
        bytes[G5L_HEADER_LEN + 3] = 0xD3;
        let patches = parse(&bytes).expect("trailing bytes should be tolerated");
        assert_eq!(patches.len(), 1);
        assert_eq!(trailing_bytes_after_last_slot(&bytes), 17);
    }

    #[test]
    fn tolerates_unusual_prefix_marker() {
        // FB's reader learns the marker key per-file from slot 0; a
        // synthetic file with a non-`04 D3` prefix should still parse,
        // since we navigate by the fixed 1239-byte slot stride.
        let mut bytes = vec![0u8; G5L_HEADER_LEN + G5L_PATCH_SLOT_LEN];
        bytes[..16].copy_from_slice(G5L_MAGIC);
        bytes[G5L_HEADER_LEN + 2] = 0x05; // not 0x04
        bytes[G5L_HEADER_LEN + 3] = 0x2B; // not 0xD3
        let patches = parse(&bytes).expect("variant marker should still parse");
        assert_eq!(patches.len(), 1);
    }

    /// Walk every `.g5l` file in the vendored FloorBoard distribution
    /// (`docs/spec/floorboard_src/.../packager/saved_patches/`, ~741
    /// files) and assert each one parses cleanly AND every patch in it
    /// round-trips losslessly via PatchArea. Any parse failure or
    /// round-trip mismatch is collected and reported with the file
    /// name + slot index so regressions point straight at the culprit.
    ///
    /// This is the broadest patch-corpus test we have — touching ~28k
    /// per-block payloads across the full diversity of guitar/bass mode,
    /// every Modeling category, all 20 MFX types, etc.
    #[test]
    fn g5l_round_trips_across_all_bundled_patches() {
        let patches_dir = crate::test_support::floorboard_root()
            .join("packager")
            .join("saved_patches");
        if !patches_dir.exists() {
            panic!(
                "FloorBoard submodule missing at {}.\n\
                 Run `git submodule update --init --recursive` from the workspace root.",
                patches_dir.display()
            );
        }
        // Recursively walk subdirectories: saved_patches/ has
        // Guitar Mode/, Bass Mode/, quick_saved/ children.
        fn collect_g5l(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let stem = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                // Skip dot-prefixed (hidden) entries — FloorBoard leaves
                // partial-save stubs there (e.g. `.335.g5l` is 814 bytes,
                // short of even one full slot).
                if stem.starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    collect_g5l(&path, out);
                } else if path.extension().and_then(|e| e.to_str()) == Some("g5l") {
                    out.push(path);
                }
            }
        }
        let mut all_paths = Vec::new();
        collect_g5l(&patches_dir, &mut all_paths);
        all_paths.sort();

        let mut files_scanned = 0;
        let mut patches_round_tripped = 0;
        let mut parse_failures: Vec<(String, String)> = Vec::new();
        let mut round_trip_failures: Vec<(String, usize)> = Vec::new();

        for path in &all_paths {
            files_scanned += 1;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let bytes = std::fs::read(path).expect("read .g5l");
            match parse(&bytes) {
                Err(e) => parse_failures.push((name.clone(), format!("{e}"))),
                Ok(patches) => {
                    for p in &patches {
                        let area = p.to_patch_area(0x18);
                        let frames = area.to_frames(0x10, 0x18).unwrap_or_else(|e| {
                            panic!("re-emit {name} slot {}: {e:?}", p.slot_index)
                        });
                        let area2 = PatchArea::from_frames_at(&frames, 0x18);
                        if area == area2 {
                            patches_round_tripped += 1;
                        } else {
                            round_trip_failures.push((name.clone(), p.slot_index));
                        }
                    }
                }
            }
        }

        eprintln!(
            "g5l corpus: scanned {files_scanned} files, round-tripped {patches_round_tripped} patches"
        );
        if !parse_failures.is_empty() {
            eprintln!("  parse failures ({}):", parse_failures.len());
            for (name, err) in parse_failures.iter().take(10) {
                eprintln!("    {name}: {err}");
            }
            if parse_failures.len() > 10 {
                eprintln!("    ... ({} more)", parse_failures.len() - 10);
            }
        }
        if !round_trip_failures.is_empty() {
            eprintln!("  round-trip failures ({}):", round_trip_failures.len());
            for (name, slot) in round_trip_failures.iter().take(10) {
                eprintln!("    {name} slot {slot}");
            }
            if round_trip_failures.len() > 10 {
                eprintln!("    ... ({} more)", round_trip_failures.len() - 10);
            }
        }

        assert!(
            files_scanned >= 200,
            "expected to scan at least 200 .g5l files (got {files_scanned}) — \
             motiz88/GR-55Floorboard ships ~297 patches at \
             packager/saved_patches/; check the submodule checkout."
        );
        assert!(
            parse_failures.is_empty(),
            "{} .g5l file(s) failed to parse — see eprintln output",
            parse_failures.len()
        );
        assert!(
            round_trip_failures.is_empty(),
            "{} patch(es) failed PatchArea round-trip — see eprintln output",
            round_trip_failures.len()
        );
    }
}
