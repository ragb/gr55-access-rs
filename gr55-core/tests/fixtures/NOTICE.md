# NOTICE — vendored FloorBoard fixture data

The `.syx` and `.g5l` files in this directory are copied verbatim from the
**GR-55 FloorBoard** distribution (Colin Willcocks et al., GPL-2-or-later).
They are GR-55 SysEx byte streams used by FloorBoard as starting-point
patches and library data; we reuse them as round-trip test fixtures for
`gr55-core`'s codec.

| File                              | FloorBoard source name | Purpose |
|-----------------------------------|------------------------|---------|
| `floorboard_default_patch.syx`    | `default.syx`          | A single default patch as a sequence of DT1 messages. Starts at address MSB `18` — likely the file-format "canonical patch" address (open question: how this maps to live USER / TEMP / PRESET areas). |
| `floorboard_system_area.syx`      | `system.syx`           | Full GR-55 system area dump, starting at address `01 00 00 00`. |
| `floorboard_ez_tone_library.syx`  | `EZ-Tone.syx`          | Multi-patch EZ-Tone preset library (~200 KB of SysEx). |
| `floorboard_default.g5l`          | `default.g5l`          | FloorBoard's own library-file format (wraps multiple patches; not raw SysEx). |

Source snapshot: `gr55floorboard_source_code.zip` dated 2026-05-08, downloaded
from https://sourceforge.net/projects/grfloorboard/.
