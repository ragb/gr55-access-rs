//! Test-only helpers for loading FloorBoard fixture files from the
//! `external/floorboard` git submodule.
//!
//! The submodule points at <https://github.com/motiz88/GR-55Floorboard>
//! and is checked out at `<workspace>/external/floorboard`. Tests load
//! fixture bytes at runtime via [`fb_fixture`] / [`fb_fixture_required`]
//! rather than baking them into the compiled binary with
//! `include_bytes!` — this keeps the `gr55-core` crate artifact free of
//! verbatim FloorBoard data.
//!
//! If the submodule isn't checked out, [`fb_fixture_required`] panics
//! with a clear message telling the developer to run
//! `git submodule update --init`. CI must check out submodules
//! recursively for the FB-dependent tests to pass.

use std::path::PathBuf;

/// Path to the FloorBoard submodule root, e.g.
/// `<workspace>/external/floorboard`.
pub fn floorboard_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("external")
        .join("floorboard")
}

/// Load a FloorBoard fixture by repo-relative path. Returns `None` if
/// the submodule isn't checked out — useful for tests that should skip
/// quietly when fixtures aren't available.
#[allow(dead_code)]
pub fn fb_fixture(rel: &str) -> Option<Vec<u8>> {
    let path = floorboard_root().join(rel);
    std::fs::read(&path).ok()
}

/// Like [`fb_fixture`] but panics if the file is missing. Use this when
/// the test absolutely depends on the fixture (most do).
pub fn fb_fixture_required(rel: &str) -> Vec<u8> {
    let path = floorboard_root().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "missing FloorBoard fixture {}: {e}\n\
             The fixtures live in the `external/floorboard` git submodule.\n\
             Run `git submodule update --init --recursive` from the workspace root.",
            path.display(),
        )
    })
}
