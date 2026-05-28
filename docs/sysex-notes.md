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
| FloorBoard `menuPage_system.cpp` | `docs/spec/floorboard_src/.../menuPage_system.cpp` | The UI wiring file for FloorBoard's System menu page. Each `addComboBox(...)` / `addKnob(...)` call carries the `(hex1, hex2, hex3)` wire-address triplet — the cleanest enumeration of every user-facing System parameter. Used to drive `gr55-core::system` field list. |
| FloorBoard `menuPage_master.cpp` | `docs/spec/floorboard_src/.../menuPage_master.cpp` | Same pattern for Master settings (smaller). |
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

Status: **MEDIUM** — pattern extracted from FloorBoard `globalVariables.h:41`.

```
F0 7E 10 06 02 41 53 02 00 00 00 00 00 00 F7
   │  │  │  │  │  │  │  │  │  │  │           │
   │  │  │  │  │  │  │  │  │  │  └─sw-rev────┤
   │  │  │  │  │  │  │  │  └─number───────────┤
   │  │  │  │  │  │  └──family code (53 02)──┤
   │  │  │  │  │  └──Roland manufacturer──────┤
   │  │  │  └──sub-id1=06, sub-id2=02 (reply)─┤
   │  │  └──device ID (FloorBoard uses 10)────┤
   │  └──Universal Non-Realtime (7E)──────────┤
   └──SOX─────────────────────────────────────┘
```

- **Family code = `53 02`** (LSB, MSB; 16-bit value `0x0253`). Note: same low
  byte (`53`) as the GR-55's 3-byte model ID.
- **Device family number = `00 00`**.
- **Software revision = `00 00 00 00`** in FloorBoard's identity-pattern string
  (FloorBoard substring-matches the reply, so the literal `00 00 00 00` is just
  the pattern; the device probably sends real sw-rev bytes here that FloorBoard
  ignores). Confirm against a real device when possible.
- The request to elicit this reply: `F0 7E 10 06 01 F7` (per `globalVariables.h:40`).

## 4. Top-level address space

Read out of FloorBoard `midi.xml`'s `<Address>` section and the top-level
`<System>` / `<Structure>` anchors.

| MSB     | Meaning                                      | Provenance | Notes |
|---------|----------------------------------------------|------------|-------|
| `01 *` | System area, page 1 (start of System parameters) | HIGH | `system.syx` opens with DT1 to this MSB |
| `02 *` | System area, additional pages | HIGH | Discovered via build.rs codegen of midi.xml: `<System>` contains two top-level `<LSB>` children (`value="01"` and `value="02"`). System frames in `system.syx` at addresses like `[02, 00, 02, 00]` correspond to MSB-02 system parameters, not patches. |
| `18 00 00 00` | Live current-patch write area | HIGH | FloorBoard's `globalVariables.h:77` calls this `tempDataWrite`; the device keeps the currently-edited patch here before save. `default.syx` writes its patch payload to this MSB because it represents the patch as edit-buffer state. |
| `20 00 00 00`..`22 28 00 00` | USER patch slots (`PatchSlot::User { bank: 1..=99, position: 1..=3 }`) | HIGH | Reverse-engineered from FloorBoard `MidiTable::patchRequest`: `address = [0x20 + n, p, 0, 0]` where `n*128 + p = (bank-1)*3 + (position-1)`. Implemented in `gr55-core::address`. |
| `23 00 00 00`+ | PRESET patch slots | HIGH | Same formula with a `+87` index gap between USER and PRESET ranges. |
| `60 00 00 00` | Temporary Buffer / Edit Buffer base (read) | MEDIUM | FloorBoard XML labels it "Temporary Buffer (Bulk)" and "Temporary Buffer (Individual)" — two access modes at the same MSB. Distinct from RE-202's `20 00 00 00` edit-buffer address (which on GR-55 is occupied by user patch 1:1). |

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

