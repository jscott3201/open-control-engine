//! `oce import` beyond its transcripts (GitHub issue #514): the document is the same bytes from
//! stdin, a file and `-o`; the text and the wire output are one sequence, each canonical in its
//! form and read back by `oce`'s own readers; the verb reads and writes and judges nothing, so
//! what the validator would refuse is carried and a sequence a form cannot spell is refused; the
//! read is bounded at the engine's CXF limit; and a number's bits survive both forms. An Integer
//! literal on a Real parameter is imported as written, emitted as a Real, and keeps the content
//! id, because the engine holds both as one Real (issue #683). `import_corpus.rs` runs the round trip over every CXF fixture
//! tracked under `crates/`, `import_refusals.rs` holds one refusal per importer code, and
//! `output_guards.rs` the two `-o` guards.

mod support;

use std::fs::{self, File};

use oce_api::{Engine, MAX_CXF_BYTES};
use serde_json::{Value, json};
use support::cxf_edit::{XSD_INTEGER, econ, edited, node, real};
use support::{Run, author_fixture, crate_dir, oce, oce_in, scratch};

const AHU_RULE: &str = "../../oce-api/tests/fixtures/library_rules/AHU-0016/rule.cxf.jsonld";
const LIMITS: &str =
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_limits_common.jsonld";

fn bytes_of(relative: &str) -> Vec<u8> {
    fs::read(crate_dir().join(relative)).unwrap_or_else(|_| panic!("fixture {relative}"))
}

/// The document of a clean run, after checking the run's shape: exit 0 and nothing on stderr.
fn document(run: &Run) -> &[u8] {
    assert_eq!(run.code, 0, "{}", run.transcript());
    assert!(run.stderr.is_empty(), "{}", run.transcript());
    &run.stdout
}

