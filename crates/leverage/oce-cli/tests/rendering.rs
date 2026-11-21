//! The human-readable rendering on stderr never carries a format, control or separator
//! character a document wrote (GitHub #688). A wire file's strings may hold a right-to-left
//! override, a zero-width space or a line separator (the wire reader refuses only control
//! characters), and a finding quotes them: `class-not-found` names the class as written. Until
//! #688 `oce validate` and `oce emit` wrote such a string to the terminal as it was, so a wire
//! file could reorder or hide part of the line a person reads. Each is now written as a
//! `\u{…}` escape, in the engine's spelling; the JSON report on stdout still carries the string
//! exactly.

mod support;

use support::{author_fixture, crate_dir, oce};

/// A right-to-left override, a zero-width space, a line separator, a paragraph separator, a
/// soft hyphen and a byte-order mark: none is a control character, so the wire reader carries
/// every one.
const HOSTILE: &str = "\u{202e}\u{200b}\u{2028}\u{2029}\u{ad}\u{feff}";
/// The same characters as the rendering writes them.
const ESCAPED: &str = "\\u{202E}\\u{200B}\\u{2028}\\u{2029}\\u{00AD}\\u{FEFF}";

/// The worked example's wire form with a block class that carries [`HOSTILE`].
fn wire_with_a_hostile_class() -> Vec<u8> {
    let wire = std::fs::read_to_string(crate_dir().join(author_fixture("wire/econ_enable.json")))
        .expect("fixture");
    let edited = wire.replace(
        "CDL.Reals.Subtract",
        &format!("CDL.Reals.Sub{HOSTILE}tract"),
    );
    assert_ne!(edited, wire, "the fixture names CDL.Reals.Subtract");
    edited.into_bytes()
}

fn carries_none(rendered: &str) -> bool {
    !rendered.chars().any(|c| {
        c != '\n' && (oce_api::is_format_or_control(c) || matches!(c, '\u{2028}' | '\u{2029}'))
    })
}

#[test]
fn a_finding_that_quotes_a_wire_files_class_is_rendered_with_escapes() {
    let wire = wire_with_a_hostile_class();
    for command in ["validate", "emit"] {
        let run = oce(
            &[command, "-", "--from", "wire", "--render", "always"],
            &wire,
        );
        assert_eq!(run.code, 1, "{command}: {}", run.transcript());
        // The report carries the class exactly as the file wrote it.
        let report = run.report();
        let finding = &report["findings"][0];
        assert_eq!(finding["code"], "class-not-found", "{command}");
        assert!(
            finding["detail"]
                .as_str()
                .expect("detail")
                .contains(HOSTILE),
            "{command}: the report quotes the class"
        );
        // The rendering quotes it too, every such character escaped: two lines, as written.
        let rendered = String::from_utf8(run.stderr.clone()).expect("UTF-8");
        assert!(carries_none(&rendered), "{command}: {rendered:?}");
        assert!(
            rendered.contains(&format!("CDL.Reals.Sub{ESCAPED}tract")),
            "{command}: {rendered:?}"
        );
        assert_eq!(rendered.matches('\n').count(), 2, "{command}: {rendered:?}");
    }
}

#[test]
fn a_rendering_with_nothing_to_escape_is_the_bytes_it_was() {
    // The escape touches only those characters: the pinned `--render always` transcripts of
    // every command (`validate_render_always`, `id_render_refusal`, `verify_render_always`, …)
    // did not move with it, and a clean file still renders one plain verdict line.
    let run = oce(
        &[
            "validate",
            &author_fixture("wire/econ_enable.json"),
            "--render",
            "always",
        ],
        b"",
    );
    assert_eq!(run.code, 0, "{}", run.transcript());
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        format!(
            "oce validate {}: clean\n",
            author_fixture("wire/econ_enable.json")
        )
    );
}
