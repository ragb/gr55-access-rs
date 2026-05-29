//! Generates `OUT_DIR/midi_map.rs` from `data/midi.xml` (FloorBoard's GR-55
//! address map) at build time. The generated module exposes static slices of
//! `ParamMeta` per top-level XML section so the runtime never parses XML.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use roxmltree::{Document, Node};

const SECTIONS: &[(&str, &str)] = &[
    ("System", "SYSTEM"),
    ("Structure", "STRUCTURE"),
    ("Tables", "TABLES"),
    ("MPT", "MPT"),
];

fn main() {
    println!("cargo:rerun-if-changed=data/midi.xml");
    println!("cargo:rerun-if-changed=data/midi.xsd");
    println!("cargo:rerun-if-changed=build.rs");

    let raw = fs::read_to_string("data/midi.xml").expect("read data/midi.xml");
    let xml = strip_xml_declaration(&raw);

    let doc = Document::parse(xml).expect("parse midi.xml");
    let root = doc.root_element();
    assert_eq!(
        root.tag_name().name(),
        "SysX",
        "midi.xml root should be <SysX>"
    );

    let mut out = String::new();
    write_preamble(&mut out);

    for (xml_name, rust_prefix) in SECTIONS {
        let section = root
            .children()
            .find(|c| c.has_tag_name(*xml_name))
            .unwrap_or_else(|| panic!("midi.xml missing <{xml_name}> section"));
        let params = collect_params(section);
        emit_section(&mut out, rust_prefix, &params);
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out_dir.join("midi_map.rs"), out).expect("write midi_map.rs");

    // Also emit the PCM tone catalog (910 named tones + categories).
    let pcm = emit_pcm_tone_catalog(&doc);
    fs::write(out_dir.join("pcm_tones.rs"), pcm).expect("write pcm_tones.rs");

    // And the MFX parameter table — empirically validates the
    // disjoint-type-ranges hypothesis: each (page, offset) inside the
    // 256-byte MFX block is owned by at most one MFX effect type.
    let mfx = emit_mfx_param_table(&doc);
    fs::write(out_dir.join("mfx_params.rs"), mfx).expect("write mfx_params.rs");

    // Same for the MOD effect (page 0x07 0x18..=0x5C, 14 types).
    let mod_ = emit_mod_param_table(&doc);
    fs::write(out_dir.join("mod_params.rs"), mod_).expect("write mod_params.rs");

    // And Modeling — 2-axis taxonomy (mode encoded by page, category by
    // `abbr`, type/type-set by `desc`). Multiple types CAN share a single
    // byte (e.g. desc="01-02" means types 1 and 2 of that category share
    // this parameter — physical-instrument families with the same control
    // surface), so the disjoint check here is weaker than for MFX/MOD:
    // we only assert no two DATA elements claim the same (page, offset).
    let modeling = emit_modeling_param_table(&doc);
    fs::write(out_dir.join("modeling_params.rs"), modeling)
        .expect("write modeling_params.rs");
}

fn strip_xml_declaration(input: &str) -> &str {
    let bytes = input.as_bytes();
    let start = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    let rest = &input[start..];
    if let Some(end) = rest.find("?>") {
        let after = &rest[end + 2..];
        after.trim_start()
    } else {
        rest
    }
}

/// One addressable byte location in the GR-55's map.
#[derive(Debug, Clone)]
struct LeafParam {
    /// XML path from the section root down to this leaf, in hex (e.g. `[0x01, 0x00, 0x00]`).
    /// Length depends on section: System uses 3 levels under `<System>`, Structure uses
    /// 3 levels, etc. Stored variable-width; callers fold this into the 4-byte wire
    /// address according to the section's base.
    path: Vec<u8>,
    /// Human-readable name, derived from `abbr`/`name`/`desc` of the leaf node.
    name: String,
    /// Per-byte-value enumeration, sorted by value. Empty if the parameter has no
    /// declared enum (e.g. a raw numeric range — we don't extract ranges in v1).
    enum_values: Vec<(u8, String)>,
}

fn collect_params(section: Node) -> Vec<LeafParam> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    let mut names = Vec::new();
    walk(section, &mut path, &mut names, &mut out);
    out
}

/// Depth-first walk. At each node, decide whether to:
/// - extend the path (LSB / PARAM / DATA wrappers with `value` attr) and recurse,
/// - treat the node as a leaf parameter and collect its enum-value children.
///
/// Heuristic: a node is a leaf parameter when it has `<PARAM value="…" name="…"/>`
/// children that themselves have no children. Otherwise it's a structural wrapper.
///
/// `names` is a stack of non-empty parameter labels collected from ancestors.
/// Multi-byte parameters in FloorBoard's XML are encoded as
/// `<PARAM name="…"><DATA value="…">…enum values…</DATA></PARAM>`, so the
/// leaf `<DATA>` has no name — we inherit the parent's.
fn walk(node: Node, path: &mut Vec<u8>, names: &mut Vec<String>, out: &mut Vec<LeafParam>) {
    let value_byte = hex_value_attr(node);
    let node_name = leaf_param_name(node);
    let pushed_name = !node_name.is_empty();

    if let Some(v) = value_byte {
        path.push(v);
    }
    if pushed_name {
        names.push(node_name.clone());
    }

    let children: Vec<Node> = node.children().filter(|n| n.is_element()).collect();

    if !children.is_empty() && children.iter().all(|c| is_leaf_enum_value(*c)) {
        let enum_values: Vec<(u8, String)> = children
            .iter()
            .filter_map(|c| {
                let v = hex_value_attr(*c)?;
                let name = leaf_value_name(*c);
                Some((v, name))
            })
            .collect();
        let name = names.last().cloned().unwrap_or_default();
        out.push(LeafParam {
            path: path.clone(),
            name,
            enum_values,
        });
    } else {
        for child in children {
            walk(child, path, names, out);
        }
    }

    if pushed_name {
        names.pop();
    }
    if value_byte.is_some() {
        path.pop();
    }
}

