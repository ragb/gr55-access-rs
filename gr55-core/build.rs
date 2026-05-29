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
