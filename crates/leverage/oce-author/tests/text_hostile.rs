//! Hostile text input (issue #482; the pattern of `import_hostile.rs`): a bounded deterministic
//! mutation sweep over every text fixture — structural offsets, multi-byte characters, escapes,
//! brackets, line feeds, control bytes, bit flips and truncation — with `catch_unwind` naming the
//! mutant. Every case returns a typed result; none panics. A mutant that parses must also print to
//! text that parses back to the same model and is its own `fmt`. Then the giant shapes: deep
//! brackets, giant statements, a block of thousands of assignments, long runs of comments.

use crate::support;

use std::panic::{AssertUnwindSafe, catch_unwind};

use oce_author::{PARSE_CODES, parse_text, parse_text_bytes};

use support::fixture;
use support::text::print;

/// Every text fixture: the bundle goldens, the note's example, the `fmt` inputs and outputs.
fn documents() -> Vec<(String, Vec<u8>)> {
    let mut paths: Vec<_> = ["text", "text/fmt", "note"]
        .iter()
        .flat_map(|dir| std::fs::read_dir(fixture(dir)).expect("fixtures"))
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "oce"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 14, "{paths:?}");
    paths
        .into_iter()
        .map(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .expect("name")
                .to_owned();
            (name, std::fs::read(&p).expect("read"))
        })
        .collect()
}

/// The most offsets a sweep visits in one document.
const MAX_OFFSETS: usize = 160;

/// Half the offsets spread over the structural bytes and digit boundaries, half over every byte.
fn offsets(bytes: &[u8]) -> Vec<usize> {
    let structural: Vec<usize> = (0..bytes.len())
        .filter(|&i| {
            matches!(
                bytes[i],
                b'"' | b'\\'
                    | b'('
                    | b')'
                    | b'['
                    | b']'
                    | b':'
                    | b','
                    | b'='
                    | b'#'
                    | b'@'
                    | b'.'
                    | b'-'
                    | b'>'
                    | b'\n'
            ) || (bytes[i].is_ascii_digit() != bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        })
        .collect();
    let spread = |all: &[usize], count: usize| -> Vec<usize> {
        let stride = all.len().div_ceil(count.max(1)).max(1);
        all.iter().copied().step_by(stride).collect()
    };
    let every: Vec<usize> = (0..bytes.len()).collect();
    let mut chosen = spread(&structural, MAX_OFFSETS / 2);
    chosen.extend(spread(&every, MAX_OFFSETS / 2));
    chosen.sort_unstable();
    chosen.dedup();
    chosen
}

/// Parse `bytes`; if it parses, print it and require the print to parse back to the same model
/// and to be its own `fmt`. A panic anywhere names the mutant.
fn check(bytes: &[u8], document: &str, at: usize, mutation: &str) -> bool {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let model = match parse_text_bytes(bytes) {
            Ok(model) => model,
            Err(error) => {
                assert!(PARSE_CODES.contains(&error.code), "{error}");
                return false;
            }
        };
        let printed = print(&model);
        let again =
            parse_text(&printed).unwrap_or_else(|e| panic!("print does not parse: {e}\n{printed}"));
        assert_eq!(again, model, "print → parse changed the model\n{printed}");
        assert_eq!(print(&again), printed, "fmt is not idempotent\n{printed}");
        true
    }));
    match outcome {
        Ok(parsed) => parsed,
        Err(_) => panic!("the text form failed on {document} at byte {at} ({mutation})"),
    }
}

#[test]
fn a_bounded_mutation_sweep_over_every_text_fixture_never_panics() {
    let insertions: [&[u8]; 16] = [
        "\u{e9}".as_bytes(),
        "\u{1f321}".as_bytes(),
        b"\\",
        b"\"",
        b"\\n",
        b"#",
        b"[",
        b"]",
        b"(",
        b")",
        b"\n",
        b"\n\n",
        b"-",
        b"->",
        b"\t",
        b" 1e999 h ",
    ];
    let (mut cases, mut parsed) = (0usize, 0usize);
    for (name, bytes) in documents() {
        assert!(
            check(&bytes, &name, 0, "unmutated"),
            "{name} parses unmutated"
        );
        // Above 4 KiB a document gets one flip and two insertions per offset, so the sweep stays
        // at a few seconds in a debug build.
        let large = bytes.len() > 4 * 1024;
        for at in offsets(&bytes) {
            for &flip in if large {
                &[0x80u8][..]
            } else {
                &[0x01u8, 0x20, 0x80][..]
            } {
                let mut mutated = bytes.clone();
                mutated[at] ^= flip;
                parsed += usize::from(check(&mutated, &name, at, &format!("xor {flip:#04x}")));
                cases += 1;
            }
            for insert in insertions
                .iter()
                .take(if large { 2 } else { insertions.len() })
            {
                let label = String::from_utf8_lossy(insert).into_owned();
                let mut replaced = bytes[..at].to_vec();
                replaced.extend_from_slice(insert);
                replaced.extend_from_slice(&bytes[at + 1..]);
                parsed += usize::from(check(
                    &replaced,
                    &name,
                    at,
                    &format!("replace with {label:?}"),
                ));
                let mut inserted = bytes[..at].to_vec();
                inserted.extend_from_slice(insert);
                inserted.extend_from_slice(&bytes[at..]);
                parsed += usize::from(check(&inserted, &name, at, &format!("insert {label:?}")));
                cases += 2;
            }
            parsed += usize::from(check(&bytes[..at], &name, at, "truncate"));
            cases += 1;
        }
    }
    assert!(cases > 25_000, "{cases} cases");
    // The sweep reaches the printer, not only the refusals.
    assert!(parsed > cases / 10, "{parsed} of {cases} mutants parsed");
}

