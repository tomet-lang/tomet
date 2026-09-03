//! Checks the pinned `pandoc-api-version` against the installed Pandoc.
//!
//! Pandoc refuses JSON whose API version disagrees with its own, so the
//! constant this crate writes has to follow Pandoc releases. Nothing else
//! in the test suite can catch that drift: it depends on a binary that is
//! not part of this workspace.
//!
//! The test is therefore *conditional*. Where `pandoc` is not installed
//! -- the dev container included -- it reports that and passes, rather
//! than failing a suite for a tool the repository does not depend on.

use std::process::{Command, Stdio};

use tomet_pandoc::PANDOC_API_VERSION;

#[test]
fn the_pinned_api_version_matches_the_installed_pandoc() {
    let Some(actual) = installed_api_version() else {
        eprintln!(
            "skipped: no usable `pandoc` on PATH. \
             The pinned API version is {PANDOC_API_VERSION:?}; run this \
             where pandoc is installed to check it."
        );
        return;
    };

    // Pandoc compares the major and minor components; a patch bump is
    // compatible.
    assert_eq!(
        (actual[0], actual[1]),
        (PANDOC_API_VERSION[0], PANDOC_API_VERSION[1]),
        "the installed pandoc speaks API {actual:?} but this crate writes \
         {PANDOC_API_VERSION:?}. Update `PANDOC_API_VERSION` in \
         `crates/tomet-convert-pandoc/src/ast.rs`."
    );
}

/// Asks the installed Pandoc what API version it speaks, by having it
/// convert a trivial document and reading the field back.
fn installed_api_version() -> Option<Vec<u32>> {
    let out = Command::new("pandoc")
        .args(["-f", "markdown", "-t", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take()?.write_all(b"x\n").ok()?;
            child.wait_with_output().ok()
        })?;
    if !out.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let version = json.get("pandoc-api-version")?.as_array()?;
    let parsed: Vec<u32> = version
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .collect();
    (parsed.len() >= 2).then_some(parsed)
}
