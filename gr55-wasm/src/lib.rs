//! `gr55-wasm` — `wasm-bindgen` surface over `gr55-core` for browser /
//! Node consumers (e.g. a proprietary web editor).
//!
//! Two kinds of API live here:
//!
//! 1. **Patch I/O** — strings in, strings out for patch payloads (YAML
//!    or raw SysEx bytes). TypeScript types for the patch model itself
//!    come from the JSON Schema (`gr55 schema --of patch`).
//! 2. **Static-data exports** — the `gr55-core` per-block parameter
//!    tables (MFX / MOD / Modeling / PCM tail) and the PCM tone catalog
//!    surfaced as JS arrays so editors don't have to redeclare the data
//!    on the TS side.
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
//! - [`validate_patch_yaml`] — parse YAML as a PatchArea and report
//!   the parser error (if any) as a string array.
//! - [`identity_request`] — bytes of the Universal Non-Realtime Identity
//!   Request to broadcast.
//! - [`matches_identity_reply`] — predicate for a GR-55 identity reply.
//! - [`list_pcm_tones`] — the 910-entry PCM tone catalog (linear index,
//!   wire bank/position, display number, name, category).
//! - [`mfx_params`] — flat MFX parameter table (256 entries).
//! - [`mod_params`] — flat page-0x07 parameter table (128 entries:
//!   PreAmp + NS + MOD together).
//! - [`modeling_params`] — flat Modeling parameter table (256 entries).
//! - [`pcm_tail_params`] — flat PCM tone tail-page parameter table
//!   (40 documented entries).
//! - [`help_for`] — tooltip text for a parameter name.
//! - [`version`] — crate version string.

use std::borrow::Cow;

use wasm_bindgen::prelude::*;

use gr55_core::address::PatchSlot;
use gr55_core::g5l;
use gr55_core::mfx_params::{MfxParamEntry, MFX_PARAMS};
use gr55_core::mod_params::{ParamEntry as ModParamEntry, MOD_PARAMS};
use gr55_core::modeling_params::{ModelingMode, ModelingParamEntry, MODELING_PARAMS};
use gr55_core::patch::PatchArea;
use gr55_core::pcm_tail_params::{PcmTailParamEntry, PCM_TAIL_PARAMS};
use gr55_core::pcm_tones::{PCM_TONE_CATEGORIES, PCM_TONE_COUNT, PCM_TONE_NAMES};
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

/// Parse `yaml` as a `PatchArea` and return any parser error as a
/// single-element string array. Empty array means the YAML decoded
/// cleanly. Use this to surface friendly errors on YAML import without
/// having to round-trip through `encode_patch_yaml_to_sysex`.
#[wasm_bindgen(js_name = validatePatchYaml)]
pub fn validate_patch_yaml(yaml: &str) -> Vec<String> {
    match serde_yaml::from_str::<PatchArea>(yaml) {
        Ok(_) => Vec::new(),
        Err(e) => vec![e.to_string()],
    }
}

// ---------------------------------------------------------------------------
// Typed patch I/O
//
// PatchArea derives tsify-next::Tsify in gr55-core (gated behind the
// `tsify` feature, which gr55-wasm always enables). That means these
// functions take and return a structured JS object — no YAML
// round-trip, no `any` typing, and no need for a YAML parser on the
// editor side. The YAML-based variants above stay for the YAML
// import/export panel, where text is the natural format.
// ---------------------------------------------------------------------------

/// Decode raw SysEx bytes into a typed `PatchArea`. Bytes outside the
/// requested MSB are silently ignored — same semantics as
/// `PatchArea::from_frames_at`.
#[wasm_bindgen(js_name = decodePatchSysex)]
pub fn decode_patch_sysex(bytes: &[u8], base_msb: u8) -> PatchArea {
    let frames: Vec<Frame<'static>> = parse_frames_unchecked(bytes)
        .filter_map(|r| r.ok())
        .map(|(f, _)| f.into_owned())
        .collect();
    PatchArea::from_frames_at(&frames, base_msb)
}

