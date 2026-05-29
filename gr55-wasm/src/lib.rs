//! `gr55-wasm` — `wasm-bindgen` surface over `gr55-core` for browser /
//! Node consumers (e.g. a proprietary web editor).
//!
//! API stays deliberately small in v1: strings in, strings out for
//! patch payloads (YAML or raw SysEx bytes). TypeScript types come
//! from the JSON Schema (`gr55 schema --of patch`) which the editor
//! pulls separately and feeds to a form/validator library.
//!
//! Functions exposed:
//!
//! - [`import_g5l_to_yaml`] — parse a FloorBoard `.g5l` file, return
//!   the requested slot as PatchArea YAML.
//! - [`list_g5l_slots`] — patch metadata (slot index + name) for the
//!   slots in a `.g5l` file, without committing to a full decode.
//! - [`decode_patch_sysex_to_yaml`] — parse raw SysEx bytes and return
//!   PatchArea YAML at the given base MSB.
//! - [`encode_patch_yaml_to_sysex`] — parse YAML, encode as raw SysEx
//!   bytes (one DT1 frame per byte, ready to ship via Web MIDI).
//! - [`help_for`] — tooltip text for a parameter name.
//! - [`version`] — crate version string.

use wasm_bindgen::prelude::*;

use gr55_core::g5l;
use gr55_core::patch::PatchArea;
use gr55_core::sysex::{parse_frames_unchecked, Frame};

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Tooltip help text for a parameter by canonical name. Returns
/// `undefined` to JS when no entry is registered.
#[wasm_bindgen(js_name = helpFor)]
pub fn help_for(name: &str) -> Option<String> {
    gr55_core::help::help_for(name).map(|s| s.to_string())
}

/// Parse a `.g5l` file and return the requested slot as PatchArea
/// YAML. `slot` is 0-based. `base_msb` is the area MSB the YAML's
/// addresses target — typically `0x18` for "as if this patch lived in
/// TEMP RAM".
#[wasm_bindgen(js_name = importG5lToYaml)]
pub fn import_g5l_to_yaml(bytes: &[u8], slot: usize, base_msb: u8) -> Result<String, JsValue> {
    let patches = g5l::parse(bytes).map_err(js_err)?;
    let p = patches.get(slot).ok_or_else(|| {
        js_err(format!(
            "slot {slot} out of range ({} slots)",
            patches.len()
        ))
    })?;
    let area = p.to_patch_area(base_msb);
    serde_yaml::to_string(&area).map_err(js_err)
}

#[derive(serde::Serialize)]
struct G5lSlot {
    slot_index: usize,
    name: String,
}

/// List patches inside a `.g5l` file (slot index + name). Useful for
/// rendering a "pick a slot" UI before committing to a full decode.
/// Returns a JS array of `{ slot_index, name }`.
#[wasm_bindgen(js_name = listG5lSlots)]
pub fn list_g5l_slots(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let patches = g5l::parse(bytes).map_err(js_err)?;
    let slots: Vec<G5lSlot> = patches
        .iter()
        .map(|p| G5lSlot {
            slot_index: p.slot_index,
            name: p.name_str(),
        })
        .collect();
    serde_wasm_bindgen::to_value(&slots).map_err(js_err)
}

/// Decode raw SysEx bytes into PatchArea YAML at the given base MSB.
/// Bytes outside the requested MSB are silently ignored — same
/// semantics as `PatchArea::from_frames_at`.
#[wasm_bindgen(js_name = decodePatchSysexToYaml)]
pub fn decode_patch_sysex_to_yaml(bytes: &[u8], base_msb: u8) -> Result<String, JsValue> {
    let frames: Vec<Frame<'static>> = parse_frames_unchecked(bytes)
        .filter_map(|r| r.ok())
        .map(|(f, _)| f.into_owned())
        .collect();
    let area = PatchArea::from_frames_at(&frames, base_msb);
    serde_yaml::to_string(&area).map_err(js_err)
}

/// Encode PatchArea YAML into raw SysEx bytes ready to send via Web
/// MIDI. `device_id` is the Roland device ID byte (commonly `0x10`).
/// `base_msb` is the destination area MSB — `0x18` for live TEMP RAM,
/// `0x20` + slot encoding for USER storage (see `gr55-core` docs).
#[wasm_bindgen(js_name = encodePatchYamlToSysex)]
pub fn encode_patch_yaml_to_sysex(
    yaml: &str,
    device_id: u8,
    base_msb: u8,
) -> Result<Vec<u8>, JsValue> {
    let area: PatchArea = serde_yaml::from_str(yaml).map_err(js_err)?;
    let frames = area.to_frames(device_id, base_msb).map_err(js_err)?;
    let mut out = Vec::new();
    for frame in &frames {
        out.extend(frame.encode());
    }
    Ok(out)
}

fn js_err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}
