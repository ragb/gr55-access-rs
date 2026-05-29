# gr55-access-rs

Rust tooling for the **Roland GR-55 guitar synthesizer** — codec, CLI, and a
WASM-bindgen surface for browser-based editors.

Status: **under construction**. The protocol model was originally explored by
the [GR-55 FloorBoard](https://sourceforge.net/projects/grfloorboard/) project;
the static address tables here are an independent Rust expression of those
facts (see [`THIRDPARTY.md`](THIRDPARTY.md)). Cross-checked against Roland's
official GR-55 Owner's Manual and the
[VController GR-55 driver](https://github.com/sixeight7/VController/blob/master/MIDI_GR55.ino).

## Workspace

- [`gr55-core`](gr55-core/) — pure no-IO codec (SysEx framing, address space,
  typed System / Patch model, YAML serde projection, JSON Schema emitters).
- [`gr55`](gr55/) — CLI: `ports`, `identity`, `dump`, `sync`, `diff`, `show`,
  `lint`, `schema`. Owns `midir`.
- [`gr55-wasm`](gr55-wasm/) — `wasm-bindgen` + `tsify-next` surface for use
  in browser-based editors.

## Setup

Test fixtures live in a git submodule (`external/floorboard`). Clone with:

```bash
git clone --recursive https://github.com/ragb/gr55-access-rs
# or, in an existing checkout:
git submodule update --init --recursive
```

`cargo build` and `cargo test` work out of the box once the submodule is
checked out.

## Documents

- [`docs/gr55-bootstrap.md`](docs/gr55-bootstrap.md) — original project intent
  and methodology (the bootstrap prompt that started this repo).
- [`docs/sysex-notes.md`](docs/sysex-notes.md) — running log of verified
  protocol facts, with per-claim provenance.
- [`docs/spec/SOURCES.md`](docs/spec/SOURCES.md) — research artifacts kept
  locally (Roland owner's manual, FloorBoard reference snapshot).

## License

MIT. See [`LICENSE`](LICENSE).

The static parameter tables under [`gr55-core/src/generated/`](gr55-core/src/generated/)
contain factual data about Roland's GR-55 MIDI protocol — addresses,
parameter names, value enumerations. Such factual data is not subject to
copyright in the United States (*Feist v. Rural Telephone*, 1991).
Attribution and provenance — including credit to FloorBoard and the legal
position taken — is documented in [`THIRDPARTY.md`](THIRDPARTY.md).