/// Encode a typed `PatchArea` into raw SysEx bytes ready to send via
/// Web MIDI. `device_id` is the Roland device ID byte (commonly `0x10`);
/// `base_msb` is the destination area MSB — `0x18` for live TEMP RAM,
/// `0x20`+slot encoding for USER storage (see `userPatchAddress`).
#[wasm_bindgen(js_name = encodePatchToSysex)]
pub fn encode_patch_to_sysex(
    patch: PatchArea,
    device_id: u8,
    base_msb: u8,
) -> Result<Vec<u8>, JsValue> {
    let frames = patch.to_frames(device_id, base_msb).map_err(js_err)?;
    let mut out = Vec::new();
    for frame in &frames {
        out.extend(frame.encode());
    }
    Ok(out)
}

/// Parse a `.g5l` file and return the requested slot as a typed
/// `PatchArea`. `slot` is 0-based; `base_msb` is the area MSB the
/// returned addresses target (typically `0x18`).
#[wasm_bindgen(js_name = importG5lPatch)]
pub fn import_g5l_patch(bytes: &[u8], slot: usize, base_msb: u8) -> Result<PatchArea, JsValue> {
    let patches = g5l::parse(bytes).map_err(js_err)?;
    let p = patches.get(slot).ok_or_else(|| {
        js_err(format!(
            "slot {slot} out of range ({} slots)",
            patches.len()
        ))
    })?;
    Ok(p.to_patch_area(base_msb))
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Bytes of the Universal Non-Realtime Identity Request, addressed to
/// the broadcast device ID (`0x7F`). Send these via Web MIDI on patch
/// scan to elicit a GR-55 identity reply.
#[wasm_bindgen(js_name = identityRequest)]
pub fn identity_request() -> Vec<u8> {
    // F0 7E 7F 06 01 F7 — sub-id1=06 (general information), sub-id2=01
    // (identity request). Device ID 7F = broadcast.
    vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]
}

/// True if `bytes` is a Universal Non-Realtime Identity Reply from a
/// GR-55. Matches on Roland manufacturer + family code `53 02` + family
/// number `00 00`; software-revision bytes are not checked.
///
/// Reply layout (15 bytes):
/// `F0 7E [dev] 06 02 41 53 02 00 00 [sw_rev_4] F7`
#[wasm_bindgen(js_name = matchesIdentityReply)]
pub fn matches_identity_reply(bytes: &[u8]) -> bool {
    if bytes.len() < 15 {
        return false;
    }
    if bytes[0] != 0xF0 || bytes[bytes.len() - 1] != 0xF7 {
        return false;
    }
    if bytes[1] != 0x7E {
        return false;
    }
    // sub-id1=06 (general info), sub-id2=02 (identity reply)
    if bytes[3] != 0x06 || bytes[4] != 0x02 {
        return false;
    }
    // 0x41 = Roland manufacturer ID.
    if bytes[5] != 0x41 {
        return false;
    }
    // Family code (LSB first): 53 02. Family number: 00 00.
    bytes[6] == 0x53 && bytes[7] == 0x02 && bytes[8] == 0x00 && bytes[9] == 0x00
}

// ---------------------------------------------------------------------------
// Frame builders + address helpers
//
// Editors need to send single-byte DT1 writes (for per-knob edits) and
// targeted RQ1 reads (to pull the live edit buffer, a specific USER /
// PRESET slot, or a System sub-page). Building those by hand in TS
// means duplicating the Roland checksum + framing rules; expose them
// here instead.
// ---------------------------------------------------------------------------

fn require_address_4(address: &[u8]) -> Result<[u8; 4], JsValue> {
    if address.len() != 4 {
        return Err(js_err(format!(
            "address must be exactly 4 bytes, got {}",
            address.len()
        )));
    }
    Ok([address[0], address[1], address[2], address[3]])
}

