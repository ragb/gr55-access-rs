# Spec sources (local research artifacts)

The `gr55-core` crate ships no verbatim FloorBoard content. The two
external information sources we cross-reference during development are:

## FloorBoard mirror — `external/floorboard` git submodule

Pointer at <https://github.com/motiz88/GR-55Floorboard> (a community
GitHub mirror of FloorBoard's SourceForge distribution by Colin
Willcocks et al., GPL-2-or-later). Fetched via

```bash
git submodule update --init --recursive
```

Provides the test fixtures the codec tests load at runtime (root-level
`default.syx`, `EZ-Tone.syx`, `system.syx`, `default.g5l`) and the
~297-patch `.g5l` corpus under `packager/saved_patches/`. The submodule
is referenced, not vendored; cloning without `--recursive` skips the
fetch and the FB-dependent tests will fail with a clear "run
`git submodule update --init`" diagnostic.

## Roland's GR-55 Owner's Manual — kept locally, not committed

`docs/spec/roland/GR-55_OM.pdf` (29 MB) — official Roland publication;
their copyright. Used to cross-check parameter names, value ranges, and
the user-facing parameter list against the tables under
`gr55-core/src/generated/`. Re-fetch with:

```bash
mkdir -p docs/spec/roland
curl -sSL -o docs/spec/roland/GR-55_OM.pdf \
  https://cdn.roland.com/assets/media/pdf/GR-55_OM.pdf
```

Gitignored under `docs/spec/.gitignore`.

## Why nothing FloorBoard is vendored

The crate was scrubbed of vendored FloorBoard content in two phases:

1. **`2ff4c44`** — promoted the build-time-generated parameter tables to
   committed Rust source under `gr55-core/src/generated/` and removed
   `gr55-core/data/midi.xml`. The byte addresses, parameter names, and
   value enumerations are facts about Roland's MIDI protocol and not
   subject to copyright in the United States (Feist v. Rural Telephone,
   1991).
2. **The `external/floorboard` submodule + the follow-up commit** —
   moved the test fixtures (`.syx`, `.g5l`) and the bulky FloorBoard
   source tree out of the repo. Tests load fixtures at runtime via
   `crate::test_support::fb_fixture_required(...)` instead of
   `include_bytes!`, so the compiled `gr55-core` artifact contains no
   verbatim FB bytes either.
