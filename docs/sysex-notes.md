# GR-55 SysEx notes

Running log of what we know about the Roland GR-55 MIDI / SysEx protocol.
Append-only-ish: don't delete refuted entries, mark them refuted in the
"Refuted / dead ends" section at the bottom.

For each claim, **provenance** is one of:

- **HIGH** — FloorBoard `midi.xml` + Roland owner's manual + observed wire bytes (`.syx` fixture or live capture) all agree.
- **MEDIUM** — FloorBoard `midi.xml` + Roland owner's manual agree; no wire-byte confirmation yet.
- **LOW** — single source (FloorBoard, or PDF, or VController) only; not yet cross-checked.

## 1. Primary sources

| Source | URL / location | Notes |
|--------|---------------|-------|
| GR-55 FloorBoard `midi.xml` | `gr55-core/data/midi.xml` (vendored, UTF-8) | Address map source of truth. UTF-16 original at `docs/spec/floorboard_src/gr55floorboard_source/midi.xml`. Actively maintained (snapshot dated 2025-08-28). |
| GR-55 FloorBoard `midi.xsd` | `gr55-core/data/midi.xsd` | Schema for `midi.xml`. Defines the `<SysX>` document layout (`<Header>`, `<Address>`, `<System>`, `<Structure>`, `<Footer>`, `<Tables>`, `<MPT>`). |
| Roland GR-55 Owner's Manual | `docs/spec/gr55_owners_manual.pdf` (local, not committed) | 28 MB PDF. Has MIDI Implementation Chart and likely a detailed appendix (TODO: inspect). |
| VController `MIDI_GR55.ino` | https://github.com/sixeight7/VController/blob/master/MIDI_GR55.ino | Hardware-tested third-party driver. Useful tiebreaker. |
| FloorBoard `default.syx` / `system.syx` / `EZ-Tone.syx` | `gr55-core/tests/fixtures/floorboard_*.syx` | Wire-byte fixtures bundled with FloorBoard. Use for codec round-trip tests. |

## 2. Framing

| Field        | Value          | Provenance |
|--------------|----------------|------------|
| SOX          | `F0`           | HIGH (universal, observed in every fixture) |
| Manufacturer | `41` (Roland)  | HIGH (FloorBoard XML + `default.syx`) |
| Device ID    | `10`..`1F` (UI 1..16); `7F` broadcast | MEDIUM (FloorBoard XML names default `10`; broadcast behavior unverified) |
| Model ID     | `00 00 53` (3 bytes) | HIGH (FloorBoard XML, VController, `default.syx`, `system.syx`) |
| Command      | DT1 send = `12`, RQ1 (receive request) = `11` | HIGH (FloorBoard XML labels both "DT1"; byte semantics confirmed: `12` writes, `11` reads) |
| Address      | 4 bytes, MSB first, each `00`..`7F` | HIGH (observed in fixtures) |
| Checksum     | `(128 − (sum(addr+data) mod 128)) mod 128` | MEDIUM (standard Roland formula; not yet verified on a fixture — TODO) |
| EOX          | `F7`           | HIGH (universal) |

Full frame on the wire:

```
F0 41 [dev] 00 00 53 [cmd] [addr*4] [data...] [chk] F7
```

Observed example from `floorboard_default_patch.syx` (first frame):

```
F0 41 10 00 00 53 12  18 00 00 00  "Init Patch      " ... [chk] F7
```

## 3. Universal Identity Reply

Status: **UNKNOWN**. FloorBoard does not use the identity-request handshake.
The Roland MIDI Implementation Chart has not been inspected for the family code
and software-revision bytes.

To resolve: either (a) inspect the owner's manual PDF appendix, or (b) capture a
reply from the real device via friend-mediated GUI session.

Expected reply shape: `F0 7E 7F 06 02 41 [family-lo family-hi] [number-lo number-hi] [4-byte sw-rev] F7`.

## 4. Top-level address space

Read out of FloorBoard `midi.xml`'s `<Address>` section and the top-level
`<System>` / `<Structure>` anchors.

| MSB     | Meaning                                      | Provenance | Notes |
|---------|----------------------------------------------|------------|-------|
| `01 00 00 00` | System area base                        | HIGH       | `system.syx` opens with DT1 to this address |
| `18 00 00 00` | Saved-patch file-format address (?)     | LOW        | `default.syx` writes its patch payload here; appears to be a canonical "file" address that gets remapped to live USER / TEMP / PRESET on load. Open question. |
| `60 00 00 00` | Temporary Buffer / Edit Buffer base     | MEDIUM     | FloorBoard XML labels it "Temporary Buffer (Bulk)" and "Temporary Buffer (Individual)" — two access modes at the same MSB. Distinct from RE-202's `20 00 00 00` edit-buffer address. |

