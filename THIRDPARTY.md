# Third-party attribution

This project is licensed under the MIT License (see [`LICENSE`](LICENSE)).
This file records gratitude and provenance for external work that informed
the codebase, including a note on the legal position taken with respect to
factual data extracted from copylefted sources.

## GR-55 FloorBoard — original protocol exploration

The Roland GR-55's SysEx address map was originally reverse-engineered and
compiled by the **GR-55 FloorBoard** project, by **Colin Willcocks** and
**Uco Mesdag** (forking from earlier `fxfloorboard` work). FloorBoard is
distributed under GPL-2-or-later from
<https://sourceforge.net/projects/grfloorboard/>; a community GitHub mirror
is at <https://github.com/motiz88/GR-55Floorboard>.

This project gratefully acknowledges that work. **None of FloorBoard's
source code, XML data files, or other expressive material is bundled here.**
What this repository ships under MIT is:

- Static Rust tables under [`gr55-core/src/generated/`](gr55-core/src/generated/)
  containing **factual data** about Roland's GR-55 MIDI protocol — byte
  addresses, parameter names (Roland's own), and value enumerations.
- A hand-written [`gr55-core/src/pcm_tail_params.rs`](gr55-core/src/pcm_tail_params.rs)
  containing the same kind of factual data for the PCM tone tail page.

### Legal position

Factual data about a published machine protocol — what byte at what address
means what — is not subject to copyright in the United States. The U.S.
Supreme Court held in *Feist Publications, Inc. v. Rural Telephone Service
Co.*, 499 U.S. 340 (1991), that facts cannot be copyrighted no matter how
much effort went into discovering them, and that the "sweat of the brow"
doctrine has no place in U.S. copyright law. Section 102(b) of the
Copyright Act independently excludes "any idea, procedure, process, system,
method of operation, concept, principle, or discovery" from copyright
protection.

The byte addresses, parameter names, and value enumerations Roland's GR-55
firmware uses are such facts. They were extracted from FloorBoard's `midi.xml`
into an independent Rust expression that contains none of FloorBoard's XML
schema, comment text, or other creative authorship. The Rust code itself
(struct definitions, table layout, codec logic, parsers) is the present
project's authorship.

This position is U.S.-jurisdiction-specific. Users in jurisdictions with
*sui generis* database rights (e.g. the EU under Directive 96/9/EC) should
form their own legal view.

### What FloorBoard makes possible

The `external/floorboard` git submodule in this repository references the
motiz88 GitHub mirror of FloorBoard's distribution. The submodule is a
**reference**, not bundled content — it appears in the repo only as a URL +
commit hash. When checked out via `git submodule update --init`, it
provides a corpus of GR-55 `.syx` and `.g5l` test fixtures used by this
project's tests. The submodule's contents remain under FloorBoard's
GPL-2-or-later. Compiled artifacts of this project do not embed any of
those fixtures.

## VController — hardware-tested driver

[`VController`](https://github.com/sixeight7/VController) by **sixeight7** is
an open-source MIDI foot controller project whose `MD_GR55.ino` driver
file was used to cross-check protocol claims (in particular the live-area
MSB `0x18`, the USER patch slot encoding at MSB `0x20`, and ~46 parameter
addresses). No VController code or data is incorporated; the project is
referenced for its hardware-tested verification value.

## Roland GR-55 Owner's Manual

Roland Corporation's **GR-55 Owner's Manual** (29 MB PDF, 96 pages) is the
authoritative source for Roland's official parameter names, value ranges,
and the user-facing parameter list. Roland holds copyright on the manual;
this project does not redistribute it. The crate's `data/help.toml` and the
`pcm_tail_params.rs` parameter names follow Roland's published naming
verbatim (factual data, per the position stated above).
