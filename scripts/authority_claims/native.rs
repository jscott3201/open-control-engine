//! Test-only compiled observations shared by the two numeric fact owners.
//! Python owns the complete index schema; this reader independently refuses
//! malformed JSON, duplicate keys/rows, or missing/malformed native bindings.

use std::collections::BTreeSet;

use serde_json::{Value, json};

const INDEX: &str = include_str!("../../docs/authority-claims.json");
const NATIVE: [(&str, &str, &str); 4] = [
    (
        "catalog-entries",
        "crates/oce-blocks/src/catalog.rs",
        "crates/oce-blocks/src/authority_claims_tests.rs",
    ),
    (
        "catalog-reserved",
        "crates/oce-blocks/src/catalog.rs",
        "crates/oce-blocks/src/authority_claims_tests.rs",
    ),
    (
        "state-format",
        "crates/oce-api/src/state.rs",
        "crates/oce-api/src/tests/authority_claims_tests.rs",
    ),
    (
        "execution-abi",
        "crates/oce-api/src/state.rs",
        "crates/oce-api/src/tests/authority_claims_tests.rs",
    ),
];

#[derive(Debug, PartialEq)]
enum Error {
    Json,
    DuplicateKey,
    Rows,
    Binding,
    Number,
    Mismatch,
}

// JSON is already syntax-validated by serde_json. Inspect object-key tokens
// without collapsing duplicate keys into a Value. Escaped spellings decode to
// the same key; strings containing braces are skipped as complete tokens.
fn unique_keys(raw: &str) -> Result<(), Error> {
    let bytes = raw.as_bytes();
    let mut objects: Vec<BTreeSet<String>> = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'{' => objects.push(BTreeSet::new()),
            b'}' => {
                objects.pop();
            }
            b'"' => {
                let start = at;
                at += 1;
                while bytes[at] != b'"' {
                    if bytes[at] == b'\\' {
                        at += 1;
                    }
                    at += 1;
                }
                let mut next = at + 1;
                while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                if bytes.get(next) == Some(&b':') {
                    let key = serde_json::from_str(&raw[start..=at]).map_err(|_| Error::Json)?;
                    if !objects.last_mut().ok_or(Error::Json)?.insert(key) {
                        return Err(Error::DuplicateKey);
                    }
                }
            }
            _ => {}
        }
        at += 1;
    }
    Ok(())
}

fn compare(raw: &str, observed: &[(&str, u32)]) -> Result<(), Error> {
    let index: Value = serde_json::from_str(raw).map_err(|_| Error::Json)?;
    unique_keys(raw)?;
    if index["schema"] != "oce-authority-claims/v1" {
        return Err(Error::Binding);
    }
    let facts = index["facts"].as_array().ok_or(Error::Rows)?;
    let mut seen = BTreeSet::new();
    for row in facts {
        let id = row["id"].as_str().ok_or(Error::Rows)?;
        if !seen.insert(id) {
            return Err(Error::Rows);
        }
        if row["mode"] == "native" && !NATIVE.iter().any(|binding| binding.0 == id) {
            return Err(Error::Binding);
        }
    }
    for (id, source, verifier) in NATIVE {
        let row = facts
            .iter()
            .find(|row| row["id"] == id)
            .ok_or(Error::Rows)?;
        let object = row.as_object().ok_or(Error::Rows)?;
        let keys: BTreeSet<_> = object.keys().map(String::as_str).collect();
        if keys != BTreeSet::from(["id", "mode", "source", "verifier", "projection", "expected"])
            || row["mode"] != "native"
            || row["source"] != source
            || row["verifier"] != verifier
            || row["projection"] != "summary"
        {
            return Err(Error::Binding);
        }
        let expected = row["expected"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(Error::Number)?;
        if let Some((_, actual)) = observed.iter().find(|(name, _)| *name == id)
            && expected != *actual
        {
            return Err(Error::Mismatch);
        }
    }
    if observed.is_empty()
        || observed
            .iter()
            .any(|(id, _)| !NATIVE.iter().any(|b| b.0 == *id))
    {
        return Err(Error::Binding);
    }
    Ok(())
}

/// Run positive and hostile controls through the SAME native comparison entrypoint.
/// The caller supplies compiled values, never duplicate numeric literals.
pub(super) fn verify_with_controls(observed: &[(&str, u32)]) {
    assert_eq!(compare(INDEX, observed), Ok(()));
    assert_eq!(compare(INDEX, observed), Ok(()));
    let index: Value = serde_json::from_str(INDEX).unwrap();
    assert_eq!(compare("{", observed), Err(Error::Json));
    assert_eq!(compare("{}", observed), Err(Error::Binding));
    assert_eq!(compare(INDEX, &[]), Err(Error::Binding));
    for (id, _, _) in NATIVE {
        let position = index["facts"]
            .as_array()
            .unwrap()
            .iter()
            .position(|r| r["id"] == id)
            .unwrap();
        let mut missing = index.clone();
        missing["facts"].as_array_mut().unwrap().remove(position);
        assert_eq!(compare(&missing.to_string(), observed), Err(Error::Rows));
        let mut duplicate = index.clone();
        duplicate["facts"]
            .as_array_mut()
            .unwrap()
            .push(index["facts"][position].clone());
        assert_eq!(compare(&duplicate.to_string(), observed), Err(Error::Rows));
        for field in ["id", "mode", "source", "verifier", "projection", "expected"] {
            let mut missing = index.clone();
            missing["facts"][position]
                .as_object_mut()
                .unwrap()
                .remove(field);
            let expected = if field == "id" {
                Error::Rows
            } else {
                Error::Binding
            };
            assert_eq!(compare(&missing.to_string(), observed), Err(expected));
        }
        for value in [
            json!(true),
            json!(-1),
            json!(1.0),
            json!(4294967296_u64),
            Value::Null,
            json!("1"),
        ] {
            let mut malformed = index.clone();
            malformed["facts"][position]["expected"] = value;
            assert_eq!(
                compare(&malformed.to_string(), observed),
                Err(Error::Number)
            );
        }
        for field in ["mode", "source", "verifier", "projection"] {
            let mut redirected = index.clone();
            redirected["facts"][position][field] = json!("unrelated");
            assert_eq!(
                compare(&redirected.to_string(), observed),
                Err(Error::Binding)
            );
        }
        if let Some((_, actual)) = observed.iter().find(|(name, _)| *name == id) {
            let mut wrong = index.clone();
            wrong["facts"][position]["expected"] = json!(actual.wrapping_add(1));
            // An allow-all/disabled comparison would make this assertion fail.
            assert_eq!(compare(&wrong.to_string(), observed), Err(Error::Mismatch));
        }
    }
    for raw in [
        INDEX.replacen("\"schema\":", "\"schema\": null, \"schema\":", 1),
        INDEX.replacen("\"expected\":", "\"expected\": 999, \"expec\\u0074ed\":", 1),
    ] {
        assert_eq!(compare(&raw, observed), Err(Error::DuplicateKey));
    }
    // Reformatting/reordering is not a bypass or an artificial rejection.
    let mut reordered = index;
    reordered["facts"].as_array_mut().unwrap().reverse();
    assert_eq!(compare(&reordered.to_string(), observed), Ok(()));
}