XML top-level sections (mirrored in `midi.xsd`), with observed line ranges in `midi.utf8.xml`:

| Section | Lines | Bytes (~) | What it is |
|---------|-------|-----------|------------|
| `<Header>` | 12–35 | tiny | SysEx framing constants (SOX, manufacturer, device ID, model ID, command IDs). |
| `<Address>` | 36–44 | tiny | Top-level address anchors enumerated in the table above. |
| `<System>` | 45–37,142 | ~2 MB | System area parameter map. Bulk of the size is the 297-entry patch-slot enumeration in the "Current Patch" parameter (each User XX:Y is one `<PARAM>`). |
| `<Structure>` | 37,143–55,307 | ~1 MB | Per-patch parameter map. Opens with `Names and Pedal`: Guitar/Bass Mode, then 12 character-slot fields (each enumerating ASCII 0x20–0x7F), then PCM tones / COSM / Effects. **This is the layered patch model.** |
| `<Footer>` | 55,308–55,312 | tiny | EOX byte (`F7`). |
| `<Tables>` | 55,313–64,422 | ~500 KB | Assign-target enumeration — the flat list of every patch parameter that can be a modulation/CC destination (PCM1/PCM2/Mod/COSM/MFX subsystems). `customdesc` on each `<PARAM>` is a reference code pointing to a range table elsewhere. |
| `<MPT>` | 64,423–65,135 | ~50 KB | **MIDI Patch Table.** Bank-Select × PC# → patch routing matrix (e.g. `BANK 0: PC#001` → `L_01:1`). This is the user-configurable program-change-to-patch routing used when the GR-55 receives PC messages over MIDI. |

Total `<PARAM>` element count across the whole file: **60,977**. Most are enum value options (e.g. 96 character-choice PARAMs per patch-name character slot), not unique addressable parameters.

## 5. Patch slot numbering

Per FloorBoard `<System>` "Current Patch" parameter:

- User patches numbered as `User XX:Y` where XX = 1..99 and Y = 1..3.
- Total user slots: 99 × 3 = **297**. Matches Sound Programming's overview.
- Each bank of 3 sub-slots is selectable as a single PC# range (TODO: confirm PC# → patch mapping).
- Factory presets total **468** after firmware v1.50 (360 guitar + 108 bass). Distribution across MSB / LSB not yet mined from the XML.

## 6. System area parameters

To be filled in once `<System>` is mined into Rust types.

## 7. Per-patch parameters

To be filled in once `<Structure>` is mined into Rust types.

## 8. CC map

To be filled in.

## 9. Tempo / BPM encoding

UNKNOWN. RE-202 uses 4-nibble MSB→LSB packing. Need to find the tempo
parameter in FloorBoard XML and confirm by inspecting a patch dump.

## 10. Firmware-version sensitivity

FloorBoard `midi.xml` is dated 2025-08-28 with `version="1.0"`. No public
record of the address map changing between GR-55 firmware versions (v1.0
through v1.50, the latter from ~2012, factory presets expanded but no
documented protocol break). Assume the map is stable. Capture
software-revision bytes into every fixture filename anyway when friend-mediated
captures become available.

## 11. Open questions (parking lot)

- [ ] What does address MSB `18` represent — file-format canonical patch, or a live area? (Cross-check with the owner's manual.)
- [x] What is the `<MPT>` section in `midi.xml`? **Resolved:** MIDI Patch Table — Bank/PC → patch routing matrix.
- [ ] Universal Identity Reply bytes (family code, sw-rev).
- [ ] Tempo / BPM encoding.
- [ ] Confirm checksum formula matches a real fixture (compute over `default.syx` first frame, compare to the byte before `F7`).
- [ ] PC# → patch mapping (does PC#0 select User 01:1 or User 01:1's whole bank-of-3?).
- [ ] Broadcast device-id (`7F`) behavior.
- [ ] Byte size of one full patch dump (sum payload bytes across `default.syx`'s DT1 messages).
- [ ] Edit-buffer mirror behavior: does reading `60 ...` reflect the currently-active patch, like RE-202?

## 12. Refuted / dead ends

(none yet)