/// Build a Roland **DT1** (data set) frame: write `data` at `address`
/// on `device_id`. Returns the full SysEx bytes including SOX,
/// manufacturer, model ID, command, address, checksum, and EOX.
#[wasm_bindgen(js_name = buildDt1)]
pub fn build_dt1(device_id: u8, address: &[u8], data: &[u8]) -> Result<Vec<u8>, JsValue> {
    let addr = require_address_4(address)?;
    let frame = Frame::Dt1 {
        device_id,
        address: addr,
        data: Cow::Borrowed(data),
    };
    Ok(frame.encode())
}

/// Build a Roland **RQ1** (data request) frame: ask the device to
/// dump `size` bytes starting at `address`. The reply arrives as one
/// or more DT1 frames.
#[wasm_bindgen(js_name = buildRq1)]
pub fn build_rq1(device_id: u8, address: &[u8], size: u32) -> Result<Vec<u8>, JsValue> {
    let addr = require_address_4(address)?;
    let frame = Frame::Rq1 {
        device_id,
        address: addr,
        size,
    };
    Ok(frame.encode())
}

/// 4-byte address of the **edit-buffer write** area (TEMP RAM the
/// device is currently rendering). Send patch DT1 frames here to push
/// edits live.
#[wasm_bindgen(js_name = editBufferWriteAddress)]
pub fn edit_buffer_write_address() -> Vec<u8> {
    vec![gr55_core::address::TEMP_WRITE_MSB, 0x00, 0x00, 0x00]
}

/// 4-byte address of the **edit-buffer read** area (Temporary Buffer
/// Bulk). Use as the address of an RQ1 to pull the current patch.
#[wasm_bindgen(js_name = editBufferReadAddress)]
pub fn edit_buffer_read_address() -> Vec<u8> {
    vec![gr55_core::address::TEMP_BUFFER_MSB, 0x00, 0x00, 0x00]
}

/// 4-byte address of the storage location for `User <bank>:<position>`
/// (bank `1..=99`, position `1..=3`). Throws on out-of-range inputs.
#[wasm_bindgen(js_name = userPatchAddress)]
pub fn user_patch_address(bank: u8, position: u8) -> Result<Vec<u8>, JsValue> {
    let slot = PatchSlot::user(bank, position).map_err(js_err)?;
    Ok(slot.address().to_vec())
}

/// 4-byte address of the storage location for `Preset <bank>:<position>`.
#[wasm_bindgen(js_name = presetPatchAddress)]
pub fn preset_patch_address(bank: u8, position: u8) -> Result<Vec<u8>, JsValue> {
    let slot = PatchSlot::preset(bank, position).map_err(js_err)?;
    Ok(slot.address().to_vec())
}

// ---------------------------------------------------------------------------
// PCM tone catalog
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct PcmToneView {
    /// Linear 0..=909.
    linear: u16,
    /// Wire bank byte (0..=7), high half of the two-byte tone selector.
    bank: u8,
    /// Wire position byte (0..=127) within the bank.
    position: u8,
    /// 1-based display number (1..=910) matching FloorBoard's labelling.
    display_number: u16,
    /// Tone name.
    name: &'static str,
    /// Tone category (e.g. "Acoustic Piano", "Synth Lead", "Drums").
    /// 46 distinct values across the 910 tones; useful as a UI
    /// "browse by category" key.
    category: &'static str,
}

/// Full GR-55 PCM tone catalog as a JS array of `{ linear, bank,
/// position, display_number, name, category }`. 910 entries.
#[wasm_bindgen(js_name = listPcmTones)]
pub fn list_pcm_tones() -> Result<JsValue, JsValue> {
    let mut out = Vec::with_capacity(PCM_TONE_COUNT);
    for i in 0..PCM_TONE_COUNT {
        let linear = i as u16;
        out.push(PcmToneView {
            linear,
            bank: (linear / 128) as u8,
            position: (linear % 128) as u8,
            display_number: linear + 1,
            name: PCM_TONE_NAMES[i],
            category: PCM_TONE_CATEGORIES[i],
        });
    }
    serde_wasm_bindgen::to_value(&out).map_err(js_err)
}