Observed patch byte layout (from `default.syx` first DT1 frame at address `18 00 00 00`,
cross-referenced with FloorBoard `midi.xml`'s `<Structure>` section):

| Sub-address | Byte | Meaning | Source |
|---|---|---|---|
| `00` | `00` (Guitar) / `01` (Bass) | Guitar/Bass Mode | `<Structure><LSB value="00"><LSB value="00"><DATA value="00" abbr="Guitar/Bass Mode">` |
| `01`..`10` | 16 bytes of printable ASCII | Patch name (16 characters; `0x20`..`0x7E` allowed) | `<DATA value="01" abbr="Name1">` … `Name16` |
| `11`+ | … | Pedal section, then PCM tones, then COSM, then effects | rest of `<Structure>` |

Implication for the typed `Patch` model: the first struct field is
`mode: Mode { Guitar, Bass }`, followed by `name: PatchName` (a `[u8; 16]`
newtype with printable-ASCII validation), then nested subsystem structs.

The full byte size of one patch is still unknown — to measure: sum payload
bytes across all DT1 frames in `default.syx` between the first `18 00 00 00`
frame and the next non-`18`-prefixed frame.

FloorBoard's `globalVariables.h:58-59`:

```c
const int patchReplySize = 1268;  // bytes in a patch before trimming
const int patchSize      = 1333;  // bytes in a patch after trimming
```

So one patch is roughly **1.3 KB** of data — small enough that `dump --all`
on 297 user patches is ~400 KB total, manageable in one batch.

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

## 11. FloorBoard fixture quirk: bad checksums in saved files

**Refer to:** `SysxIO.cpp:131-145` (load-time validation), `SysxIO.cpp:144`
(`sysxBuffer = correctSysxMsg(sysxBuffer)` — silent correction).

FloorBoard's bundled `system.syx` and `default.syx` contain frames whose
embedded checksum **does not match** the Roland formula applied to the
frame's address + data. FloorBoard silently corrects them on load.

Hand-verified example (`floorboard_system_area.syx`, frame 3): address
`02 00 02 00`, 128 data bytes, declared checksum `0x37`, correct checksum
`0x36`. Off by one in the same direction across multiple frames in that
file.

**Implication for `gr55-core::sysex`:** the strict [`Frame::try_parse`]
rejects these as `BadChecksum` (correct behavior — real device wire data
should always have valid checksums). For FloorBoard fixtures, use the
lenient [`Frame::try_parse_unchecked`] / [`parse_frames_unchecked`], which
returns a [`ChecksumStatus`] alongside each parsed frame. Round-trip tests
verify that **re-encoding** a frame parsed leniently produces a valid
checksum, proving our algorithm is correct.

## 12. Open questions (parking lot)

- [x] What does address MSB `18` represent? **Resolved (HIGH):** FloorBoard's `globalVariables.h:77` `tempDataWrite = "18"` — the live current-patch write area. `default.syx` writes its patch payload here because that's how the device receives a "load this patch into the edit buffer" command.
- [x] What is the `<MPT>` section in `midi.xml`? **Resolved:** MIDI Patch Table — Bank/PC → patch routing matrix.
- [x] Universal Identity Reply bytes. **Resolved (MEDIUM):** family code `53 02`, request `F0 7E 10 06 01 F7`. Software-revision bytes still need real-device confirmation.
- [x] USER and PRESET patch base addresses. **Resolved (HIGH):** `[0x20 + n, p, 0, 0]` where `n*128 + p = (bank-1)*3 + (position-1)`, with a +87 index gap before PRESET. Implemented in `gr55-core::address`.
- [ ] Tempo / BPM encoding.
- [x] Confirm checksum formula matches a real fixture. **Resolved:** formula `(128 − sum%128) % 128` over address+data is correct; verified by hand-computation over the first frame of `system.syx` (sum 0x3A → checksum 0x46 = matches file) and by every `gr55-core` strict round-trip test passing.
- [ ] PC# → patch mapping (does PC#0 select User 01:1 or User 01:1's whole bank-of-3?).
- [ ] Broadcast device-id (`7F`) behavior.
- [ ] Byte size of one full patch dump. **Partially answered:** FloorBoard reports `patchSize = 1333` bytes after trimming (`globalVariables.h:59`).
- [ ] Edit-buffer mirror behavior: does reading `60 ...` reflect the currently-active patch, like RE-202?

## 13. Refuted / dead ends

(none yet)
