//! `oce id` against the engine and against itself (GitHub issue #513): every printed id is the
//! one `Engine::content_id` mints for the same bytes (the tracked pin file's rows, the in-repo
//! library rules, the emitter's goldens), a text or wire file has the id of its emitted CXF,
//! stdin and a file agree, the namespace never moves the id, and the bounds and hostile inputs
//! of each form are typed refusals. No id literal is restated here: the pins are
//! `crates/oce-api/tests/fixtures/content_id/pin.tsv`, read in place, and the transcripts of
//! `id_runs.rs`.

mod support;

use std::fs::{self, File};

use oce_api::{Engine, MAX_CXF_BYTES};
use oce_author::MAX_TEXT_BYTES;
use support::{Run, author_fixture, crate_dir, oce, oce_in, scratch};

/// The repository root as seen from the crate directory every run starts in.
const REPO: &str = "../../..";
const PIN: &str = "../../oce-api/tests/fixtures/content_id/pin.tsv";
const LIBRARY: &str = "../../oce-api/tests/fixtures/library_rules";

/// The six sequences `oce-author` keeps as text beside the emitter's golden CXF.
const EMITTED: [&str; 6] = [
    "cooling_only_system_requests",
    "econ_enable",
    "every_element",
    "generic_time_suppression",
    "multizone_vav_relief_fan_group",
    "reheat_overrides",
];

/// The oracle: the id the facade mints for `cxf` in a fresh engine.
fn engine_id(cxf: &[u8]) -> String {
    let mut engine = Engine::new();
    engine.load_cxf(cxf).expect("the document loads");
    engine
        .content_id()
        .expect("a loaded model has an id")
        .to_string()
}

fn bytes_of(relative: &str) -> Vec<u8> {
    fs::read(crate_dir().join(relative)).unwrap_or_else(|_| panic!("fixture {relative}"))
}

/// The id of a clean run, after checking the whole contract of its output: exit 0, nothing on
/// stderr, and stdout exactly `oce:canon1:`, 64 lowercase hex digits and one line feed.
fn printed_id(run: &Run) -> String {
    assert_eq!(run.code, 0, "{}", run.transcript());
    assert!(run.stderr.is_empty(), "{}", run.transcript());
    let line = run.stdout_text();
    let id = line.strip_suffix('\n').expect("one final line feed");
    let digest = id.strip_prefix("oce:canon1:").expect("the canon1 tag");
    assert_eq!(digest.len(), 64, "{id}");
    assert!(
        digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{id}"
    );
    id.to_owned()
}