// ---------------------------------------------------------------------------
// Parameter tables (MFX / MOD / Modeling / PCM tail)
//
// Each table is exposed as a flat JS array. Filtering by owning type,
// mode, category, group, etc. is left to the editor — keeps the wasm
// surface small and makes the data trivially cacheable in JS.
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct EnumValue {
    /// Wire byte value.
    byte: u8,
    /// Human-readable label for that byte.
    label: &'static str,
}

/// Shared shape for MFX / page-0x07 / Modeling / PCM-tail parameter
/// entries. Fields not relevant to a given table (e.g. `mode` for MFX)
/// are omitted via `Option`.
#[derive(serde::Serialize)]
struct ParamView {
    /// Wire page byte.
    page: u8,
    /// Wire offset within the page.
    offset: u8,
    /// Effect type that owns this byte, snake_case (e.g. "equalizer",
    /// "super_filter"). `None` for common header bytes and (in the
    /// Modeling table) bytes that belong to a category rather than a
    /// type. Only populated for MFX and MOD tables.
    #[serde(skip_serializing_if = "Option::is_none")]
    owning_type: Option<&'static str>,
    /// Modeling-only: which mode this byte applies to ("guitar",
    /// "bass", or "both").
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<&'static str>,
    /// Modeling-only: top-level category ("E.GTR", "Acoustic", "Bass",
    /// "Synth", "Modeling", "NS", or "").
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<&'static str>,
    /// Modeling-only: types subset (`desc` field — dash-separated IDs,
    /// sub-type names, or descriptive phrases). Empty when the byte
    /// applies to all types in its category.
    #[serde(skip_serializing_if = "Option::is_none")]
    types: Option<&'static str>,
    /// PCM-tail-only: editor UI bucket ("filter", "tvf", "tva",
    /// "pitch_env", "lfo", "portamento", "velocity", "reserved").
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'static str>,
    /// Human-readable parameter name.
    name: &'static str,
    /// Named byte values (empty for purely-numeric params).
    values: Vec<EnumValue>,
    /// Raw wire-byte (min, max). `None` for purely enumerated params.
    #[serde(skip_serializing_if = "Option::is_none")]
    range: Option<(u8, u8)>,
    /// Display (min, max) for params whose display values differ from
    /// their wire bytes (e.g. -15..=+15 dB across wire 0..=30).
    #[serde(skip_serializing_if = "Option::is_none")]
    display_range: Option<(i32, i32)>,
    /// Tooltip-friendly description (may be empty).
    help: &'static str,
}

fn enum_values(values: &'static [(u8, &'static str)]) -> Vec<EnumValue> {
    values
        .iter()
        .map(|&(byte, label)| EnumValue { byte, label })
        .collect()
}

fn mfx_view(e: &MfxParamEntry) -> ParamView {
    ParamView {
        page: e.page,
        offset: e.offset,
        owning_type: e.owning_type.map(|o| o.as_snake()),
        mode: None,
        category: None,
        types: None,
        group: None,
        name: e.name,
        values: enum_values(e.values),
        range: e.range,
        display_range: e.display_range,
        help: e.help,
    }
}

fn mod_view(e: &ModParamEntry) -> ParamView {
    ParamView {
        page: e.page,
        offset: e.offset,
        owning_type: e.owning_type.map(|o| o.as_snake()),
        mode: None,
        category: None,
        types: None,
        group: None,
        name: e.name,
        values: enum_values(e.values),
        range: e.range,
        display_range: e.display_range,
        help: e.help,
    }
}

fn modeling_mode_snake(m: ModelingMode) -> &'static str {
    match m {
        ModelingMode::Guitar => "guitar",
        ModelingMode::Bass => "bass",
        ModelingMode::Both => "both",
    }
}