/// The single error of a refused run, after checking the run's shape.
fn refusal(run: &Run) -> Value {
    assert_eq!(run.code, 2, "{}", run.transcript());
    assert!(run.stderr.is_empty(), "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["command"], "import");
    assert_eq!(report["ok"], false);
    assert_eq!(report["findings"], json!([]));
    assert!(report.get("output").is_none(), "{}", run.transcript());
    assert!(report.get("bytes").is_none(), "{}", run.transcript());
    let errors = report["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 1, "{}", run.transcript());
    errors[0].clone()
}

/// The oracle: the id the facade mints for `cxf` in a fresh engine.
fn engine_id(cxf: &[u8]) -> String {
    let mut engine = Engine::new();
    engine.load_cxf(cxf).expect("the document loads");
    engine
        .content_id()
        .expect("a loaded model has an id")
        .to_string()
}

/// The id of what `oce emit` writes for the sequence `oce import` reads from `cxf`, in `form`.
fn round_trip_id(cxf: &[u8], form: &str) -> String {
    let import: &[&str] = if form == "wire" {
        &["import", "-", "--wire"]
    } else {
        &["import", "-"]
    };
    let imported = oce(import, cxf);
    let emitted = oce(&["emit", "-", "--from", form], document(&imported));
    engine_id(document(&emitted))
}

// --- stdin, a file, -o ---

#[test]
fn stdin_and_a_file_write_the_same_document_in_both_forms() {
    for path in [AHU_RULE, LIMITS] {
        for flags in [&[][..], &["--wire"][..]] {
            let file = oce(&[&["import", path], flags].concat(), b"");
            let stdin = oce(&[&["import", "-"], flags].concat(), &bytes_of(path));
            assert_eq!(document(&file), document(&stdin), "{path} {flags:?}");
            assert!(file.stdout.ends_with(b"\n") && !file.stdout.ends_with(b"\n\n"));
        }
    }
}

#[test]
fn the_output_flag_writes_the_document_and_reports_it() {
    let dir = scratch("import_output");
    fs::write(dir.join("in.cxf.json"), bytes_of(LIMITS)).expect("copy");
    for (flags, name, form) in [
        (&[][..], "out.oce", "text"),
        (&["--wire"][..], "out.json", "wire"),
    ] {
        let plain = oce_in(&dir, &[&["import", "in.cxf.json"], flags].concat(), b"");
        let run = oce_in(
            &dir,
            &[&["import", "in.cxf.json", "-o", name], flags].concat(),
            b"",
        );
        assert_eq!(run.code, 0, "{}", run.transcript());
        let written = fs::read(dir.join(name)).expect("the output");
        assert_eq!(written, document(&plain), "{form}");
        let report = run.report();
        assert_eq!(report["ok"], true);
        assert_eq!(report["errors"], json!([]));
        assert_eq!(report["input"], "in.cxf.json");
        assert_eq!(report["output"], name);
        assert_eq!(report["output_form"], form);
        assert_eq!(report["bytes"], written.len());
        assert_eq!(report["dropped"].as_array().map(Vec::len), Some(10));
        assert_eq!(report["normalised"].as_array().map(Vec::len), Some(1));
        // `-o -` is stdout: the document, not the report.
        let dash = oce_in(
            &dir,
            &[&["import", "in.cxf.json", "-o", "-"], flags].concat(),
            b"",
        );
        assert_eq!(document(&dash), document(&plain), "{form}");
    }
}

#[test]
fn the_reported_lists_are_the_importers_in_reading_order() {
    let dir = scratch("import_lists");
    let cxf = bytes_of(LIMITS);
    fs::write(dir.join("in.cxf.json"), &cxf).expect("copy");
    let report = oce_in(&dir, &["import", "in.cxf.json", "-o", "out.oce"], b"").report();
    let imported = oce_author::import_cxf(&cxf, &oce_author::ImportOptions::new()).expect("import");
    let dropped: Vec<Value> = imported
        .report
        .dropped
        .iter()
        .map(|d| json!({ "property": d.property.as_str(), "subject": d.subject, "value": d.value }))
        .collect();
    let normalised: Vec<Value> = imported
        .report
        .normalised
        .iter()
        .map(|n| json!({ "data_type": n.data_type, "subject": n.subject, "to": n.to.as_str() }))
        .collect();
    assert_eq!(report["dropped"], Value::Array(dropped));
    assert_eq!(report["normalised"], Value::Array(normalised));
    // The package-typed boundary of importer note decision 21.
    assert_eq!(report["normalised"][0]["to"], "Integer");
    assert!(
        report["normalised"][0]["subject"]
            .as_str()
            .expect("subject")
            .ends_with(".uOpeMod")
    );
}

#[test]
fn a_refused_import_writes_no_file_and_leaves_an_existing_one_alone() {
    let dir = scratch("import_refused_output");
    fs::write(dir.join("bad.cxf.json"), b"{\"@graph\": []}").expect("write");
    let fresh = oce_in(&dir, &["import", "bad.cxf.json", "-o", "fresh.oce"], b"");
    let error = refusal(&fresh);
    assert_eq!(error["code"], "no-root");
    assert!(!dir.join("fresh.oce").exists());
    // The lists belong to an accepted document: a refused one has none.
    assert!(fresh.report().get("dropped").is_none());
    assert!(fresh.report().get("normalised").is_none());
    fs::write(dir.join("kept.oce"), b"kept\n").expect("write");
    refusal(&oce_in(
        &dir,
        &["import", "bad.cxf.json", "-o", "kept.oce"],
        b"",
    ));
    assert_eq!(fs::read(dir.join("kept.oce")).expect("read"), b"kept\n");
}

#[test]
fn a_failed_write_reports_neither_output_nor_bytes_and_keeps_the_lists() {
    let dir = scratch("import_failed_write");
    fs::write(dir.join("in.cxf.json"), bytes_of(LIMITS)).expect("copy");
    fs::create_dir(dir.join("taken")).expect("mkdir");
    let run = oce_in(&dir, &["import", "in.cxf.json", "-o", "taken"], b"");
    assert_eq!(refusal(&run)["code"], "write-failed");
    assert_eq!(run.report()["dropped"].as_array().map(Vec::len), Some(10));
    assert!(dir.join("taken").is_dir());
}

#[test]
fn an_existing_output_is_replaced_whole() {
    let dir = scratch("import_replace");
    fs::write(dir.join("in.cxf.json"), bytes_of(AHU_RULE)).expect("copy");
    // Longer than the document, so a write in place would leave a tail.
    fs::write(dir.join("out.oce"), vec![b'#'; 64 * 1024]).expect("write");
    let run = oce_in(&dir, &["import", "in.cxf.json", "-o", "out.oce"], b"");
    assert_eq!(run.code, 0, "{}", run.transcript());
    let plain = oce(&["import", AHU_RULE], b"");
    assert_eq!(fs::read(dir.join("out.oce")).expect("read"), plain.stdout);
    // Nothing else is left beside it.
    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, ["in.cxf.json", "out.oce"]);
}

// --- the two output forms ---

#[test]
fn the_text_and_the_wire_output_are_one_sequence_each_canonical_in_its_form() {
    for path in [
        AHU_RULE.to_owned(),
        LIMITS.to_owned(),
        author_fixture("emit/every_element.cxf.json"),
    ] {
        let text = oce(&["import", &path], b"");
        let wire = oce(&["import", &path, "--wire"], b"");
        // `oce fmt` prints a wire file as text.
        let from_wire = oce(&["fmt", "-", "--from", "wire"], document(&wire));
        assert_eq!(document(&from_wire), document(&text), "{path}");
        for (form, run) in [("text", &text), ("wire", &wire)] {
            let check = oce(&["fmt", "-", "--from", form, "--check"], &run.stdout);
            assert_eq!(check.code, 0, "{path} {form}: {}", check.transcript());
            assert_eq!(check.report()["canonical"], true);
        }
        // And the library's readers read back the sequence the library imports.
        let imported = oce_author::import_cxf(&bytes_of(&path), &oce_author::ImportOptions::new())
            .expect("import")
            .sequence;
        assert_eq!(
            oce_author::parse_text(text.stdout_text()).expect("text"),
            imported,
            "{path}"
        );
        assert_eq!(
            oce_author::from_wire_bytes(&wire.stdout).expect("wire"),
            imported,
            "{path}"
        );
    }
}

#[test]
fn an_emitted_document_imports_with_nothing_dropped_and_emits_to_the_same_bytes() {
    let dir = scratch("import_emitted");
    for name in ["econ_enable", "every_element", "reheat_overrides"] {
        let golden = bytes_of(&author_fixture(&format!("emit/{name}.cxf.json")));
        fs::write(dir.join("in.cxf.json"), &golden).expect("copy");
        let report = oce_in(&dir, &["import", "in.cxf.json", "-o", "out.oce"], b"").report();
        assert_eq!(report["dropped"], json!([]), "{name}");
        assert_eq!(report["normalised"], json!([]), "{name}");
        let emitted = oce_in(&dir, &["emit", "out.oce"], b"");
        assert_eq!(document(&emitted), golden, "{name}");
    }
}

// --- the verb judges nothing ---

#[test]
fn a_name_the_validator_refuses_is_carried_by_the_wire_form() {
    // A block named `block`: a text-form keyword. The importer carries it (importer note,
    // decision 13), the text form has no spelling for a connection that starts with it, and
    // the wire form does; `oce validate` then names it.
    let cxf = String::from_utf8(econ())
        .expect("UTF-8")
        .replace("econ_enable.sub", "econ_enable.block")
        .into_bytes();
    let text = oce(&["import", "-"], &cxf);
    assert_eq!(refusal(&text)["code"], "unspellable");
    let wire = oce(&["import", "-", "--wire"], &cxf);
    let validated = oce(&["validate", "-", "--from", "wire"], document(&wire));
    assert_eq!(validated.code, 1, "{}", validated.transcript());
    assert_eq!(validated.report()["findings"][0]["code"], "invalid-name");
}

#[test]
fn a_class_the_catalog_lacks_is_imported_and_left_for_the_validator() {
    let cxf = edited(|document| {
        node(document, ".sub")["@type"] =
            json!("http://example.org#Buildings.Controls.OBC.CDL.Reals.NoSuchBlock");
    });
    for (flags, form) in [(&[][..], "text"), (&["--wire"][..], "wire")] {
        let imported = oce(&[&["import", "-"], flags].concat(), &cxf);
        let validated = oce(&["validate", "-", "--from", form], document(&imported));
        assert_eq!(validated.code, 1, "{form}");
        assert_eq!(validated.report()["findings"][0]["code"], "class-not-found");
    }
}

#[test]
fn a_class_path_the_text_form_cannot_spell_is_carried_by_the_wire_form() {
    let cxf = edited(|document| {
        node(document, ".sub")["@type"] =
            json!("http://example.org#Buildings.Controls.OBC.CDL.Reals.Sub tract");
    });
    let text = oce(&["import", "-"], &cxf);
    let error = refusal(&text);
    assert_eq!(error["code"], "unspellable");
    assert!(
        error["detail"]
            .as_str()
            .expect("detail")
            .starts_with("the text form cannot carry the imported sequence: unspellable at sub:"),
        "{error}"
    );
    // The importer accepted the document, so its lists are in the report.
    assert_eq!(text.report()["dropped"], json!([]));
    let wire = oce(&["import", "-", "--wire"], &cxf);
    let read = oce_author::from_wire_bytes(document(&wire)).expect("the wire form reads back");
    assert_eq!(read.blocks[0].class_path, "CDL.Reals.Sub tract");
}

#[test]
fn a_string_past_the_wire_bound_has_no_spelling_in_either_form() {
    // A 5,000-byte class path: longer than a text token and than a wire string. The wire
    // writer would write it; the wire reader would not read it back, so nothing is written.
    let long = "A".repeat(5000);
    let cxf = edited(|document| {
        node(document, ".sub")["@type"] = json!(format!(
            "http://example.org#Buildings.Controls.OBC.CDL.Reals.{long}"
        ));
    });
    let dir = scratch("import_unspellable");
    fs::write(dir.join("in.cxf.json"), &cxf).expect("write");
    for (flags, name, reader) in [
        (&[][..], "out.oce", "the text form cannot carry"),
        (&["--wire"][..], "out.json", "the wire form cannot carry"),
    ] {
        let run = oce_in(
            &dir,
            &[&["import", "in.cxf.json", "-o", name], flags].concat(),
            b"",
        );
        let error = refusal(&run);
        assert_eq!(error["code"], "unspellable", "{name}");
        let detail = error["detail"].as_str().expect("detail");
        assert!(detail.starts_with(reader), "{detail}");
        assert!(!dir.join(name).exists(), "{name} was written");
    }
    let wire = refusal(&oce_in(&dir, &["import", "in.cxf.json", "--wire"], b""));
    assert!(
        wire["detail"]
            .as_str()
            .expect("detail")
            .contains("(string-too-long)"),
        "{wire}"
    );
}

#[test]
fn a_sequence_whose_wire_document_would_pass_the_wire_bound_is_written_as_text_only() {
    // 20,000 boundary inputs: about 2 MB of CXF, 0.4 MB of text and over 4 MiB in the wire
    // form, which spells every boundary's semantic record. The wire reader would refuse the
    // document by its size, so `--wire` writes nothing; the text form carries it.
    let mut document: Value = serde_json::from_slice(&econ()).expect("JSON");
    let mut nodes = Vec::new();
    let mut references = Vec::new();
    for index in 0..20_000 {
        let id = format!("{}.extra{index}", support::cxf_edit::ROOT);
        references.push(json!({ "@id": id }));
        nodes.push(json!({
            "@id": id,
            "@type": "S231:BooleanInput",
            "S231:isOfDataType": { "@id": "S231:Boolean" },
        }));
    }
    node(&mut document, "")["S231:hasInput"]
        .as_array_mut()
        .expect("inputs")
        .extend(references);
    document["@graph"]
        .as_array_mut()
        .expect("a graph")
        .extend(nodes);
    let cxf = serde_json::to_vec(&document).expect("JSON");
    assert!(cxf.len() < MAX_CXF_BYTES);
    let wire = refusal(&oce(&["import", "-", "--wire"], &cxf));
    assert_eq!(wire["code"], "unspellable");
    assert!(
        wire["detail"]
            .as_str()
            .expect("detail")
            .contains("(input-too-large)"),
        "{wire}"
    );
    let text = oce(&["import", "-"], &cxf);
    assert!(document_of_len(&text) < oce_author::MAX_TEXT_BYTES);
    let check = oce(&["fmt", "-", "--from", "text", "--check"], &text.stdout);
    assert_eq!(check.code, 0, "{}", check.transcript());
}

/// The length of a clean run's document.
fn document_of_len(run: &Run) -> usize {
    document(run).len()
}

// --- numbers ---

#[test]
fn a_reals_bits_survive_both_forms() {
    // The id covers every parameter bit, so equal ids mean the value came through whole.
    let mut ids = std::collections::BTreeSet::new();
    for lexical in [
        "0.0",
        "-0.0",
        "5e-324",
        "0.1",
        "0.30000000000000004",
        "-1.5e-7",
    ] {
        let cxf = edited(|document| node(document, ".hys.uLow")["S231:value"] = real(lexical));
        let original = engine_id(&cxf);
        for form in ["text", "wire"] {
            assert_eq!(round_trip_id(&cxf, form), original, "{lexical} as {form}");
        }
        ids.insert(original);
    }
    // Six values, six ids: the comparison above can tell `-0.0` from `0.0`.
    assert_eq!(ids.len(), 6);
}

#[test]
fn an_integer_literal_on_a_real_parameter_is_imported_as_written_and_keeps_its_content_id() {
    // The importer keeps the literal as the document wrote it (`uLow = 0`) and the emitter
    // writes a Real parameter's value as a Real. The engine holds an Integer bound to a Real
    // parameter as that Real (issue #683), so the original, the document with the literal
    // written as a Real and the round trip through either form have one id. (Until 0.6.0 the
    // id told the two spellings apart and this round trip moved it.)
    let cases: [(&str, Value, &str, &str); 4] = [
        (".hys.uLow", json!(0), "uLow = 0,", "0.0"),
        (
            ".hys.uLow",
            json!({ "@value": "0", "@type": XSD_INTEGER }),
            "uLow = 0,",
            "0.0",
        ),
        (".dTHys", json!(1), "param dTHys : Real = 1\n", "1.0"),
        (
            ".dTHys",
            json!({ "@value": "1", "@type": XSD_INTEGER }),
            "param dTHys : Real = 1\n",
            "1.0",
        ),
    ];
    let fixture_id = engine_id(&econ());
    for (suffix, integer, spelled, as_real) in cases {
        let original = edited(|document| node(document, suffix)["S231:value"] = integer.clone());
        let written_as_real =
            edited(|document| node(document, suffix)["S231:value"] = real(as_real));
        let text = oce(&["import", "-"], &original);
        assert!(
            String::from_utf8_lossy(document(&text)).contains(spelled),
            "{suffix}: {}",
            text.transcript()
        );
        let original_id = engine_id(&original);
        assert_eq!(original_id, engine_id(&written_as_real), "{suffix}");
        for form in ["text", "wire"] {
            assert_eq!(
                round_trip_id(&original, form),
                original_id,
                "{suffix} {form}"
            );
        }
        // The fixture writes these two values as `0.0` and `1.0`: respelling one as an Integer
        // is not a change to the document at all.
        assert_eq!(original_id, fixture_id, "{suffix}");
    }
    // Not vacuous: another Integer is another document, and still the one its Real spelling is.
    let other = edited(|document| node(document, ".hys.uLow")["S231:value"] = json!(-1));
    let other_as_real = edited(|document| node(document, ".hys.uLow")["S231:value"] = real("-1.0"));
    assert_ne!(engine_id(&other), fixture_id);
    assert_eq!(engine_id(&other), engine_id(&other_as_real));
    for form in ["text", "wire"] {
        assert_eq!(round_trip_id(&other, form), engine_id(&other), "{form}");
    }
}

#[test]
fn an_integer_parameter_keeps_its_id() {
    // An Integer literal on an Integer parameter is not that case.
    let path = author_fixture("emit/every_element.cxf.json");
    let cxf = bytes_of(&path);
    assert!(String::from_utf8_lossy(&cxf).contains("\"S231:Integer\""));
    for form in ["text", "wire"] {
        assert_eq!(round_trip_id(&cxf, form), engine_id(&cxf), "{form}");
    }
}

// --- the read ---

/// The worked example padded with trailing spaces to exactly `len` bytes: the same document to
/// a JSON reader.
fn padded(len: usize) -> Vec<u8> {
    let mut bytes = econ();
    assert!(bytes.len() < len);
    bytes.resize(len, b' ');
    bytes
}

#[test]
fn a_document_at_the_engines_bound_is_imported_and_one_byte_over_is_refused() {
    let dir = scratch("import_bound");
    let expected = oce(&["import", "-"], &econ());
    let at = padded(MAX_CXF_BYTES);
    fs::write(dir.join("at.cxf.json"), &at).expect("write");
    assert_eq!(
        document(&oce_in(&dir, &["import", "at.cxf.json"], b"")),
        document(&expected)
    );
    assert_eq!(
        document(&oce_in(&dir, &["import", "-"], &at)),
        document(&expected)
    );
    let over = padded(MAX_CXF_BYTES + 1);
    fs::write(dir.join("over.cxf.json"), &over).expect("write");
    // The tool's own code, as for every other verb: the importer's `too-large` is never
    // reached, because the read stops at the importer's limit.
    let file = refusal(&oce_in(&dir, &["import", "over.cxf.json"], b""));
    assert_eq!(file["code"], "input-too-large");
    assert_eq!(
        file["detail"],
        format!(
            "the input is {} bytes; at most {MAX_CXF_BYTES} are read",
            MAX_CXF_BYTES + 1
        )
    );
    let stdin = refusal(&oce_in(&dir, &["import", "-"], &over));
    assert_eq!(stdin["code"], "input-too-large");
    assert_eq!(
        stdin["detail"],
        format!("the input is larger than {MAX_CXF_BYTES} bytes")
    );
    let importer = oce_author::import_cxf(&over, &oce_author::ImportOptions::new())
        .expect_err("the importer's own limit is the same number");
    assert_eq!(importer.code(), "too-large");
}

#[test]
fn a_hundred_megabyte_file_is_refused_before_it_is_read() {
    let dir = scratch("import_huge");
    // A sparse file: 100 MB by its metadata, no data written.
    File::create(dir.join("huge.jsonld"))
        .and_then(|file| file.set_len(100 * 1024 * 1024))
        .expect("create a sparse file");
    let error = refusal(&oce_in(
        &dir,
        &["import", "huge.jsonld", "-o", "out.oce"],
        b"",
    ));
    assert_eq!(error["code"], "input-too-large");
    assert_eq!(
        error["detail"],
        format!("the input is 104857600 bytes; at most {MAX_CXF_BYTES} are read")
    );
    assert!(!dir.join("out.oce").exists());
}

#[test]
fn the_name_of_the_input_gives_no_form() {
    // One input form: a CXF document is imported whatever its name ends in.
    let dir = scratch("import_names");
    let expected = oce(&["import", "-"], &econ());
    for name in [
        "rule.jsonld",
        "rule.cxf.json",
        "rule.json",
        "rule.oce",
        "rule",
    ] {
        fs::write(dir.join(name), econ()).expect("write");
        assert_eq!(
            document(&oce_in(&dir, &["import", name], b"")),
            document(&expected),
            "{name}"
        );
    }
}

// --- determinism and what a report shows ---

#[test]
fn a_second_run_writes_the_same_bytes() {
    for args in [
        &["import", LIMITS][..],
        &["import", LIMITS, "--wire"][..],
        &[
            "import",
            "../../oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld",
        ][..],
    ] {
        assert_eq!(oce(args, b"").stdout, oce(args, b"").stdout, "{args:?}");
    }
}

#[test]
fn a_report_names_the_input_and_the_output_as_given_and_no_host_path() {
    let dir = scratch("import_paths");
    fs::create_dir(dir.join("sub")).expect("mkdir");
    fs::write(dir.join("sub/x.cxf.json"), econ()).expect("write");
    let run = oce_in(&dir, &["import", "sub/x.cxf.json", "-o", "sub/x.oce"], b"");
    assert_eq!(run.code, 0, "{}", run.transcript());
    assert_eq!(run.report()["input"], "sub/x.cxf.json");
    assert_eq!(run.report()["output"], "sub/x.oce");
    let refused = oce_in(&dir, &["import", "sub/missing.cxf.json"], b"");
    assert_eq!(refusal(&refused)["code"], "not-found");
    for shown in [run.stdout_text(), refused.stdout_text()] {
        assert!(!shown.contains(&*dir.to_string_lossy()), "{shown}");
    }
}
