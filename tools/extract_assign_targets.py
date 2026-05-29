#!/usr/bin/env python3
"""Extract the assign-target enumeration from FloorBoard's midi.xml.

The Tables / DATA(customdesc="Target") branch carries the GR-55's
modulation-destination table. Two top-level Target sub-trees (Guitar
Mode + Bass Mode), each with 3 numbered sub-lists of PARAM entries
named with human-readable strings like "PCM1 Tone Level" or
"Modeling Tone String 3 Level".

Wire layout (per gr55-core/src/patch.rs::Assign):
- target (1 byte) selects the sub-list (0, 1, or 2)
- target_b (1 byte) selects the entry within the sub-list

The list values run past 0x7F in the XML (up to ~0xFD); FloorBoard
treats `target` as a 7-bit byte plus a one-bit overflow in `target_b`.
Emit `value` as-is (0..255); the editor splits low/high when writing
the wire bytes.

Emits Rust source to gr55-core/src/generated/assign_targets.rs.

Run from the repo root:
    python tools/extract_assign_targets.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
XML = ROOT / "external" / "floorboard" / "midi.xml"
OUT = ROOT / "gr55-core" / "src" / "generated" / "assign_targets.rs"

DATA_OPEN = re.compile(
    r'<DATA\s+value="([0-9A-Fa-f]+)"\s+name="[^"]*"\s+abbr="([^"]*)"\s+'
    r'desc="([^"]*)"\s+customdesc="([^"]*)"\s*(/?)>'
)
DATA_CLOSE = re.compile(r"^\s*</DATA>")
PARAM = re.compile(r'<PARAM\s+value="([0-9A-Fa-f]+)"\s+name="([^"]*)"')


def main() -> None:
    text = XML.read_text(encoding="utf-16")
    lines = text.splitlines()

    # Walk the file with a stack of open <DATA> frames. Each frame is
    # (value, abbr, desc, customdesc). A Target tree opens when a DATA
    # has customdesc="Target" AND abbr is "Guitar Mode" or "Bass Mode"
    # — the Structure section also references customdesc="Target" via
    # forward refs (abbr="") which we ignore. Inside an open Target
    # tree, the immediate child DATAs with desc="assign" set the
    # sub-list byte; their PARAM children are the actual destination
    # entries.
    stack: list[tuple[int, str, str, str]] = []
    target_depth: int | None = None
    sub_list: int | None = None
    mode: str | None = None

    entries: list[tuple[str, int, int, str]] = []  # (mode, list_byte, value, name)

    for raw in lines:
        line = raw.strip()

        m = DATA_OPEN.search(raw)
        if m:
            val = int(m.group(1), 16)
            abbr = m.group(2)
            desc = m.group(3)
            cdesc = m.group(4)
            self_closing = bool(m.group(5))
            if not self_closing:
                stack.append((val, abbr, desc, cdesc))
                if cdesc == "Target" and abbr in ("Guitar Mode", "Bass Mode"):
                    target_depth = len(stack)
                    mode = "guitar" if abbr == "Guitar Mode" else "bass"
                elif (
                    target_depth is not None
                    and len(stack) == target_depth + 1
                    and desc == "assign"
                ):
                    sub_list = val
            continue

        if DATA_CLOSE.match(line):
            if stack:
                _, _, _, cdesc_top = stack.pop()
                # Closing the active Target frame → exit Target mode.
                if target_depth is not None and len(stack) < target_depth:
                    target_depth = None
                    sub_list = None
                    mode = None
                # Closing an assign sub-list → clear sub_list but stay
                # in the Target tree.
                elif target_depth is not None and len(stack) == target_depth:
                    sub_list = None
            continue

        if sub_list is not None and mode is not None:
            pm = PARAM.search(line)
            if pm:
                pvalue = int(pm.group(1), 16)
                pname = pm.group(2)
                entries.append((mode, sub_list, pvalue, pname))

    # Dedupe by (mode, list, value). Multiple Target trees exist for
    # each mode (one per MSB region) but the destination names are
    # consistent across them — first occurrence wins.
    seen: set[tuple[str, int, int]] = set()
    deduped: list[tuple[str, int, int, str]] = []
    for e in entries:
        key = (e[0], e[1], e[2])
        if key in seen:
            continue
        seen.add(key)
        deduped.append(e)
    entries = deduped

    by_mode_list: dict[tuple[str, int], int] = {}
    for mode_, lb, _v, _n in entries:
        by_mode_list[(mode_, lb)] = by_mode_list.get((mode_, lb), 0) + 1

    print(f"Captured {len(entries)} target entries.")
    for (m_, lb), n in sorted(by_mode_list.items()):
        print(f"  {m_} list {lb}: {n} entries")

    # Emit Rust.
    out_lines = [
        "// Static factual data about Roland's GR-55 MIDI protocol — assign-target",
        "// enumeration extracted from FloorBoard midi.xml's `<Tables>` /",
        "// `<DATA customdesc=\"Target\">` branches.",
        "//",
        "// Two Target sub-trees exist: Guitar Mode and Bass Mode. Each has 3",
        "// numbered sub-lists of named modulation destinations. The Assign's",
        "// `target` byte selects the sub-list (0, 1, or 2); the `target_b`",
        "// byte selects the entry within the sub-list. PARAM value attributes",
        "// run past 0x7F (up to ~0xFD); on the wire the low 7 bits land in",
        "// `target_b` and the high bit lands in `target_c`'s LSB. Editors can",
        "// keep this as a single u8 in [0, 255] for display.",
        "//",
        "// Generated by `tools/extract_assign_targets.py`; hand edits permitted.",
        "//",
        "// Such factual data is not subject to copyright in the United States",
        "// (Feist Publications, Inc. v. Rural Telephone Service Co., 499 U.S.",
        "// 340 (1991)); it represents discoveries about a published machine",
        "// protocol, not creative authorship.",
        "",
        "/// Which patch-mode Target tree an entry belongs to.",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub enum AssignTargetMode {",
        "    Guitar,",
        "    Bass,",
        "}",
        "",
        "impl AssignTargetMode {",
        "    pub fn as_snake(&self) -> &'static str {",
        "        match self {",
        "            AssignTargetMode::Guitar => \"guitar\",",
        "            AssignTargetMode::Bass => \"bass\",",
        "        }",
        "    }",
        "}",
        "",
        "/// One entry in the assign-target enumeration.",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub struct AssignTargetEntry {",
        "    /// Which top-level mode this entry belongs to.",
        "    pub mode: AssignTargetMode,",
        "    /// Wire byte for the Assign `target` field (sub-list index 0..=2).",
        "    pub list: u8,",
        "    /// Wire byte for the Assign `target_b` field (entry within the sub-list).",
        "    pub value: u8,",
        "    /// Human-readable parameter name (e.g. `\"PCM1 Tone Level\"`).",
        "    pub name: &'static str,",
        "}",
        "",
        f"pub const ASSIGN_TARGET_COUNT: usize = {len(entries)};",
        "",
        f"pub static ASSIGN_TARGETS: [AssignTargetEntry; ASSIGN_TARGET_COUNT] = [",
    ]
    for mode_, lb, v, n in entries:
        m_variant = "Guitar" if mode_ == "guitar" else "Bass"
        escaped = n.replace("\\", "\\\\").replace('"', '\\"')
        out_lines.append(
            f"    AssignTargetEntry {{ mode: AssignTargetMode::{m_variant}, "
            f"list: 0x{lb:02X}, value: 0x{v:02X}, name: \"{escaped}\" }},"
        )
    out_lines.append("];")
    out_lines.append("")

    OUT.write_text("\n".join(out_lines), encoding="utf-8")
    print(f"Wrote {OUT}")


if __name__ == "__main__":
    main()