fn modeling_view(e: &ModelingParamEntry) -> ParamView {
    ParamView {
        page: e.page,
        offset: e.offset,
        owning_type: None,
        mode: Some(modeling_mode_snake(e.mode)),
        category: Some(e.category),
        types: Some(e.types),
        group: None,
        name: e.name,
        values: enum_values(e.values),
        range: e.range,
        display_range: e.display_range,
        help: e.help,
    }
}

fn pcm_tail_view(e: &PcmTailParamEntry) -> ParamView {
    ParamView {
        // PCM tail bytes are reachable on page 0x30 (Tone 1) or 0x31
        // (Tone 2); the table is page-agnostic so we report the offset
        // and a synthetic page=0 placeholder.
        page: 0,
        offset: e.offset,
        owning_type: None,
        mode: None,
        category: None,
        types: None,
        group: Some(e.group.as_snake()),
        name: e.name,
        values: enum_values(e.values),
        range: e.range,
        display_range: e.display_range,
        help: e.help,
    }
}

/// Flat MFX parameter table — 256 entries (page 0x03 + page 0x04).
/// Each entry carries its wire address, `owning_type` (snake-case of
/// the effect type, or `null` for the 6 common header bytes), and full
/// labelling / range / help metadata. Filter on `owning_type` in the
/// editor to render a specific MFX type's tail.
#[wasm_bindgen(js_name = mfxParams)]
pub fn mfx_params() -> Result<JsValue, JsValue> {
    let out: Vec<ParamView> = MFX_PARAMS.iter().map(mfx_view).collect();
    serde_wasm_bindgen::to_value(&out).map_err(js_err)
}

/// Flat page-0x07 parameter table — 128 entries covering PreAmp + NS
/// + MOD common-header + MOD type-specific tails. The "MOD" name
/// matches the upstream `MOD_PARAMS` symbol; the table actually spans
/// the whole page. Filter on `owning_type` for MOD's 14 effect types,
/// or on `offset` ranges for PreAmp (`0x00..=0x10`) / NS (`0x5A..=0x5C`)
/// / MOD-common (`0x11..=0x17`) / MOD-tail (`0x18..=0x59`).
#[wasm_bindgen(js_name = modParams)]
pub fn mod_params() -> Result<JsValue, JsValue> {
    let out: Vec<ParamView> = MOD_PARAMS.iter().map(mod_view).collect();
    serde_wasm_bindgen::to_value(&out).map_err(js_err)
}

/// Flat Modeling parameter table — 256 entries (page 0x10 = Guitar
/// mode + common header, page 0x11 = Bass mode). Each entry carries
/// `mode` ("guitar" / "bass" / "both"), `category` (e.g. "E.GTR",
/// "Synth"), and `types` (FloorBoard's `desc` — dash-separated IDs
/// or sub-type names). Filter on these in the editor.
#[wasm_bindgen(js_name = modelingParams)]
pub fn modeling_params() -> Result<JsValue, JsValue> {
    let out: Vec<ParamView> = MODELING_PARAMS.iter().map(modeling_view).collect();
    serde_wasm_bindgen::to_value(&out).map_err(js_err)
}

/// Flat PCM tone tail-page parameter table — 40 documented entries
/// covering Filter / TVF / TVA / PitchEnv / LFO / Velocity /
/// Portamento. Both Tone 1 (page 0x30) and Tone 2 (page 0x31) share
/// this layout; the `page` field is a placeholder (`0`). Use the
/// `group` field for editor UI bucketing.
#[wasm_bindgen(js_name = pcmTailParams)]
pub fn pcm_tail_params() -> Result<JsValue, JsValue> {
    let out: Vec<ParamView> = PCM_TAIL_PARAMS.iter().map(pcm_tail_view).collect();
    serde_wasm_bindgen::to_value(&out).map_err(js_err)
}

fn js_err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}