fn is_leaf_enum_value(n: Node) -> bool {
    n.tag_name().name() == "PARAM" && !n.children().any(|c| c.is_element())
}

fn hex_value_attr(n: Node) -> Option<u8> {
    let v = n.attribute("value")?;
    u8::from_str_radix(v.trim(), 16).ok()
}

fn leaf_value_name(n: Node) -> String {
    let raw = n.attribute("name").unwrap_or("").trim();
    if raw.is_empty() {
        n.attribute("desc").unwrap_or("").trim().to_string()
    } else {
        raw.to_string()
    }
}

fn leaf_param_name(n: Node) -> String {
    // FloorBoard's XML stores the descriptive parameter label in different
    // attributes depending on tag:
    //   - `<PARAM name="Output Select" abbr="" customdesc="Output Select">`
    //     uses `name` for the label; `abbr` (when present) is a short tag
    //     orthogonal to the name, e.g. `<PARAM name="GK Set" abbr="Both Modes">`.
    //   - `<DATA value="00" name="" abbr="Guitar/Bass Mode" customdesc="Mode">`
    //     leaves `name` empty and puts the descriptive label in `abbr`.
    // Use tag-aware priority so both forms surface their human-readable label.
    let priority: &[&str] = match n.tag_name().name() {
        "DATA" => &["abbr", "name", "customdesc", "desc"],
        _ => &["name", "customdesc", "abbr", "desc"],
    };
    for attr in priority {
        if let Some(v) = n.attribute(*attr) {
            let t = v.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    String::new()
}

fn write_preamble(out: &mut String) {
    writeln!(
        out,
        "// AUTO-GENERATED by build.rs from data/midi.xml — do not edit."
    )
    .unwrap();
    writeln!(
        out,
        "// Mined from GR-55 FloorBoard's midi.xml (Colin Willcocks, GPL-2-or-later)."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// One legal byte value for a `ParamMeta` and its symbolic name."
    )
    .unwrap();
    writeln!(out, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]").unwrap();
    writeln!(out, "pub struct EnumValue {{").unwrap();
    writeln!(out, "    pub byte: u8,").unwrap();
    writeln!(out, "    pub name: &'static str,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "/// One addressable byte in the GR-55 map.").unwrap();
    writeln!(out, "#[derive(Debug, Clone, Copy)]").unwrap();
    writeln!(out, "pub struct ParamMeta {{").unwrap();
    writeln!(
        out,
        "    /// XML path from the section root to this leaf (most-significant first)."
    )
    .unwrap();
    writeln!(
        out,
        "    /// Section base address must be prepended to form the on-wire 4-byte address."
    )
    .unwrap();
    writeln!(out, "    pub path: &'static [u8],").unwrap();
    writeln!(out, "    pub name: &'static str,").unwrap();
    writeln!(
        out,
        "    /// Legal byte values for this parameter. Empty for numeric / range parameters."
    )
    .unwrap();
    writeln!(out, "    pub values: &'static [EnumValue],").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
}

fn emit_section(out: &mut String, prefix: &str, params: &[LeafParam]) {
    let mut path_pool: BTreeMap<Vec<u8>, String> = BTreeMap::new();
    let mut values_pool: BTreeMap<Vec<(u8, String)>, String> = BTreeMap::new();

    for p in params {
        if !path_pool.contains_key(&p.path) {
            let id = format!("{prefix}_PATH_{}", path_pool.len());
            path_pool.insert(p.path.clone(), id);
        }
        if !values_pool.contains_key(&p.enum_values) {
            let id = format!("{prefix}_VALUES_{}", values_pool.len());
            values_pool.insert(p.enum_values.clone(), id);
        }
    }

    writeln!(
        out,
        "// === {prefix} (mined leaf-parameter count: {}) ===",
        params.len()
    )
    .unwrap();
    writeln!(out).unwrap();

    for (path, ident) in &path_pool {
        write!(out, "const {ident}: &[u8] = &[").unwrap();
        for (i, b) in path.iter().enumerate() {
            if i > 0 {
                write!(out, ", ").unwrap();
            }
            write!(out, "0x{b:02X}").unwrap();
        }
        writeln!(out, "];").unwrap();
    }
    writeln!(out).unwrap();

    for (vals, ident) in &values_pool {
        writeln!(out, "const {ident}: &[EnumValue] = &[").unwrap();
        for (b, n) in vals {
            writeln!(
                out,
                "    EnumValue {{ byte: 0x{b:02X}, name: {} }},",
                rust_str(n)
            )
            .unwrap();
        }
        writeln!(out, "];").unwrap();
    }
    writeln!(out).unwrap();

    writeln!(out, "pub static {prefix}_PARAMETERS: &[ParamMeta] = &[").unwrap();
    for p in params {
        let path_ident = path_pool.get(&p.path).unwrap();
        let vals_ident = values_pool.get(&p.enum_values).unwrap();
        writeln!(
            out,
            "    ParamMeta {{ path: {path_ident}, name: {}, values: {vals_ident} }},",
            rust_str(&p.name),
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();
}

/// Walk the XML to the Synth Tone PARAM list (`<Structure>/<LSB value="20">/
/// <LSB value="00">/<DATA value="01">/<DATA value="0..7">/<PARAM ...>`) and
/// emit a static table of names + categories indexed by linear tone number.
///
/// FloorBoard `midi.xml` stores the catalog as 8 banks of up to 128 PARAMs.
/// We reject any structural surprise (missing parent, non-contiguous bank,
/// gap within a bank) so a future midi.xml change can't silently corrupt
/// the table.
fn emit_pcm_tone_catalog(doc: &Document) -> String {
    let root = doc.root_element();
    let structure = root
        .children()
        .find(|c| c.has_tag_name("Structure"))
        .expect("midi.xml missing <Structure>");

    let lsb20 = find_child_by_value(structure, "LSB", 0x20)
        .expect("Structure missing LSB value=\"20\" (PCM-1-A)");
    let lsb00 = find_child_by_value(lsb20, "LSB", 0x00)
        .expect("PCM-1-A missing LSB value=\"00\"");
    let synth_tone = find_child_by_value(lsb00, "DATA", 0x01)
        .expect("PCM-1-A inner LSB missing DATA value=\"01\" (Synth Tone)");

    let mut tones: Vec<(String, String)> = Vec::with_capacity(910);
    for (next_bank, bank_node) in synth_tone
        .children()
        .filter(|n| n.is_element())
        .enumerate()
    {
        let next_bank = next_bank as u8;
        assert_eq!(
            bank_node.tag_name().name(),
            "DATA",
            "Synth Tone children should all be <DATA>"
        );
        let bank = hex_value_attr(bank_node).expect("bank <DATA> missing value");
        assert_eq!(
            bank, next_bank,
            "PCM tone banks must appear in order; expected 0x{next_bank:02X}, got 0x{bank:02X}"
        );

        for (next_pos, param) in bank_node
            .children()
            .filter(|n| n.is_element())
            .enumerate()
        {
            let next_pos = next_pos as u8;
            assert_eq!(
                param.tag_name().name(),
                "PARAM",
                "PCM tone bank children should be <PARAM>"
            );
            let pos = hex_value_attr(param).expect("PARAM missing value");
            assert_eq!(
                pos, next_pos,
                "PCM tone positions must be dense; expected 0x{next_pos:02X}, got 0x{pos:02X} in bank {bank}"
            );
            let raw_name = param.attribute("name").unwrap_or("").trim();
            let category = param.attribute("customdesc").unwrap_or("").trim();
            tones.push((strip_leading_number(raw_name), category.to_string()));
        }
    }

    assert_eq!(
        tones.len(),
        910,
        "PCM tone catalog should contain exactly 910 tones (got {})",
        tones.len()
    );

    let mut out = String::new();
    writeln!(
        out,
        "// AUTO-GENERATED by build.rs from data/midi.xml — do not edit."
    )
    .unwrap();
    writeln!(
        out,
        "// Mined from GR-55 FloorBoard's midi.xml (Colin Willcocks, GPL-2-or-later)."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Number of populated PCM tones in the GR-55 catalog (0..=909)."
    )
    .unwrap();
    writeln!(
        out,
        "pub const PCM_TONE_COUNT: usize = {};",
        tones.len()
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Tone names indexed by linear tone number (0..=909). Each entry is"
    )
    .unwrap();
    writeln!(
        out,
        "/// the FloorBoard PARAM `name` attribute with its leading 1-based"
    )
    .unwrap();
    writeln!(
        out,
        "/// display number stripped (e.g. \"001  St.Piano 1\" -> \"St.Piano 1\")."
    )
    .unwrap();
    writeln!(out, "pub static PCM_TONE_NAMES: [&str; {}] = [", tones.len()).unwrap();
    for (name, _) in &tones {
        writeln!(out, "    {},", rust_str(name)).unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Per-tone category (from FloorBoard's `customdesc`). Repeats heavily —"
    )
    .unwrap();
    writeln!(
        out,
        "/// 46 distinct values across the 910 tones (\"Synth Lead\" alone covers"
    )
    .unwrap();
    writeln!(out, "/// 123 entries).").unwrap();
    writeln!(
        out,
        "pub static PCM_TONE_CATEGORIES: [&str; {}] = [",
        tones.len()
    )
    .unwrap();
    for (_, category) in &tones {
        writeln!(out, "    {},", rust_str(category)).unwrap();
    }
    writeln!(out, "];").unwrap();
    out
}

/// Per-parameter metadata extracted from a `<DATA>` element's `<PARAM>`
/// children. Returned by [`extract_param_meta`].
#[derive(Debug, Clone, Default)]
struct ParamMeta {
    /// Named single-byte values, e.g. `(0x00, "Off")`, `(0x01, "On")`.
    values: Vec<(u8, String)>,
    /// Raw wire-byte range as `(min, max)`. `None` when the param is
    /// purely enumerated.
    range: Option<(u8, u8)>,
    /// Display range as `(min, max)` (e.g. `(-15, 15)` for "Low Gain"
    /// whose wire bytes are `0..=30`). Signed since many params display
    /// negative values.
    display_range: Option<(i32, i32)>,
}

/// Emit `ParamMeta` fields (`values`, `range`, `display_range`) as
/// Rust literals suitable for inclusion in a generated struct
/// initialiser. Returns the three formatted snippets in that order so
/// they can be threaded into the caller's `writeln!`.
fn format_param_meta(meta: &ParamMeta) -> (String, String, String) {
    let values = if meta.values.is_empty() {
        "&[]".to_string()
    } else {
        let mut s = String::from("&[");
        for (i, (byte, name)) in meta.values.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("(0x{byte:02X}, {})", rust_str(name)));
        }
        s.push(']');
        s
    };
    let range = match meta.range {
        Some((min, max)) => format!("Some((0x{min:02X}, 0x{max:02X}))"),
        None => "None".to_string(),
    };
    let display_range = match meta.display_range {
        Some((min, max)) => format!("Some(({min}, {max}))"),
        None => "None".to_string(),
    };
    (values, range, display_range)
}

/// Walk a `<DATA>` element's `<PARAM>` children and extract:
///
/// - Every `<PARAM value="HH" name="..."/>` entry as a named byte value.
/// - The single `<PARAM value="range" name="MIN/MAX/DISP_MIN/DISP_MAX"/>`
///   entry as a `(range, display_range)` pair, if present. Values are
///   parsed as hex for the wire bytes and signed-decimal (with optional
///   leading `+`) for the display half.
fn extract_param_meta(data: Node) -> ParamMeta {
    let mut meta = ParamMeta::default();
    for param in data
        .children()
        .filter(|c| c.is_element() && c.has_tag_name("PARAM"))
    {
        let value = param.attribute("value").unwrap_or("").trim();
        let name = param.attribute("name").unwrap_or("");
        if value.eq_ignore_ascii_case("range") {
            let parts: Vec<&str> = name.split('/').map(str::trim).collect();
            if parts.len() == 4 {
                let min_b = u8::from_str_radix(parts[0], 16).ok();
                let max_b = u8::from_str_radix(parts[1], 16).ok();
                let disp_min = parts[2].trim_start_matches('+').parse::<i32>().ok();
                let disp_max = parts[3].trim_start_matches('+').parse::<i32>().ok();
                if let (Some(min), Some(max)) = (min_b, max_b) {
                    meta.range = Some((min, max));
                }
                if let (Some(min), Some(max)) = (disp_min, disp_max) {
                    meta.display_range = Some((min, max));
                }
            }
        } else if let Ok(byte) = u8::from_str_radix(value, 16) {
            meta.values.push((byte, name.trim().to_string()));
        }
    }
    meta
}

/// The 20 effect types FloorBoard `midi.xml` declares for MFX at page
/// `0x03` offset `0x05`. Order matches the wire byte. Each entry pairs
/// the canonical short name (used by FloorBoard's `desc` attribute on
/// per-type DATA elements) with a Rust-friendly identifier suffix.
const MFX_TYPE_NAMES: &[(&str, &str)] = &[
    ("Equalizer", "Equalizer"),
    ("Super Filter", "SuperFilter"),
    ("Phaser", "Phaser"),
    ("Step Phaser", "StepPhaser"),
    ("Ring Modulator", "RingModulator"),
    ("Tremolo", "Tremolo"),
    ("Auto Pan", "AutoPan"),
    ("Slicer", "Slicer"),
    ("VK Rotary", "VkRotary"),
    ("Hexa-Chorus", "HexaChorus"),
    ("Space-D", "SpaceD"),
    ("Flanger", "Flanger"),
    ("Step Flanger", "StepFlanger"),
    ("Guitar Amp Sim", "GuitarAmpSim"),
    ("Compressor", "Compressor"),
    ("Limiter", "Limiter"),
    ("3Tap Pan Delay", "ThreeTapPanDelay"),
    ("Time CTRL Delay", "TimeCtrlDelay"),
    ("LOFI Compressor", "LofiCompressor"),
    ("Pitch Shifter", "PitchShifter"),
];

/// Walk MFX's two-page parameter block (`<Structure>/<LSB value="03">/
/// <LSB value="00">/` and `<LSB value="04">/<LSB value="00">/`) and emit
/// a static table mapping each of the 256 linear offsets to its name
/// and (optional) owning MFX type.
///
/// Asserts the disjoint-type-ranges hypothesis: no two type-specific
/// DATA elements share the same offset. If that assertion ever fires,
/// the table model would be wrong — the build fails loudly and the
/// alternative (Rust sum types / per-type structs) would need to be
/// considered.
fn emit_mfx_param_table(doc: &Document) -> String {
    let root = doc.root_element();
    let structure = root
        .children()
        .find(|c| c.has_tag_name("Structure"))
        .expect("midi.xml missing <Structure>");

    let mut entries: Vec<MfxParamRow> = vec![MfxParamRow::placeholder(); 256];
    for (page_byte, mfx_page) in [(0x03_u8, 0_u16), (0x04_u8, 128_u16)] {
        let page_lsb = find_child_by_value(structure, "LSB", page_byte)
            .unwrap_or_else(|| panic!("Structure missing LSB value=\"{page_byte:02X}\""));
        let inner = find_child_by_value(page_lsb, "LSB", 0x00).unwrap_or_else(|| {
            panic!("MFX page 0x{page_byte:02X} missing inner LSB value=\"00\"")
        });
        for data in inner.children().filter(|n| n.is_element() && n.has_tag_name("DATA")) {
            let Some(offset) = hex_value_attr(data) else {
                continue;
            };
            let linear = mfx_page + offset as u16;
            assert!(
                linear < 256,
                "MFX byte at page 0x{page_byte:02X} offset 0x{offset:02X} overflows the 256-byte block"
            );
            let desc = data.attribute("desc").unwrap_or("").trim();
            let customdesc = data.attribute("customdesc").unwrap_or("").trim();
            let owning_type = mfx_type_for_desc(desc);
            let name = mfx_param_name(desc, customdesc);
            let meta = extract_param_meta(data);
            let row = MfxParamRow {
                page: page_byte,
                offset,
                owning_type,
                name: name.to_string(),
                meta,
            };
            // Disjoint check — fires loudly if any (page, offset) already has
            // a type-specific owner and the new entry is also type-specific.
            let prior = &entries[linear as usize];
            if prior.owning_type.is_some() && row.owning_type.is_some() {
                panic!(
                    "MFX disjoint-type-ranges hypothesis violated: linear 0x{linear:02X} \
                     (page 0x{page_byte:02X} offset 0x{offset:02X}) is claimed by both \
                     {:?} ({}) and {:?} ({})",
                    prior.owning_type, prior.name, row.owning_type, row.name
                );
            }
            // A later entry generally shouldn't overwrite a placeholder — but
            // if it does (which shouldn't happen with the current XML), keep
            // the later one since it's the one we just walked into.
            entries[linear as usize] = row;
        }
    }

    let mut out = String::new();
    writeln!(
        out,
        "// AUTO-GENERATED by build.rs from data/midi.xml — do not edit."
    )
    .unwrap();
    writeln!(
        out,
        "// Mined from GR-55 FloorBoard's midi.xml (Colin Willcocks, GPL-2-or-later)."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Which MFX effect type \"owns\" a given byte of the MFX block, if any."
    )
    .unwrap();
    writeln!(
        out,
        "/// `None` for common bytes (the 7 always-present header bytes at page"
    )
    .unwrap();
    writeln!(
        out,
        "/// 0x03 offsets 0x00..=0x06) and for any byte FloorBoard left undocumented."
    )
    .unwrap();
    writeln!(out, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]").unwrap();
    writeln!(out, "pub enum MfxTypeOwner {{").unwrap();
    for (_, ident) in MFX_TYPE_NAMES {
        writeln!(out, "    {ident},").unwrap();
    }
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "/// One byte of the MFX parameter block.").unwrap();
    writeln!(out, "#[derive(Debug, Clone, Copy)]").unwrap();
    writeln!(out, "pub struct MfxParamEntry {{").unwrap();
    writeln!(out, "    /// Wire page byte (0x03 or 0x04).").unwrap();
    writeln!(out, "    pub page: u8,").unwrap();
    writeln!(out, "    /// Offset within the page (0x00..=0x7F).").unwrap();
    writeln!(out, "    pub offset: u8,").unwrap();
    writeln!(
        out,
        "    /// Effect type that this byte parameterises, or None for the common"
    )
    .unwrap();
    writeln!(out, "    /// always-present header bytes.").unwrap();
    writeln!(out, "    pub owning_type: Option<MfxTypeOwner>,").unwrap();
    writeln!(
        out,
        "    /// Human-readable parameter name (e.g. \"Equalizer Low Freq\")."
    )
    .unwrap();
    writeln!(out, "    pub name: &'static str,").unwrap();
    writeln!(
        out,
        "    /// Named byte values for this param. Empty for purely numeric"
    )
    .unwrap();
    writeln!(out, "    /// (range-only) parameters.").unwrap();
    writeln!(out, "    pub values: &'static [(u8, &'static str)],").unwrap();
    writeln!(
        out,
        "    /// Raw wire-byte range (min, max). None for purely enumerated"
    )
    .unwrap();
    writeln!(out, "    /// parameters.").unwrap();
    writeln!(out, "    pub range: Option<(u8, u8)>,").unwrap();
    writeln!(
        out,
        "    /// Display range (min, max) for numeric parameters whose"
    )
    .unwrap();
    writeln!(
        out,
        "    /// display values differ from their wire bytes (e.g. -15..=+15"
    )
    .unwrap();
    writeln!(out, "    /// dB across wire 0x00..=0x1E).").unwrap();
    writeln!(out, "    pub display_range: Option<(i32, i32)>,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Linear MFX block size — bytes at page 0x03 0x00..=0x7F (= 0..=127)"
    )
    .unwrap();
    writeln!(
        out,
        "/// followed by page 0x04 0x00..=0x7F (= 128..=255)."
    )
    .unwrap();
    writeln!(out, "pub const MFX_BLOCK_SIZE: usize = 256;").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "pub static MFX_PARAMS: [MfxParamEntry; MFX_BLOCK_SIZE] = ["
    )
    .unwrap();
    for row in &entries {
        let owner = match row.owning_type {
            Some(ref ident) => format!("Some(MfxTypeOwner::{ident})"),
            None => "None".to_string(),
        };
        let (values, range, display_range) = format_param_meta(&row.meta);
        writeln!(
            out,
            "    MfxParamEntry {{ page: 0x{:02X}, offset: 0x{:02X}, owning_type: {owner}, name: {}, values: {values}, range: {range}, display_range: {display_range} }},",
            row.page,
            row.offset,
            rust_str(&row.name),
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // Per-type byte counts as a sanity-checkable summary.
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut common = 0_usize;
    let mut blank = 0_usize;
    for row in &entries {
        match row.owning_type {
            Some(t) => *counts.entry(t).or_default() += 1,
            None if !row.name.is_empty() => common += 1,
            None => blank += 1,
        }
    }
    writeln!(out, "/// Per-effect-type byte-count summary (debug aid).").unwrap();
    writeln!(
        out,
        "pub static MFX_TYPE_BYTE_COUNTS: &[(&str, usize)] = &["
    )
    .unwrap();
    for (ty, n) in &counts {
        writeln!(out, "    ({}, {n}),", rust_str(ty)).unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(
        out,
        "/// Number of \"common\" (type-agnostic) parameter bytes the table found."
    )
    .unwrap();
    writeln!(out, "pub const MFX_COMMON_BYTES: usize = {common};").unwrap();
    writeln!(
        out,
        "/// Number of byte positions FloorBoard didn't document (no DATA element)."
    )
    .unwrap();
    writeln!(out, "pub const MFX_BLANK_BYTES: usize = {blank};").unwrap();
    out
}

/// The 14 effect types FloorBoard `midi.xml` declares for the MOD slot
/// at page `0x07` offset `0x16`. The string column matches FloorBoard's
/// `desc` attribute exactly (note the trailing whitespace on
/// "Compressor       " and the hyphen in "Uni-Vibe").
const MOD_TYPE_NAMES: &[(&str, &str)] = &[
    ("Distortion", "Distortion"),
    ("Wah", "Wah"),
    ("Compressor", "Compressor"),
    ("Limiter", "Limiter"),
    ("Octave", "Octave"),
    ("Phaser", "Phaser"),
    ("Flanger", "Flanger"),
    ("Tremolo", "Tremolo"),
    ("Rotary", "Rotary"),
    ("Uni-Vibe", "UniVibe"),
    ("Panner", "Panner"),
    ("Delay", "Delay"),
    ("Chorus", "Chorus"),
    ("Equalizer", "Equalizer"),
];

fn emit_mod_param_table(doc: &Document) -> String {
    let root = doc.root_element();
    let structure = root
        .children()
        .find(|c| c.has_tag_name("Structure"))
        .expect("midi.xml missing <Structure>");
    let page07 = find_child_by_value(structure, "LSB", 0x07)
        .expect("Structure missing LSB value=\"07\" (PreAmp / MOD)");
    let inner = find_child_by_value(page07, "LSB", 0x00)
        .expect("MOD page 0x07 missing inner LSB value=\"00\"");

    let mut entries: Vec<TypedParamRow> = (0..128)
        .map(|i| TypedParamRow {
            page: 0x07,
            offset: i as u8,
            owning_type: None,
            name: String::new(),
            meta: ParamMeta::default(),
        })
        .collect();
    for data in inner
        .children()
        .filter(|n| n.is_element() && n.has_tag_name("DATA"))
    {
        let Some(offset) = hex_value_attr(data) else {
            continue;
        };
        if offset >= 128 {
            continue;
        }
        let raw_desc = data.attribute("desc").unwrap_or("");
        let customdesc = data.attribute("customdesc").unwrap_or("").trim();
        let owning_type = mod_type_for_desc(raw_desc);
        let name = mfx_param_name(raw_desc.trim(), customdesc);
        let meta = extract_param_meta(data);
        let row = TypedParamRow {
            page: 0x07,
            offset,
            owning_type,
            name,
            meta,
        };
        let prior = &entries[offset as usize];
        if prior.owning_type.is_some() && row.owning_type.is_some() {
            panic!(
                "MOD disjoint-type-ranges hypothesis violated: offset 0x{offset:02X} \
                 claimed by both {:?} and {:?}",
                prior.owning_type, row.owning_type
            );
        }
        entries[offset as usize] = row;
    }

    emit_table_module(
        "MOD",
        "ModTypeOwner",
        "MOD_PARAMS",
        "MOD_BLOCK_SIZE",
        128,
        MOD_TYPE_NAMES,
        &entries,
        "Mod_TYPE_BYTE_COUNTS",
    )
    .replace("Mod_TYPE_BYTE_COUNTS", "MOD_TYPE_BYTE_COUNTS")
}

fn mod_type_for_desc(desc: &str) -> Option<&'static str> {
    // Match on trim_end since FloorBoard occasionally pads the desc
    // (e.g. "Compressor       ").
    let trimmed = desc.trim_end();
    for (xml_name, ident) in MOD_TYPE_NAMES {
        if *xml_name == trimmed {
            return Some(ident);
        }
    }
    None
}

/// Mine the Modeling parameter table on pages 0x10 + 0x11. Output a
/// flat 256-byte table with structured ownership: each populated byte
/// carries (mode, category, types, customdesc).
///
/// Mode is derived from the page (0x10 = Guitar Mode for type-specific
/// bytes, 0x11 = Bass Mode for type-specific bytes; common-header bytes
/// on page 0x10 0x00..=0x1D apply to both modes). Category comes from
/// FloorBoard's `abbr`. Types are FloorBoard's `desc` — either a
/// dash-separated list of numeric type IDs (E.GTR/Bass), a sub-type
/// name (steel/Nylon/Sitar/...), or a longer phrase ("Analog GR
/// Envelope modulation"). We don't try to parse the desc into a
/// structured type set in build.rs; we expose it as a raw string and
/// let consumers split/match as they need.
fn emit_modeling_param_table(doc: &Document) -> String {
    let root = doc.root_element();
    let structure = root
        .children()
        .find(|c| c.has_tag_name("Structure"))
        .expect("midi.xml missing <Structure>");

    let mut entries: Vec<ModelingParamRow> =
        (0..256).map(|_| ModelingParamRow::placeholder()).collect();

    for (page_byte, base) in [(0x10_u8, 0_usize), (0x11_u8, 128_usize)] {
        let page_lsb = match find_child_by_value(structure, "LSB", page_byte) {
            Some(p) => p,
            None => continue,
        };
        let inner = match find_child_by_value(page_lsb, "LSB", 0x00) {
            Some(i) => i,
            None => continue,
        };
        for data in inner
            .children()
            .filter(|n| n.is_element() && n.has_tag_name("DATA"))
        {
            let Some(offset) = hex_value_attr(data) else {
                continue;
            };
            let linear = base + offset as usize;
            if linear >= entries.len() {
                continue;
            }
            let abbr = data.attribute("abbr").unwrap_or("").trim().to_string();
            let desc = data.attribute("desc").unwrap_or("").trim().to_string();
            let customdesc = data.attribute("customdesc").unwrap_or("").trim().to_string();

            // Disjoint-at-wire check: each (page, offset) should be claimed
            // by at most ONE DATA element in the XML. (Multiple TYPES per
            // byte via desc dash-lists is fine — that's intra-category
            // sharing, not a structural overlap.)
            let prior = &entries[linear];
            if prior.populated {
                panic!(
                    "Modeling table: two DATA elements claim linear 0x{linear:04X} \
                     (page 0x{page_byte:02X} offset 0x{offset:02X}) — prior {{abbr={:?}, \
                     desc={:?}}} vs new {{abbr={:?}, desc={:?}}}",
                    prior.abbr, prior.desc, abbr, desc
                );
            }
            let meta = extract_param_meta(data);
            entries[linear] = ModelingParamRow {
                page: page_byte,
                offset,
                abbr,
                desc,
                customdesc,
                populated: true,
                meta,
            };
        }
    }

    // Categories the table actually carries (post-extraction).
    let mut categories: BTreeMap<String, usize> = BTreeMap::new();
    let mut shared_byte_count = 0_usize;
    let mut single_type_byte_count = 0_usize;
    let mut common_byte_count = 0_usize;
    for row in entries.iter().filter(|r| r.populated) {
        *categories.entry(row.abbr.clone()).or_default() += 1;
        if row.desc.is_empty() || row.abbr == "Modeling" || row.abbr == "NS" {
            common_byte_count += 1;
        } else if row.desc.contains('-') && row.desc.chars().any(|c| c.is_ascii_digit()) {
            shared_byte_count += 1;
        } else if row.desc.contains('-') {
            // Dash-separated name list like "Jazz-PB" — also shared.
            shared_byte_count += 1;
        } else {
            single_type_byte_count += 1;
        }
    }

    let mut out = String::new();
    writeln!(
        out,
        "// AUTO-GENERATED by build.rs from data/midi.xml — do not edit."
    )
    .unwrap();
    writeln!(
        out,
        "// Mined from GR-55 FloorBoard's midi.xml (Colin Willcocks, GPL-2-or-later)."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Which mode a Modeling byte belongs to. Common-header bytes on"
    )
    .unwrap();
    writeln!(
        out,
        "/// page 0x10 0x00..=0x1D return `Both`; bytes on page 0x10 0x1E.."
    )
    .unwrap();
    writeln!(
        out,
        "/// return `Guitar`; bytes on page 0x11 return `Bass`."
    )
    .unwrap();
    writeln!(out, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]").unwrap();
    writeln!(out, "pub enum ModelingMode {{").unwrap();
    writeln!(out, "    Guitar,").unwrap();
    writeln!(out, "    Bass,").unwrap();
    writeln!(out, "    Both,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// One byte of the Modeling parameter block. The ownership is"
    )
    .unwrap();
    writeln!(
        out,
        "/// richer than MFX/MOD: a single byte can be shared by multiple"
    )
    .unwrap();
    writeln!(
        out,
        "/// instrument types within a category (FloorBoard encodes this"
    )
    .unwrap();
    writeln!(
        out,
        "/// via dash-separated `desc` strings like \"01-02-03\" or"
    )
    .unwrap();
    writeln!(out, "/// \"Jazz-PB\").").unwrap();
    writeln!(out, "#[derive(Debug, Clone, Copy)]").unwrap();
    writeln!(out, "pub struct ModelingParamEntry {{").unwrap();
    writeln!(out, "    pub page: u8,").unwrap();
    writeln!(out, "    pub offset: u8,").unwrap();
    writeln!(out, "    pub mode: ModelingMode,").unwrap();
    writeln!(
        out,
        "    /// Top-level category from `abbr` — e.g. \"E.GTR\", \"Acoustic\","
    )
    .unwrap();
    writeln!(
        out,
        "    /// \"Bass\", \"Synth\", \"Modeling\" (common header bytes), \"NS\""
    )
    .unwrap();
    writeln!(out, "    /// (noise suppressor), or \"\" (unmapped).").unwrap();
    writeln!(out, "    pub category: &'static str,").unwrap();
    writeln!(
        out,
        "    /// Type subset from `desc` — dash-separated IDs (\"01-02\","
    )
    .unwrap();
    writeln!(
        out,
        "    /// \"03-04-05-07\"), sub-type names (\"steel\", \"Nylon\","
    )
    .unwrap();
    writeln!(
        out,
        "    /// \"Jazz-PB\"), or descriptive phrases (\"Analog GR Envelope"
    )
    .unwrap();
    writeln!(
        out,
        "    /// modulation\"). Empty for bytes that apply to all types"
    )
    .unwrap();
    writeln!(out, "    /// of the category.").unwrap();
    writeln!(out, "    pub types: &'static str,").unwrap();
    writeln!(out, "    pub name: &'static str,").unwrap();
    writeln!(
        out,
        "    /// Named byte values (empty for purely-numeric params)."
    )
    .unwrap();
    writeln!(out, "    pub values: &'static [(u8, &'static str)],").unwrap();
    writeln!(out, "    /// Raw wire-byte range (min, max).").unwrap();
    writeln!(out, "    pub range: Option<(u8, u8)>,").unwrap();
    writeln!(
        out,
        "    /// Display range (min, max) when it differs from the wire range."
    )
    .unwrap();
    writeln!(out, "    pub display_range: Option<(i32, i32)>,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub const MODELING_BLOCK_SIZE: usize = 256;").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "pub static MODELING_PARAMS: [ModelingParamEntry; MODELING_BLOCK_SIZE] = ["
    )
    .unwrap();
    for (linear, row) in entries.iter().enumerate() {
        let mode = if !row.populated || linear < 0x1E {
            "ModelingMode::Both"
        } else if row.page == 0x10 {
            "ModelingMode::Guitar"
        } else {
            "ModelingMode::Bass"
        };
        let (values, range, display_range) = format_param_meta(&row.meta);
        writeln!(
            out,
            "    ModelingParamEntry {{ page: 0x{:02X}, offset: 0x{:02X}, mode: {mode}, \
             category: {}, types: {}, name: {}, values: {values}, range: {range}, \
             display_range: {display_range} }},",
            row.page,
            row.offset,
            rust_str(&row.abbr),
            rust_str(&row.desc),
            rust_str(&row.customdesc),
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Number of populated Modeling bytes per top-level category."
    )
    .unwrap();
    writeln!(
        out,
        "pub static MODELING_CATEGORY_BYTE_COUNTS: &[(&str, usize)] = &["
    )
    .unwrap();
    for (cat, n) in &categories {
        let label = if cat.is_empty() { "<unmapped>" } else { cat };
        writeln!(out, "    ({}, {n}),", rust_str(label)).unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(
        out,
        "/// Bytes shared across multiple types within their category"
    )
    .unwrap();
    writeln!(out, "/// (desc contains a dash-separated list).").unwrap();
    writeln!(
        out,
        "pub const MODELING_SHARED_TYPE_BYTES: usize = {shared_byte_count};"
    )
    .unwrap();
    writeln!(
        out,
        "/// Bytes owned by exactly one type within their category."
    )
    .unwrap();
    writeln!(
        out,
        "pub const MODELING_SINGLE_TYPE_BYTES: usize = {single_type_byte_count};"
    )
    .unwrap();
    writeln!(
        out,
        "/// Bytes that aren't type-specific (common header, NS, mode-bridging)."
    )
    .unwrap();
    writeln!(
        out,
        "pub const MODELING_COMMON_BYTES: usize = {common_byte_count};"
    )
    .unwrap();
    out
}

#[derive(Debug, Clone)]
struct ModelingParamRow {
    page: u8,
    offset: u8,
    abbr: String,
    desc: String,
    customdesc: String,
    populated: bool,
    meta: ParamMeta,
}

impl ModelingParamRow {
    fn placeholder() -> Self {
        ModelingParamRow {
            page: 0,
            offset: 0,
            abbr: String::new(),
            desc: String::new(),
            customdesc: String::new(),
            populated: false,
            meta: ParamMeta::default(),
        }
    }
}

/// Shared codegen for MFX-style tables. `block_label` and `enum_name` go
/// into doc strings; the rest names the Rust items.
#[allow(clippy::too_many_arguments)]
fn emit_table_module(
    block_label: &str,
    enum_name: &str,
    array_name: &str,
    size_const: &str,
    block_size: usize,
    type_names: &[(&str, &str)],
    entries: &[TypedParamRow],
    counts_array_name: &str,
) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "// AUTO-GENERATED by build.rs from data/midi.xml — do not edit."
    )
    .unwrap();
    writeln!(
        out,
        "// Mined from GR-55 FloorBoard's midi.xml (Colin Willcocks, GPL-2-or-later)."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Which {block_label} effect type \"owns\" a given byte of the block."
    )
    .unwrap();
    writeln!(out, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]").unwrap();
    writeln!(out, "pub enum {enum_name} {{").unwrap();
    for (_, ident) in type_names {
        writeln!(out, "    {ident},").unwrap();
    }
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "/// One byte of the {block_label} parameter block.").unwrap();
    writeln!(out, "#[derive(Debug, Clone, Copy)]").unwrap();
    writeln!(out, "pub struct ParamEntry {{").unwrap();
    writeln!(out, "    pub page: u8,").unwrap();
    writeln!(out, "    pub offset: u8,").unwrap();
    writeln!(out, "    pub owning_type: Option<{enum_name}>,").unwrap();
    writeln!(out, "    pub name: &'static str,").unwrap();
    writeln!(
        out,
        "    /// Named byte values (empty for purely-numeric params)."
    )
    .unwrap();
    writeln!(out, "    pub values: &'static [(u8, &'static str)],").unwrap();
    writeln!(
        out,
        "    /// Raw wire-byte range (min, max). None when purely enumerated."
    )
    .unwrap();
    writeln!(out, "    pub range: Option<(u8, u8)>,").unwrap();
    writeln!(
        out,
        "    /// Display range (min, max) when it differs from the wire range."
    )
    .unwrap();
    writeln!(out, "    pub display_range: Option<(i32, i32)>,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub const {size_const}: usize = {block_size};").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "pub static {array_name}: [ParamEntry; {size_const}] = ["
    )
    .unwrap();
    for row in entries {
        let owner = match row.owning_type {
            Some(ident) => format!("Some({enum_name}::{ident})"),
            None => "None".to_string(),
        };
        let (values, range, display_range) = format_param_meta(&row.meta);
        writeln!(
            out,
            "    ParamEntry {{ page: 0x{:02X}, offset: 0x{:02X}, owning_type: {owner}, name: {}, values: {values}, range: {range}, display_range: {display_range} }},",
            row.page,
            row.offset,
            rust_str(&row.name),
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut common = 0_usize;
    let mut blank = 0_usize;
    for row in entries {
        match row.owning_type {
            Some(t) => *counts.entry(t).or_default() += 1,
            None if !row.name.is_empty() => common += 1,
            None => blank += 1,
        }
    }
    writeln!(
        out,
        "pub static {counts_array_name}: &[(&str, usize)] = &["
    )
    .unwrap();
    for (ty, n) in &counts {
        writeln!(out, "    ({}, {n}),", rust_str(ty)).unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out, "pub const COMMON_BYTES: usize = {common};").unwrap();
    writeln!(out, "pub const BLANK_BYTES: usize = {blank};").unwrap();
    out
}

#[derive(Debug, Clone)]
struct TypedParamRow {
    page: u8,
    offset: u8,
    owning_type: Option<&'static str>,
    name: String,
    meta: ParamMeta,
}

#[derive(Debug, Clone)]
struct MfxParamRow {
    page: u8,
    offset: u8,
    owning_type: Option<&'static str>,
    name: String,
    meta: ParamMeta,
}

impl MfxParamRow {
    fn placeholder() -> Self {
        MfxParamRow {
            page: 0,
            offset: 0,
            owning_type: None,
            name: String::new(),
            meta: ParamMeta::default(),
        }
    }
}

fn mfx_type_for_desc(desc: &str) -> Option<&'static str> {
    for (xml_name, ident) in MFX_TYPE_NAMES {
        if *xml_name == desc {
            return Some(ident);
        }
    }
    None
}

fn mfx_param_name(desc: &str, customdesc: &str) -> String {
    if customdesc.is_empty() {
        desc.trim_end_matches(':').trim().to_string()
    } else if desc.is_empty() || desc == "MFX:" {
        customdesc.to_string()
    } else {
        format!("{} {customdesc}", desc.trim_end_matches(':').trim())
    }
}

fn find_child_by_value<'a, 'input>(
    parent: Node<'a, 'input>,
    tag: &str,
    value: u8,
) -> Option<Node<'a, 'input>> {
    parent
        .children()
        .filter(|c| c.is_element() && c.has_tag_name(tag))
        .find(|c| hex_value_attr(*c) == Some(value))
}

/// Strip a leading 1-based tone number from `"001  St.Piano 1"` →
/// `"St.Piano 1"`. Tolerant of arbitrary leading whitespace / digit
/// widths.
fn strip_leading_number(s: &str) -> String {
    let s = s.trim();
    let mut idx = 0;
    let mut saw_digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            saw_digit = true;
            idx += c.len_utf8();
        } else {
            break;
        }
    }
    if !saw_digit {
        return s.to_string();
    }
    s[idx..].trim_start().to_string()
}

fn rust_str(s: &str) -> String {
    let mut buf = String::with_capacity(s.len() + 2);
    buf.push('"');
    for c in s.chars() {
        match c {
            '\\' => buf.push_str("\\\\"),
            '"' => buf.push_str("\\\""),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                write!(buf, "\\u{{{:x}}}", c as u32).unwrap();
            }
            c => buf.push(c),
        }
    }
    buf.push('"');
    buf
}
