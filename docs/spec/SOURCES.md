# Spec sources (local research artifacts)

These files are kept locally under `docs/spec/` for cross-reference during
development. They are **not** committed to the repository (see `.gitignore`)
because:

- `gr55_owners_manual.pdf` is Roland's copyright; we don't redistribute it.
- `gr55floorboard_source_code.zip` (and its extraction `floorboard_src/`) is
  ~64 MB / ~78 MB unpacked — only a tiny subset (`midi.xml`, `midi.xsd`,
  a handful of `.syx` fixtures) is needed at build time. Those are vendored
  under `gr55-core/data/` and `gr55-core/tests/fixtures/` with explicit
  attribution (see `NOTICE.md` in each of those directories).

## How to re-fetch

```bash
mkdir -p docs/spec
# Roland GR-55 Owner's Manual (28 MB)
curl -sSL -o docs/spec/gr55_owners_manual.pdf \
  https://archive.org/download/manuallib-id-2725234/2725234.pdf
# FloorBoard canonical source zip (~64 MB)
curl -sSL -o docs/spec/gr55floorboard_source_code.zip \
  "https://sourceforge.net/projects/grfloorboard/files/GR-55/gr55floorboard_source_code.zip/download"
( cd docs/spec && mkdir -p floorboard_src && cd floorboard_src && \
  unzip -q ../gr55floorboard_source_code.zip )
# Convert midi.xml from UTF-16 to UTF-8 for grep ergonomics
iconv -f UTF-16 -t UTF-8 \
  docs/spec/floorboard_src/gr55floorboard_source/midi.xml \
  > docs/spec/floorboard_src/gr55floorboard_source/midi.utf8.xml
```

## Snapshot manifest

| File                                  | Date       | Size    | Source URL |
|---------------------------------------|------------|---------|------------|
| `gr55_owners_manual.pdf`              | downloaded 2026-05-28 | ~28 MB | https://archive.org/details/manuallib-id-2725234 |
| `gr55floorboard_source_code.zip`      | 2026-05-08 | ~64 MB  | https://sourceforge.net/projects/grfloorboard/files/GR-55/gr55floorboard_source_code.zip/download |

When the FloorBoard source is updated, re-download, re-extract, re-convert,
diff `midi.utf8.xml` against `gr55-core/data/midi.xml`, and refresh the
vendored copy if the address map has changed.
