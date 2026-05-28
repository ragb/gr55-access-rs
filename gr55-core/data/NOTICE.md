# NOTICE — vendored FloorBoard data

This directory contains data files derived from the **GR-55 FloorBoard** project
by Colin Willcocks (and Uco Mesdag for the original `fxfloorboard` codebase
it forks from).

- Source project: https://sourceforge.net/projects/grfloorboard/
- Source mirror (older): https://github.com/motiz88/GR-55Floorboard
- Snapshot used: `gr55floorboard_source_code.zip` dated **2026-05-08**
  (downloaded from the SourceForge project on 2026-05-28).
- License: GNU General Public License, version 2 or (at your option) any later
  version — compatible with this project's GPL-3.0-or-later choice.

## Files

| File         | FloorBoard source path | Notes |
|--------------|------------------------|-------|
| `midi.xml`   | `midi.xml` (UTF-16)    | Re-encoded UTF-8 (no semantic changes) for diffability and grep ergonomics. The canonical UTF-16 form is preserved in `docs/spec/floorboard_src/gr55floorboard_source/midi.xml` for cross-reference. |
| `midi.xsd`   | `midi.xsd`             | Verbatim copy, defines the schema for `midi.xml`. |

## Copyright

```
Copyright (C) 2007–2025 Colin Willcocks.
Copyright (C) 2005–2007 Uco Mesdag.
```

The full GPL-2-or-later license text is reproduced in the FloorBoard
distribution and is compatible with the GPL-3.0-or-later under which
`gr55-access-rs` is distributed (see top-level `LICENSE`).
