//! Repository-side conformance check: every `conformance/grid/` fixture must be
//! well-formed against the `gridwire` wire types.
//!
//! This crate has no VT engine (serde + bitflags only, per the dependency rule),
//! so it cannot *run* a projection — that is the producers' job (a session backend's
//! `Term` projection, a frontend's local PTY projection, each in its own tree).
//! What it owns is the corpus as the contract: here we assert every input parses
//! as `{dims, feed}` and every expected output parses as a `GridDelta`, and that
//! the two files are paired. Producer agreement is asserted where the producers
//! live.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use gridwire::{GridDelta, GridDims};

/// A corpus input is either single-drain (`feed` → one expected `GridDelta`) or
/// multi-drain (`steps` → drain after each chunk, expected an array of the same
/// length). Exactly one of `feed`/`steps` is present.
#[derive(Deserialize)]
struct CorpusInput {
    #[allow(dead_code)]
    dims: GridDims,
    #[serde(default)]
    feed: Option<String>,
    #[serde(default)]
    steps: Option<Vec<String>>,
}

fn corpus_dir() -> PathBuf {
    // <repo>/conformance/grid, beside the crate root
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/grid")
}

#[test]
fn every_corpus_fixture_is_well_formed_and_paired() {
    let dir = corpus_dir();
    let mut cases = 0;
    for entry in std::fs::read_dir(&dir).expect("read conformance/grid") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if name.contains(".expect.") {
            continue;
        }
        let stem = &name[..name.len() - ".json".len()];

        // Input parses as {dims, feed|steps} with exactly one of feed/steps.
        let input = std::fs::read_to_string(&path).expect("read input");
        let parsed: CorpusInput =
            serde_json::from_str(&input).unwrap_or_else(|e| panic!("input {name} malformed: {e}"));
        assert!(
            parsed.feed.is_some() ^ parsed.steps.is_some(),
            "input {name} must have exactly one of `feed` or `steps`"
        );

        let expect_path = dir.join(format!("{stem}.expect.json"));
        let expect = std::fs::read_to_string(&expect_path)
            .unwrap_or_else(|_| panic!("case '{stem}' has no .expect.json pair"));

        match (&parsed.feed, &parsed.steps) {
            // Single drain: expected is one GridDelta.
            (Some(_), None) => {
                let _: GridDelta = serde_json::from_str(&expect).unwrap_or_else(|e| {
                    panic!("expected {stem}.expect.json is not a GridDelta: {e}")
                });
            }
            // Multi drain: expected is an array of GridDeltas, one per step.
            (None, Some(steps)) => {
                let deltas: Vec<GridDelta> = serde_json::from_str(&expect).unwrap_or_else(|e| {
                    panic!("expected {stem}.expect.json is not a [GridDelta]: {e}")
                });
                assert_eq!(
                    deltas.len(),
                    steps.len(),
                    "case '{stem}': {} steps but {} expected deltas",
                    steps.len(),
                    deltas.len()
                );
            }
            _ => unreachable!("xor asserted above"),
        }

        cases += 1;
    }
    assert!(cases >= 3, "expected >= 3 corpus cases, found {cases}");
}
