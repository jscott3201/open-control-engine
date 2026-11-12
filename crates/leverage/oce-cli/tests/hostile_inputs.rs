//! Hostile inputs through the built binary: binary junk, bad UTF-8, an over-size file refused
//! from its metadata before a byte is read, an over-size stdin cut off one byte past the bound,
//! a directory, a missing path, a wire object written as the list of its values (GitHub #544).
//! Every one is exit 2 with one typed error code, a well-formed JSON report on stdout, nothing on
//! stderr and no panic.

mod support;

use std::fs::File;

use oce_author::{MAX_TEXT_BYTES, MAX_WIRE_BYTES};
use support::{Run, crate_dir, oce_in, scratch};

/// The single error code of a refused run, after checking the run's shape.
fn refusal(run: &Run) -> String {
    assert_eq!(run.code, 2, "{}", run.transcript());
    assert!(run.stderr.is_empty(), "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["ok"], false);
    assert_eq!(report["findings"], serde_json::json!([]));
    let errors = report["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 1, "{}", run.transcript());
    errors[0]["code"].as_str().expect("code").to_owned()
}

#[test]
fn binary_junk_is_refused_in_both_forms() {
    let dir = scratch("junk");
    let junk: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    std::fs::write(dir.join("junk.oce"), &junk).expect("write");
    std::fs::write(dir.join("junk.json"), &junk).expect("write");
    for command in ["validate", "emit", "fmt"] {
        let text = refusal(&oce_in(&dir, &[command, "junk.oce"], b""));
        assert!(oce_author::PARSE_CODES.contains(&text.as_str()), "{text}");
        let wire = refusal(&oce_in(&dir, &[command, "junk.json"], b""));
        assert!(!wire.is_empty());
    }
}

#[test]
fn bad_utf8_is_refused_as_not_utf8() {
    let dir = scratch("bad_utf8");
    let bytes = b"sequence s\nderived_from none\nparam x : Real = \"\xff\"\n";
    std::fs::write(dir.join("bad.oce"), bytes).expect("write");
    std::fs::write(dir.join("bad.json"), b"{\"sequence\": \"\xc3\x28\"}").expect("write");
    for command in ["validate", "emit", "fmt"] {
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "bad.oce"], b"")),
            "not-utf8"
        );
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "bad.json"], b"")),
            "not-utf8"
        );
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "-", "--from", "text"], bytes)),
            "not-utf8"
        );
    }
}

#[test]
fn a_hundred_megabyte_file_is_refused_before_it_is_read() {
    let dir = scratch("huge");
    for name in ["huge.oce", "huge.json"] {
        // A sparse file: 100 MB by its metadata, no data written.
        File::create(dir.join(name))
            .and_then(|file| file.set_len(100 * 1024 * 1024))
            .expect("create a sparse file");
        for command in ["validate", "emit", "fmt"] {
            let run = oce_in(&dir, &[command, name], b"");
            assert_eq!(refusal(&run), "input-too-large");
            // The metadata path names the size; the bounded-read path cannot know it.
            let detail = run.report()["errors"][0]["detail"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(
                detail,
                "the input is 104857600 bytes; at most 4194304 are read"
            );
        }
    }
}

#[test]
fn a_file_at_the_bound_is_read_and_one_byte_over_is_refused() {
    let dir = scratch("bound");
    assert_eq!(MAX_TEXT_BYTES, MAX_WIRE_BYTES);
    // A parseable document of exactly the bound: comment lines of 8,000 bytes, then one shorter.
    let mut at = b"sequence s\nderived_from none\n".to_vec();
    while at.len() < MAX_TEXT_BYTES {
        let room = (MAX_TEXT_BYTES - at.len()).min(8_000);
        at.push(b'#');
        at.extend(std::iter::repeat_n(b'x', room.saturating_sub(2)));
        at.push(b'\n');
        at.truncate(MAX_TEXT_BYTES);
    }
    assert_eq!(at.len(), MAX_TEXT_BYTES);
    std::fs::write(dir.join("at.oce"), &at).expect("write");
    let mut over = at.clone();
    over.push(b'\n');
    std::fs::write(dir.join("over.oce"), &over).expect("write");
    // Read and parsed: stage 1 then judges it (no blocks), so the answer is no, not an error.
    let run = oce_in(&dir, &["validate", "--author-side", "at.oce"], b"");
    assert_eq!(run.code, 1, "{}", run.transcript());
    assert_eq!(run.report()["findings"][0]["code"], "empty-sequence");
    assert_eq!(
        refusal(&oce_in(&dir, &["fmt", "--check", "over.oce"], b"")),
        "input-too-large"
    );
}

#[test]
fn an_over_size_stdin_is_cut_off_one_byte_past_the_bound() {
    let dir = scratch("stdin_over");
    for form in ["text", "wire"] {
        let over = vec![b' '; MAX_TEXT_BYTES + 1];
        let run = oce_in(&dir, &["validate", "-", "--from", form], &over);
        assert_eq!(refusal(&run), "input-too-large");
        assert_eq!(
            run.report()["errors"][0]["detail"],
            "the input is larger than 4194304 bytes"
        );
    }
}

#[test]
fn a_directory_and_a_missing_path_are_refused() {
    let dir = scratch("paths");
    std::fs::create_dir_all(dir.join("folder.oce")).expect("mkdir");
    for command in ["validate", "emit", "fmt"] {
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "folder.oce"], b"")),
            "not-a-file"
        );
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "missing.json"], b"")),
            "not-found"
        );
    }
}

