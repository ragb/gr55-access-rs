# NOTICE — test fixture loading

This directory used to vendor a handful of FloorBoard `.syx`/`.g5l`
fixtures verbatim. They were removed in favor of loading the same
files at runtime from the `external/floorboard` git submodule (a
mirror of <https://github.com/motiz88/GR-55Floorboard>). The compiled
`gr55-core` artifact carries no verbatim FloorBoard data.

Tests resolve fixture paths via [`crate::test_support`]:

```rust
let bytes = crate::test_support::fb_fixture_required("default.syx");
```

The helper joins the relative path against `<workspace>/external/floorboard`.
If the submodule isn't checked out, `fb_fixture_required` panics with
a clear "run `git submodule update --init`" diagnostic.

## Fixture map (submodule-relative paths)

| Test usage                              | Submodule path                                        |
|-----------------------------------------|-------------------------------------------------------|
| Default-patch round-trip + audit        | `default.syx`                                         |
| EZ-Tone library per-slot round-trip     | `EZ-Tone.syx`                                         |
| System-area decoder smoke               | `system.syx`                                          |
| `.g5l` parser default-patch test        | `default.g5l`                                         |
| `.g5l` corpus round-trip (~297 patches) | `packager/saved_patches/**/*.g5l`                     |

## Patch address space (kept for cross-reference)

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