#[test]
fn deep_brackets_and_parentheses_are_refused_without_recursing() {
    for open in ["[", "(", "[(", "(["] {
        let text = format!(
            "sequence s\nderived_from none\nblock b = C(k = {}\n",
            open.repeat(3_000)
        );
        let error = parse_text(&text).expect_err("refused");
        assert!(PARSE_CODES.contains(&error.code), "{error}");
    }
}

#[test]
fn giant_statements_parse_or_refuse_in_bounded_time() {
    // A block of 20,000 assignments on 20,000 lines, a line of 400 assignments, 50,000 comment
    // lines before one statement, and a list of 4,000 elements on one line.
    let mut text = String::from("sequence s\nderived_from none\nblock b = CDL.Reals.MultiSum(\n");
    for i in 0..20_000 {
        text.push_str(&format!("  a{i} = {i},\n"));
    }
    text.push_str("  last = 0\n)\n");
    let model = parse_text(&text).expect("parses");
    assert_eq!(model.blocks[0].assignments.len(), 20_001);
    assert_eq!(parse_text(&print(&model)).expect("parses"), model);

    let items: Vec<String> = (0..400).map(|i| format!("a{i} = 1")).collect();
    let line = format!(
        "sequence s\nderived_from none\nblock b = C({})\n",
        items.join(", ")
    );
    assert!(line.len() < 8_192);
    assert_eq!(
        parse_text(&line).expect("parses").blocks[0]
            .assignments
            .len(),
        400
    );

    let comments = format!(
        "sequence s\nderived_from none\n{}in u : Real\n",
        "# c\n".repeat(50_000)
    );
    let model = parse_text(&comments).expect("parses");
    assert_eq!(model.inputs[0].annotations.comments.len(), 50_000);

    let list = format!(
        "sequence s\nderived_from none\nblock b = CDL.Reals.MultiSum(k = [{}])\n",
        vec!["1"; 4_000].join(",")
    );
    let model = parse_text(&list).expect("parses");
    assert_eq!(model.blocks[0].assignments.len(), 4_000);
    assert_eq!(model.blocks[0].assignments[3_999].name.as_str(), "k_4000");
    let printed = print(&model);
    assert!(
        printed.contains("  k = [\n    1,\n    1,\n"),
        "the complete run regroups, one element per line past the one-line width"
    );
    assert!(printed.lines().all(|l| l.len() <= 100));
    assert_eq!(parse_text(&printed).expect("parses"), model);
}

#[test]
fn a_multi_byte_character_at_every_offset_of_a_statement_is_typed() {
    let base = "sequence s\nderived_from none\nblock hys = CDL.Reals.Hysteresis(uLow = 0.0 K, uHigh = dTHys)\n";
    for at in 0..=base.len() {
        if !base.is_char_boundary(at) {
            continue;
        }
        for ch in ['\u{e9}', '\u{20ac}', '\u{1f321}', '\u{feff}', '\u{85}'] {
            let mut text = base.to_owned();
            text.insert(at, ch);
            check(text.as_bytes(), "statement", at, &format!("insert {ch:?}"));
        }
    }
}

#[test]
fn hostile_bounds_and_parameter_types_are_typed() {
    // Issue #492's syntax at its edges: a type name at and past the token bound, a bound at the
    // token bound, every tag repeated on one line, and every byte of a bounded statement replaced
    // or preceded by a multi-byte character.
    let head = "sequence s\nderived_from none\n";
    let at_bound = format!("T{}", "y".repeat(255));
    let s = parse_text(&format!("{head}param p : {at_bound} = {at_bound}.m\n")).expect("parses");
    assert_eq!(s.params[0].value_type.as_str(), at_bound);
    let past = format!("{at_bound}y");
    let error = parse_text(&format!("{head}param p : {past} = 1\n")).expect_err("refused");
    assert_eq!(error.code, "token-too-long");
    let digits = format!("1.{}", "0".repeat(254));
    assert_eq!(digits.len(), 256);
    let s = parse_text(&format!("{head}in u : Real @min {digits}\n")).expect("parses");
    assert_eq!(s.inputs[0].min, Some(oce_author::Bound::Real(1.0)));
    let error = parse_text(&format!("{head}in u : Real @min {digits}0\n")).expect_err("refused");
    assert_eq!(error.code, "token-too-long");
    let repeated = format!("{head}in u : Real{}\n", " @min 0".repeat(1_000));
    let error = parse_text(&repeated).expect_err("refused");
    assert_eq!(error.code, "duplicate-tag");
    let base = "sequence s\nderived_from none\nparam c : SimpleController = SimpleController.PI\nin u : Real @min -1.5e3 @max 2\n";
    for at in 0..=base.len() {
        if !base.is_char_boundary(at) {
            continue;
        }
        for ch in ['\u{e9}', '\u{1f321}', '\u{202e}', '-', '@', '.', '#', ' '] {
            let mut text = base.to_owned();
            text.insert(at, ch);
            check(
                text.as_bytes(),
                "bounded statement",
                at,
                &format!("insert {ch:?}"),
            );
        }
    }
}