#[test]
fn stdin_without_from_is_refused_for_every_reading_command() {
    let dir = scratch("stdin_from");
    for command in ["validate", "emit", "fmt"] {
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "-"], b"sequence s\n")),
            "missing-from"
        );
    }
}

#[test]
fn an_empty_input_is_a_parse_refusal_not_a_panic() {
    let dir = scratch("empty");
    std::fs::write(dir.join("empty.oce"), b"").expect("write");
    std::fs::write(dir.join("empty.json"), b"").expect("write");
    assert_eq!(
        refusal(&oce_in(&dir, &["validate", "empty.oce"], b"")),
        "missing-sequence"
    );
    assert_eq!(
        refusal(&oce_in(&dir, &["validate", "empty.json"], b"")),
        "json-syntax"
    );
}

#[test]
fn a_document_in_the_earlier_extends_spelling_is_refused_by_every_reading_command() {
    // Issue #529: the provenance statement and key are `derived_from`; `extends` keeps its CDL
    // meaning and the format has no reader for the earlier spelling, in either form.
    let dir = scratch("earlier_extends");
    let body = "in u : Real\nout y : Real\nblock abs = CDL.Reals.Abs\nu -> abs.u\nabs.y -> y\n";
    let current = format!("sequence s\nderived_from none\n{body}");
    std::fs::write(dir.join("current.oce"), &current).expect("write");
    std::fs::write(
        dir.join("earlier.oce"),
        format!("sequence s\nextends none\n{body}"),
    )
    .expect("write");
    let wire = oce_author::to_wire_string(&oce_author::parse_text(&current).expect("parses"));
    let earlier = wire.replacen("\"derived_from\":", "\"extends\":", 1);
    assert_ne!(earlier, wire);
    std::fs::write(dir.join("current.json"), &wire).expect("write");
    std::fs::write(dir.join("earlier.json"), earlier).expect("write");
    for command in ["validate", "emit", "fmt"] {
        for current in ["current.oce", "current.json"] {
            let run = oce_in(&dir, &[command, current], b"");
            assert_eq!(run.code, 0, "{}", run.transcript());
        }
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "earlier.oce"], b"")),
            "missing-derived-from"
        );
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "earlier.json"], b"")),
            "unknown-key"
        );
    }
}

#[test]
fn an_object_spelled_another_way_is_refused_by_every_reading_command_at_its_path() {
    // GitHub #544: the wire reader read a list positionally as the object it abbreviates, and a
    // signal type's map form `{"Real": null}` as the string. Each of the econ_enable bundle's
    // mutants below loaded before the fix; `validate` answered `"ok": true` for the first.
    let dir = scratch("closed_objects");
    let bundle = std::fs::read_to_string(
        crate_dir().join("../oce-author/tests/fixtures/wire/econ_enable.json"),
    )
    .expect("the econ_enable bundle");
    let semantic = "{\n        \"name\": \"\",\n        \"label\": \"\",\n        \"unit\": \"K\",\n        \
                    \"brick\": \"\",\n        \"s223\": \"\",\n        \"qudt\": \"\",\n        \
                    \"haystack\": [],\n        \"provisional\": false\n      }";
    assert!(
        bundle.contains(semantic),
        "the first boundary's semantic slot"
    );
    let cases = [
        (
            "semantic.json",
            bundle.replacen(semantic, r#"["", "", "K", "", "", "", [], false]"#, 1),
            "inputs[0].semantic",
            19,
        ),
        (
            "type.json",
            bundle.replacen(
                "\"type\": \"Real\",\n      \"quantity\"",
                "\"type\": {\"Real\": null},\n      \"quantity\"",
                1,
            ),
            "inputs[0].type",
            14,
        ),
        (
            "root.json",
            r#"["econ_enable", "", [], [], [], [], []]"#.to_owned(),
            "document",
            1,
        ),
    ];
    for (name, document, path, line) in &cases {
        assert_ne!(document, &bundle, "{name} changed the bundle");
        std::fs::write(dir.join(name), document).expect("write");
        let runs = [
            oce_in(&dir, &["validate", name], b""),
            oce_in(&dir, &["validate", "--author-side", name], b""),
            oce_in(&dir, &["emit", name], b""),
            oce_in(&dir, &["fmt", name], b""),
            oce_in(&dir, &["fmt", "--check", name], b""),
            oce_in(
                &dir,
                &["validate", "-", "--from", "wire"],
                document.as_bytes(),
            ),
            oce_in(&dir, &["emit", "-", "--from", "wire"], document.as_bytes()),
        ];
        for run in &runs {
            assert_eq!(refusal(run), "wrong-type", "{name}: {}", run.transcript());
            let error = &run.report()["errors"][0];
            let detail = error["detail"].as_str().expect("detail");
            assert!(
                detail.contains(&format!(" at {path}, line {line} ")),
                "{name}: {detail}"
            );
            assert_eq!(error["span"]["line"], *line, "{name}");
        }
    }
}
