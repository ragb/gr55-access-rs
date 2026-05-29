# NOTICE — vendored FloorBoard fixture data

The `.syx` and `.g5l` files in this directory are copied verbatim from the
**GR-55 FloorBoard** distribution (Colin Willcocks et al., GPL-2-or-later).
They are GR-55 SysEx byte streams used by FloorBoard as starting-point
patches and library data; we reuse them as round-trip test fixtures for
`gr55-core`'s codec.

| File                              | FloorBoard source name | Purpose                                                                       |
|-----------------------------------|------------------------|-------------------------------------------------------------------------------|
| `floorboard_default_patch.syx`    | `default.syx`          | Single default patch (DT1 stream) at MSB `0x18` — live TEMP RAM.              |
| `floorboard_system_area.syx`      | `system.syx`           | Full GR-55 system area dump, starting at address `01 00 00 00`.               |
| `floorboard_ez_tone_library.syx`  | `EZ-Tone.syx`          | Multi-patch EZ-Tone preset library (~200 KB of SysEx).                        |
| `floorboard_default.g5l`          | `default.g5l`          | FloorBoard's own library-file format (wraps multiple patches; not raw SysEx). |

## Patch address space

MSB `0x18` targets the GR-55's **live current-patch (TEMP) RAM** —
writing a DT1 there takes effect immediately, no USER-slot store
required. Confirmed against VController's hardware-tested
[`MD_GR55.ino`](https://github.com/sixeight7/VController_v3/blob/master/Firmware/VController_v3/MD_GR55.ino):
`GR55_CTL_ADDRESS 0x18000011`, `GR55_PCM1_SW 0x18002003`,
`GR55_COSM_GUITAR_SW 0x1800100A`, and `GR55_TEMPO 0x1800023C` all
match this crate's typed-field offsets exactly.

**USER patch storage** lives at MSB `0x20`. Slot `N` base address:

```text
0x20000001 + ((N / 0x80) * 0x01000000) + ((N % 0x80) * 0x00010000)
```

Patch *recall* (vs. write) goes via standard MIDI Program Change + CC#0
bank select, not DT1 — VController uses
`MIDI_send_CC(0, patch >> 7); MIDI_send_PC(patch & 0x7F);`.

Source snapshot: `gr55floorboard_source_code.zip` dated 2026-05-08, downloaded
from https://sourceforge.net/projects/grfloorboard/.
