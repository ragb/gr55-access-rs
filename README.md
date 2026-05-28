# gr55-access-rs

Rust tooling for the **Roland GR-55 guitar synthesizer** — codec, CLI, and a
WASM-bindgen surface for browser-based editors.

Status: **under construction**. The data model is being mined from the
[GR-55 FloorBoard](https://sourceforge.net/projects/grfloorboard/) project's
address map (`midi.xml`, GPL-2-or-later by Colin Willcocks) and cross-checked
against Roland's GR-55 MIDI Implementation and the
[VController GR-55 driver](https://github.com/sixeight7/VController/blob/master/MIDI_GR55.ino).

## Workspace

- [`gr55-core`](gr55-core/) — pure no-IO codec (SysEx framing, address space,
  typed System / Patch model, YAML serde projection, JSON Schema emitters).
- [`gr55`](gr55/) — CLI: `ports`, `identity`, `dump`, `sync`, `diff`, `show`,
  `lint`, `schema`. Owns `midir`.
- [`gr55-wasm`](gr55-wasm/) — `wasm-bindgen` + `tsify-next` surface for use
  in browser-based editors (the intended end-user form factor, since the
  primary developer does not own a GR-55 — see `docs/sysex-notes.md`).

## Documents

- [`docs/gr55-bootstrap.md`](docs/gr55-bootstrap.md) — original project intent
  and methodology (the bootstrap prompt that started this repo).
- [`docs/sysex-notes.md`](docs/sysex-notes.md) — running log of verified
  protocol facts, with per-claim provenance.
- [`docs/spec/SOURCES.md`](docs/spec/SOURCES.md) — how to re-fetch the local
  research artifacts (FloorBoard source zip, Roland owner's manual).

## License

GPL-3.0-or-later. See [`LICENSE`](LICENSE).

This project incorporates GPL-2-or-later content from FloorBoard
(`gr55-core/data/midi.xml`, `gr55-core/data/midi.xsd`, the `.syx`/`.g5l`
files under `gr55-core/tests/fixtures/`). See `NOTICE.md` in each of those
directories for attribution.