/// The single error code of a refused run, after checking the run's shape.
fn refusal(run: &Run) -> String {
    assert_eq!(run.code, 2, "{}", run.transcript());
    assert!(run.stderr.is_empty(), "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["command"], "id");
    assert_eq!(report["ok"], false);
    assert!(report.get("content_id").is_none(), "{}", run.transcript());
    let errors = report["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 1, "{}", run.transcript());
    errors[0]["code"].as_str().expect("code").to_owned()
}

// --- the oracle ---

#[test]
fn every_pinned_document_prints_its_pinned_id_and_the_engines() {
    let pin = String::from_utf8(bytes_of(PIN)).expect("the pin file is UTF-8");
    let mut rows = 0;
    for line in pin.lines().filter(|line| !line.starts_with('#')) {
        let (pinned, path) = line.split_once('\t').expect("<id> TAB <path>");
        let relative = format!("{REPO}/{path}");
        let printed = printed_id(&oce(&["id", &relative], b""));
        assert_eq!(printed, pinned, "{path}: the pin");
        assert_eq!(
            printed,
            engine_id(&bytes_of(&relative)),
            "{path}: the engine"
        );
        rows += 1;
    }
    assert!(rows >= 47, "only {rows} pinned documents");
}

#[test]
fn the_in_repo_library_rules_print_the_engines_id() {
    for rule in ["AHU-0004", "AHU-0016"] {
        let path = format!("{LIBRARY}/{rule}/rule.cxf.jsonld");
        assert_eq!(
            printed_id(&oce(&["id", &path], b"")),
            engine_id(&bytes_of(&path)),
            "{rule}"
        );
    }
}

#[test]
fn a_text_file_has_the_id_of_its_committed_and_its_emitted_cxf() {
    for name in EMITTED {
        let text = author_fixture(&format!("text/{name}.oce"));
        let committed = author_fixture(&format!("emit/{name}.cxf.json"));
        let from_text = printed_id(&oce(&["id", &text], b""));
        // The committed golden, by the verb and by the engine.
        assert_eq!(
            from_text,
            printed_id(&oce(&["id", &committed], b"")),
            "{name}"
        );
        assert_eq!(from_text, engine_id(&bytes_of(&committed)), "{name}");
        // What `oce emit` writes for the same file, piped back in.
        let emitted = oce(&["emit", &text], b"");
        assert_eq!(emitted.code, 0, "{name}: {}", emitted.transcript());
        assert_eq!(from_text, engine_id(&emitted.stdout), "{name}");
        assert_eq!(
            from_text,
            printed_id(&oce(&["id", "-", "--from", "cxf"], &emitted.stdout)),
            "{name}"
        );
    }
}

#[test]
fn a_wire_file_has_the_id_of_its_text_form() {
    for name in ["econ_enable", "every_element"] {
        let wire = author_fixture(&format!("wire/{name}.json"));
        let text = author_fixture(&format!("text/{name}.oce"));
        assert_eq!(
            printed_id(&oce(&["id", &wire], b"")),
            printed_id(&oce(&["id", &text], b"")),
            "{name}"
        );
    }
}

#[test]
fn two_different_sequences_print_different_ids() {
    let ids: std::collections::BTreeSet<String> = EMITTED
        .iter()
        .map(|name| {
            printed_id(&oce(
                &["id", &author_fixture(&format!("text/{name}.oce"))],
                b"",
            ))
        })
        .collect();
    assert_eq!(ids.len(), EMITTED.len());
}

// --- the namespace, stdin, the form ---

#[test]
fn the_namespace_never_moves_the_id() {
    let text = author_fixture("text/econ_enable.oce");
    let plain = printed_id(&oce(&["id", &text], b""));
    for namespace in ["urn:plant:ahu1#", "https://example.org/site/", "urn:a:"] {
        assert_eq!(
            printed_id(&oce(&["id", &text, "--namespace", namespace], b"")),
            plain,
            "{namespace}"
        );
        // The document `oce emit` writes under that namespace has the same id.
        let emitted = oce(&["emit", &text, "--namespace", namespace], b"");
        assert_eq!(emitted.code, 0, "{}", emitted.transcript());
        assert_eq!(engine_id(&emitted.stdout), plain, "{namespace}");
    }
}

#[test]
fn stdin_and_a_file_print_the_same_bytes_in_every_form() {
    for (path, form) in [
        (author_fixture("emit/econ_enable.cxf.json"), "cxf"),
        (author_fixture("text/econ_enable.oce"), "text"),
        (author_fixture("wire/econ_enable.json"), "wire"),
    ] {
        let file = oce(&["id", &path], b"");
        let stdin = oce(&["id", "-", "--from", form], &bytes_of(&path));
        printed_id(&file);
        assert_eq!(file.stdout, stdin.stdout, "{form}");
        assert_eq!(stdin.code, 0, "{form}");
        // The report names the input as it was given, and the same id.
        let report = oce(&["id", "-", "--from", form, "--json"], &bytes_of(&path)).report();
        assert_eq!(report["input"], "-");
        assert_eq!(report["form"], form);
        assert_eq!(report["ok"], true);
        assert_eq!(
            format!("{}\n", report["content_id"].as_str().expect("content_id")),
            file.stdout_text()
        );
    }
}

#[test]
fn the_form_is_read_from_the_name_and_from_overrides_it() {
    let dir = scratch("id_forms");
    let cxf = bytes_of(&author_fixture("emit/econ_enable.cxf.json"));
    let expected = engine_id(&cxf);
    for name in ["rule.jsonld", "rule.cxf.jsonld", "rule.cxf.json"] {
        fs::write(dir.join(name), &cxf).expect("write");
        assert_eq!(
            printed_id(&oce_in(&dir, &["id", name], b"")),
            expected,
            "{name}"
        );
    }
    // A CXF document under a bare `.json` name is read as the wire form, and refused by it.
    fs::write(dir.join("rule.json"), &cxf).expect("write");
    let as_wire = refusal(&oce_in(&dir, &["id", "rule.json"], b""));
    assert!(
        oce_author::WIRE_CODES.contains(&as_wire.as_str()),
        "{as_wire}"
    );
    assert_eq!(
        printed_id(&oce_in(&dir, &["id", "rule.json", "--from", "cxf"], b"")),
        expected
    );
    // And a text file under a name that gives no form.
    fs::write(
        dir.join("econ.txt"),
        bytes_of(&author_fixture("text/econ_enable.oce")),
    )
    .expect("write");
    assert_eq!(
        refusal(&oce_in(&dir, &["id", "econ.txt"], b"")),
        "unknown-form"
    );
    assert_eq!(
        printed_id(&oce_in(&dir, &["id", "econ.txt", "--from", "text"], b"")),
        expected
    );
}

#[test]
fn a_second_run_prints_the_same_bytes() {
    let path = format!("{LIBRARY}/AHU-0016/rule.cxf.jsonld");
    for args in [
        &["id", path.as_str()][..],
        &["id", path.as_str(), "--json"][..],
    ] {
        assert_eq!(oce(args, b"").stdout, oce(args, b"").stdout);
    }
}

// --- bounds ---

/// A CXF document padded with trailing spaces to exactly `len` bytes: the same document to a
/// JSON reader.
fn padded(len: usize) -> Vec<u8> {
    let mut bytes = bytes_of(&author_fixture("emit/econ_enable.cxf.json"));
    assert!(bytes.len() < len);
    bytes.resize(len, b' ');
    bytes
}

#[test]
fn a_cxf_document_at_the_engines_bound_is_read_and_one_byte_over_is_refused() {
    let dir = scratch("id_cxf_bound");
    let expected = engine_id(&bytes_of(&author_fixture("emit/econ_enable.cxf.json")));
    let at = padded(MAX_CXF_BYTES);
    fs::write(dir.join("at.cxf.json"), &at).expect("write");
    // Twice a text file's limit, and still read: the bound is the form's own.
    const { assert!(MAX_CXF_BYTES > MAX_TEXT_BYTES) };
    assert_eq!(
        printed_id(&oce_in(&dir, &["id", "at.cxf.json"], b"")),
        expected
    );
    assert_eq!(
        printed_id(&oce_in(&dir, &["id", "-", "--from", "cxf"], &at)),
        expected
    );
    let over = padded(MAX_CXF_BYTES + 1);
    fs::write(dir.join("over.cxf.json"), &over).expect("write");
    let file = oce_in(&dir, &["id", "over.cxf.json"], b"");
    assert_eq!(refusal(&file), "input-too-large");
    assert_eq!(
        file.report()["errors"][0]["detail"],
        format!(
            "the input is {} bytes; at most {MAX_CXF_BYTES} are read",
            MAX_CXF_BYTES + 1
        )
    );
    let stdin = oce_in(&dir, &["id", "-", "--from", "cxf"], &over);
    assert_eq!(refusal(&stdin), "input-too-large");
    assert_eq!(
        stdin.report()["errors"][0]["detail"],
        format!("the input is larger than {MAX_CXF_BYTES} bytes")
    );
}

#[test]
fn an_authored_file_is_bounded_at_its_own_forms_limit() {
    let dir = scratch("id_authored_bound");
    let over = vec![b' '; MAX_TEXT_BYTES + 1];
    for (name, form) in [("over.oce", "text"), ("over.json", "wire")] {
        fs::write(dir.join(name), &over).expect("write");
        assert_eq!(
            refusal(&oce_in(&dir, &["id", name], b"")),
            "input-too-large"
        );
        assert_eq!(
            refusal(&oce_in(&dir, &["id", "-", "--from", form], &over)),
            "input-too-large"
        );
    }
}

#[test]
fn a_hundred_megabyte_cxf_file_is_refused_before_it_is_read() {
    let dir = scratch("id_huge");
    // A sparse file: 100 MB by its metadata, no data written.
    File::create(dir.join("huge.jsonld"))
        .and_then(|file| file.set_len(100 * 1024 * 1024))
        .expect("create a sparse file");
    let run = oce_in(&dir, &["id", "huge.jsonld"], b"");
    assert_eq!(refusal(&run), "input-too-large");
    assert_eq!(
        run.report()["errors"][0]["detail"],
        format!("the input is 104857600 bytes; at most {MAX_CXF_BYTES} are read")
    );
}

// --- hostile CXF ---

#[test]
fn hostile_cxf_is_not_loadable_and_never_a_panic() {
    let dir = scratch("id_hostile");
    let junk: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let deep = [vec![b'['; 200_000], vec![b']'; 200_000]].concat();
    let cases: [(&str, &[u8]); 9] = [
        ("empty", b""),
        ("junk", &junk),
        ("bad UTF-8", b"{\"@graph\": \"\xc3\x28\"}"),
        ("a byte-order mark", b"\xef\xbb\xbf{}"),
        ("null", b"null"),
        ("an empty object", b"{}"),
        ("an empty graph", b"{\"@graph\": []}"),
        ("deep nesting", &deep),
        (
            "a truncated document",
            b"{\"@graph\": [{\"@id\": \"urn:a#top\"",
        ),
    ];
    for (what, bytes) in cases {
        fs::write(dir.join("hostile.cxf.json"), bytes).expect("write");
        let file = oce_in(&dir, &["id", "hostile.cxf.json"], b"");
        assert_eq!(refusal(&file), "not-loadable", "{what}");
        assert_eq!(file.report()["errors"][0]["at"], "input", "{what}");
        let stdin = oce_in(&dir, &["id", "-", "--from", "cxf"], bytes);
        assert_eq!(refusal(&stdin), "not-loadable", "{what}");
        // The same refusal whichever way the bytes arrive, the input's name apart.
        assert_eq!(file.report()["errors"], stdin.report()["errors"], "{what}");
    }
}

#[test]
fn a_truncated_copy_of_a_loadable_document_has_no_id() {
    let dir = scratch("id_truncated");
    let cxf = bytes_of(&author_fixture("emit/econ_enable.cxf.json"));
    for cut in [1, cxf.len() / 2, cxf.len() - 2] {
        fs::write(dir.join("cut.jsonld"), &cxf[..cut]).expect("write");
        assert_eq!(
            refusal(&oce_in(&dir, &["id", "cut.jsonld"], b"")),
            "not-loadable",
            "cut at {cut}"
        );
    }
}

#[test]
fn a_cxf_document_that_loads_with_a_warning_has_an_id_its_text_form_does_not() {
    // `oce emit` refuses a sequence the engine warns about, so its text form has no id; the
    // same graph as a CXF document loads, and a loaded document always has one.
    let text = bytes_of("tests/fixtures/inputs/unused_input.oce");
    let refused = oce(&["id", "tests/fixtures/inputs/unused_input.oce"], b"");
    assert_eq!(refusal(&refused), "not-loadable");
    assert_eq!(
        refused.report()["findings"][0]["code"],
        "unused-boundary-input"
    );
    let sequence = oce_author::parse_text(std::str::from_utf8(&text).expect("UTF-8"))
        .expect("the fixture parses");
    let cxf = oce_author::emit(&sequence, &oce_author::EmitOptions::new())
        .expect("stage 1 accepts the fixture");
    let mut engine = Engine::new();
    let (report, _) = engine.load_cxf(&cxf).expect("the document loads");
    assert!(!report.warnings.is_empty(), "the load warns");
    assert_eq!(
        printed_id(&oce(&["id", "-", "--from", "cxf"], &cxf)),
        engine_id(&cxf)
    );
}

#[test]
fn a_changed_parameter_moves_the_id() {
    let dir = scratch("id_edit");
    let text = String::from_utf8(bytes_of(&author_fixture("text/econ_enable.oce"))).expect("text");
    fs::write(dir.join("a.oce"), &text).expect("write");
    let before = printed_id(&oce_in(&dir, &["id", "a.oce"], b""));
    // A comment and a blank line are not in the loaded graph.
    fs::write(dir.join("b.oce"), format!("# a note\n\n{text}")).expect("write");
    assert_eq!(printed_id(&oce_in(&dir, &["id", "b.oce"], b"")), before);
    let edited = edit_first_real(&text);
    fs::write(dir.join("c.oce"), edited).expect("write");
    assert_ne!(printed_id(&oce_in(&dir, &["id", "c.oce"], b"")), before);
}

/// `text` with the first `uLow = <number>` assignment's number replaced by another.
fn edit_first_real(text: &str) -> String {
    let at = text.find("uLow = ").expect("the fixture sets uLow") + "uLow = ".len();
    let end = at
        + text[at..]
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .expect("the number ends");
    let value: f64 = text[at..end].parse().expect("a number");
    format!("{}{:?}{}", &text[..at], value - 0.25, &text[end..])
}

#[test]
fn a_report_names_the_input_as_given_and_no_host_path() {
    let dir = scratch("id_paths");
    fs::write(dir.join("x.cxf.json"), b"{}").expect("write");
    let run = oce_in(&dir, &["id", "x.cxf.json"], b"");
    assert_eq!(refusal(&run), "not-loadable");
    assert_eq!(run.report()["input"], "x.cxf.json");
    let shown = run.stdout_text();
    assert!(!shown.contains(&*dir.to_string_lossy()), "{shown}");
}
