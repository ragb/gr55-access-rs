//! Built-in "Init Patch" — a known-playable starting point baked into
//! the crate so editors don't have to ship their own default YAML.
//!
//! The bytes come from FloorBoard's `Init Patch.syx`, which the
//! FloorBoard editor uses as its blank-slate template. The file is a
//! SysEx capture of the patch being written to the GR-55's live edit
//! buffer (MSB `0x18`); decoding it at that MSB yields a `PatchArea`
//! that round-trips to the same bytes.

use crate::patch::PatchArea;
use crate::sysex::parse_frames_unchecked;

/// Raw `Init Patch.syx` bytes — a sequence of DT1 frames writing to
/// the GR-55's live edit-buffer area (`0x18 00 00 00`).
///
/// Source: FloorBoard submodule, `external/floorboard/packager/init_patches/Init Patch.syx`.
const INIT_PATCH_SYX: &[u8] =
    include_bytes!("../../external/floorboard/packager/init_patches/Init Patch.syx");

/// MSB that `INIT_PATCH_SYX` writes to (live edit buffer).
const INIT_PATCH_MSB: u8 = crate::address::TEMP_WRITE_MSB;

/// A typed [`PatchArea`] decoded from FloorBoard's `Init Patch.syx`.
/// Returns a fresh clone on every call — callers are free to mutate.
pub fn new_init_patch() -> PatchArea {
    let frames: Vec<_> = parse_frames_unchecked(INIT_PATCH_SYX)
        .filter_map(|r| r.ok())
        .map(|(f, _)| f.into_owned())
        .collect();
    PatchArea::from_frames_at(&frames, INIT_PATCH_MSB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_patch_decodes_to_an_area() {
        let area = new_init_patch();
        // FloorBoard's Init Patch.syx is a guitar-mode patch named
        // "Init Patch" (with trailing spaces in the wire bytes).
        assert_eq!(area.name.as_string().trim_end(), "Init Patch");
    }

    #[test]
    fn init_patch_round_trips() {
        // Decode the embedded bytes, re-encode at the same MSB, decode
        // again. The two PatchArea values should match — this is the
        // strongest guarantee we can give that the embedded bytes are
        // a faithful representation.
        let first = new_init_patch();
        let frames = first
            .to_frames(0x10, INIT_PATCH_MSB)
            .expect("encode init patch");
        let mut bytes = Vec::new();
        for f in &frames {
            bytes.extend(f.encode());
        }
        let parsed: Vec<_> = parse_frames_unchecked(&bytes)
            .filter_map(|r| r.ok())
            .map(|(f, _)| f.into_owned())
            .collect();
        let second = PatchArea::from_frames_at(&parsed, INIT_PATCH_MSB);
        assert_eq!(first, second);
    }
}
