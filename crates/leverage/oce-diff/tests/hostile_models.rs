//! The diff never validates, so it must survive what a validator refuses (diff note, decision
//! 10; `TESTING.md`: hostile input is the default): repeated names, connections to blocks nobody
//! declared, several drivers on one target, strings that try to break a line or a table, and
//! models at the text form's statement limits. Each case has one fixed reading, the diff of any
//! model against itself is empty, and nothing panics.
//!
//! What the diff costs at the statement limits is pinned in counted work by the unit tests of
//! `src/work.rs`, not here and not in elapsed time.

mod support;

use oce_author::{Assignment, Literal, MAX_STATEMENTS_PER_KIND, ParamValue, Sequence, SignalType};
use oce_diff::{Diff, Note, render_markdown, render_text};
use serde_json::json;

use support::{block, boundary, compare, id, link, real_parameter, sequence};

/// A model a validator refuses on several counts at once.
fn invalid() -> Sequence {
    let mut model = sequence("s");
    model.params.push(real_parameter("p", 1.0));
    model.params.push(real_parameter("p", 2.0));
    model.inputs.push(boundary("u", SignalType::Real));
    model.inputs.push(boundary("u", SignalType::Boolean));
    model.outputs.push(boundary("y", SignalType::Real));
    model.blocks.push(block("a", "CDL.Reals.Abs", &[]));
    model.blocks.push(block("a", "CDL.Reals.Sqrt", &[]));
    model.blocks.push(block(
        "a",
        "CDL.Reals.Abs",
        &[("k", Literal::Real(1.0)), ("k", Literal::Real(2.0))],
    ));
    model.blocks.push(block("", "", &[]));
    for (source, target) in [
        ("u", "a.u"),
        ("u", "a.u"),
        ("ghost.y", "a.u"),
        ("a.y", "ghost.u"),
        ("nowhere", "y"),
        ("a.y", "y"),
        ("a.y", "a.u"),
        ("", ""),
        ("a.b.c", "a.b.c"),
    ] {
        model.connections.push(link(source, target));
    }
    model
}

fn assert_empty(diff: &Diff) {
    assert!(diff.summary.equal, "{}", diff.to_json());
    assert!(diff.parameters.is_empty() && diff.interface.is_empty());
    assert!(diff.graph.is_empty() && diff.metadata.is_empty());
    assert!(diff.notes.is_empty());
}

#[test]
fn an_invalid_model_diffed_against_itself_is_empty() {
    let model = invalid();
    let diff = compare(&model, &model);
    assert_empty(&diff);
    assert_eq!(diff.summary.blocks.by_name, 4);
}

#[test]
fn a_repeated_name_is_matched_occurrence_by_occurrence() {
    let base = invalid();
    // Drop the second of the three blocks named `a`: the third now pairs with the second.
    let mut head = base.clone();
    head.blocks.remove(1);
    let diff = compare(&base, &head);
    let json = support::json(&diff);
    // base: Abs, Sqrt, Abs(k, k); head: Abs, Abs(k, k). The second occurrence changed class, and
    // the third has no partner.
    assert_eq!(diff.summary.blocks.by_name, 2);
    let kinds = support::kinds(&json["graph"]);
    assert!(
        kinds.contains(&("removed".to_owned(), "a".to_owned())),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&("added".to_owned(), "a".to_owned())),
        "{kinds:?}"
    );
    assert!(!diff.summary.graph_equal);

    // A repeated parameter: the second `p` is the one that goes.
    let mut head = base.clone();
    head.params.pop();
    assert_eq!(
        support::json(&compare(&base, &head))["parameters"],
        json!([{ "kind": "removed", "subject": "p", "type": "Real",
                 "value": { "real": "2.0", "bits": "0x4000000000000000" }, "affects": [] }])
    );
    // A repeated assignment: the second `k` of the third block changes.
    let mut head = base.clone();
    head.blocks[2].assignments[1].value = ParamValue::Literal(Literal::Real(3.0));
    assert_eq!(
        support::json(&compare(&base, &head))["graph"],
        json!([{ "kind": "reparameterized", "subject": "a.k",
                 "old": { "real": "2.0", "bits": "0x4000000000000000" },
                 "new": { "real": "3.0", "bits": "0x4008000000000000" } }])
    );
}

#[test]
fn several_drivers_on_one_target_are_compared_as_a_multiset() {
    let target = |sources: &[&str]| {
        let mut model = sequence("s");
        model.blocks.push(block("t", "CDL.Reals.Abs", &[]));
        for name in ["a", "b", "c", "d"] {
            model.inputs.push(boundary(name, SignalType::Real));
        }
        for source in sources {
            model.connections.push(link(source, "t.u"));
        }
        model
    };
    let rewired = |base: &[&str], head: &[&str]| {
        support::json(&compare(&target(base), &target(head)))["graph"].clone()
    };
    // The shared driver is set aside; what is left is paired in sorted order.
    assert_eq!(
        rewired(&["a", "b"], &["a", "c"]),
        json!([{ "kind": "rewired", "subject": "t.u", "old": "b", "new": "c" }])
    );
    assert_eq!(
        rewired(&["a"], &["a", "b"]),
        json!([{ "kind": "rewired", "subject": "t.u", "old": null, "new": "b" }])
    );
    assert_eq!(
        rewired(&["b", "a"], &["d", "c"]),
        json!([
          { "kind": "rewired", "subject": "t.u", "old": "a", "new": "c" },
          { "kind": "rewired", "subject": "t.u", "old": "b", "new": "d" }
        ])
    );
    // The same link twice is two drivers, not one.
    assert_eq!(
        rewired(&["a", "a"], &["a"]),
        json!([{ "kind": "rewired", "subject": "t.u", "old": "a", "new": null }])
    );
    // The same drivers in another order are the same drivers (the list is reordered).
    assert_eq!(
        rewired(&["a", "b"], &["b", "a"]),
        json!([{ "kind": "reordered", "subject": "connections", "list": "connections" }])
    );
}

#[test]
fn a_target_on_a_block_nobody_declared_is_reported_as_spelled() {
    let side = |source: &str| {
        let mut model = sequence("s");
        model.inputs.push(boundary("a", SignalType::Real));
        model.inputs.push(boundary("b", SignalType::Real));
        model.connections.push(link(source, "ghost.u"));
        model
    };
    assert_empty(&compare(&side("a"), &side("a")));
    assert_eq!(
        support::json(&compare(&side("a"), &side("b")))["graph"],
        json!([{ "kind": "rewired", "subject": "ghost.u", "old": "a", "new": "b" }])
    );
    // A driver on a block nobody declared is compared by its spelling.
    let side = |source: &str| {
        let mut model = sequence("s");
        model.outputs.push(boundary("y", SignalType::Real));
        model.connections.push(link(source, "y"));
        model
    };
    assert_empty(&compare(&side("ghost.y"), &side("ghost.y")));
    assert_eq!(
        support::json(&compare(&side("ghost.y"), &side("phantom.y")))["graph"],
        json!([{ "kind": "rewired", "subject": "y", "old": "ghost.y", "new": "phantom.y" }])
    );
}

#[test]
fn a_block_wired_to_a_block_nobody_declared_is_never_renamed() {
    let side = |name: &str| {
        let mut model = sequence("s");
        model.inputs.push(boundary("u", SignalType::Real));
        model.blocks.push(block(name, "CDL.Reals.Add", &[]));
        model.connections.push(link("u", &format!("{name}.u1")));
        model
            .connections
            .push(link("ghost.y", &format!("{name}.u2")));
        model
    };
    let diff = compare(&side("old"), &side("new"));
    assert_eq!(diff.summary.blocks.renamed, 0);
    assert_eq!(diff.notes, [Note::NamesDisjoint]);
}

/// Strings a wire document can carry and a name must never hold.
const HOSTILE: [&str; 7] = [
    "line\nfeed",
    "pipe|pipe",
    "back`tick",
    "carriage\rreturn",
    "nul\u{0}",
    "| a | b | c | d |\n| --- | --- | --- | --- |",
    "# not a header\n+ not an entry",
];

/// A model whose every string is `text`.
fn hostile(text: &str, value: f64) -> Sequence {
    let mut model = sequence(text);
    model.annotations.comments.push(text.to_owned());
    model.trailing_comments.push(text.to_owned());
    let mut parameter = real_parameter(text, value);
    parameter.suffix = Some(text.to_owned());
    model.params.push(parameter);
    let mut input = boundary(text, SignalType::Real);
    input.semantic.unit = Some(text.to_owned());
    input.semantic.name = Some(format!("{text}{value}"));
    input.semantic.haystack = vec![text.to_owned(), text.to_owned()];
    input.quantity = Some(text.to_owned());
    input.display_unit = Some(text.to_owned());
    input.annotations.comments.push(format!("{text}{value}"));
    model.inputs.push(input);
    let mut hostile_block = block(
        text,
        text,
        &[
            (text, Literal::Text(format!("{text}{value}"))),
            (
                "member",
                Literal::Member(oce_author::MemberRef::parse(text)),
            ),
        ],
    );
    hostile_block
        .assignments
        .push(Assignment::new(id("reference"), ParamValue::Ref(id(text))));
    model.blocks.push(hostile_block);
    model.blocks.push(block(
        &format!("{text}{value}"),
        text,
        &[(text, Literal::Real(value))],
    ));
    model
        .connections
        .push(link(&format!("{text}{value}"), &format!("{text}.{text}")));
    model
}

#[test]
fn no_string_in_a_model_can_add_a_line_to_the_text_or_a_cell_to_the_table() {
    for text in HOSTILE {
        let (base, head) = (hostile(text, 1.0), hostile(text, 2.0));
        let diff = compare(&base, &head);
        assert!(!diff.summary.equal, "{text:?}");

        let rendered = render_text(&diff);
        assert!(rendered.ends_with('\n'));
        for line in rendered.lines() {
            assert!(
                ["# ", "+ ", "- ", "> ", "~ "]
                    .iter()
                    .any(|start| line.starts_with(start)),
                "{text:?}: a line starts with neither a header nor a sign: {line:?}"
            );
            assert!(!line.chars().any(char::is_control), "{text:?}: {line:?}");
        }
        // One line per entry, one more for each driven input of an added or removed block, the
        // two header lines, the block counts or the notes, and one header per section.
        let json = support::json(&diff);
        let entries: usize = ["parameters", "interface", "graph", "metadata"]
            .iter()
            .map(|section| json[section].as_array().expect("a section").len())
            .sum();
        let inputs: usize = json["graph"]
            .as_array()
            .expect("a section")
            .iter()
            .filter_map(|entry| entry.get("inputs").and_then(|inputs| inputs.as_array()))
            .map(Vec::len)
            .sum();
        let sections = ["parameters", "interface", "graph", "metadata"]
            .iter()
            .filter(|section| !json[**section].as_array().expect("a section").is_empty())
            .count();
        assert_eq!(
            rendered.lines().count(),
            2 + 1 + sections + entries + inputs,
            "{text:?}\n{rendered}"
        );

        let table = render_markdown(&diff);
        assert_eq!(table.lines().count(), 2 + entries, "{text:?}\n{table}");
        for row in table.lines() {
            assert!(
                row.starts_with("| ") && row.ends_with('|'),
                "{text:?}: {row:?}"
            );
            let mut cells = 0;
            let mut escaped = false;
            for character in row.chars() {
                if character == '|' && !escaped {
                    cells += 1;
                }
                escaped = character == '\\' && !escaped;
            }
            assert_eq!(cells, 5, "{text:?}: a row has four cells: {row:?}");
            assert!(!row.chars().any(char::is_control), "{text:?}: {row:?}");
        }
    }
}

#[test]
fn a_hostile_diff_survives_its_own_json() {
    for text in HOSTILE {
        let diff = compare(&hostile(text, 1.0), &hostile(text, 2.0));
        let parsed: Diff = serde_json::from_str(&diff.to_json()).expect("the JSON is a Diff");
        assert_eq!(parsed, diff, "{text:?}");
    }
}

/// A chain of `length` blocks named `<prefix><i>` with one assignment each.
fn chain(prefix: &str, length: usize, gain: f64) -> Sequence {
    let mut model = sequence("chain");
    model.inputs.push(boundary("u", SignalType::Real));
    model.outputs.push(boundary("y", SignalType::Real));
    for i in 0..length {
        model.blocks.push(block(
            &format!("{prefix}{i}"),
            "CDL.Reals.MultiplyByParameter",
            &[("k", Literal::Real(gain))],
        ));
    }
    for i in 0..length - 1 {
        let source = if i == 0 {
            "u".to_owned()
        } else {
            format!("{prefix}{}.y", i - 1)
        };
        model
            .connections
            .push(link(&source, &format!("{prefix}{i}.u")));
    }
    model
        .connections
        .push(link(&format!("{prefix}{}.y", length - 2), "y"));
    model
}

#[test]
fn two_models_at_the_statement_limits_are_diffed() {
    let limit = MAX_STATEMENTS_PER_KIND;
    let base = chain("b", limit, 1.0);
    assert_eq!(base.blocks.len(), limit);
    assert_eq!(base.connections.len(), limit);

    // Every block retuned.
    let diff = compare(&base, &chain("b", limit, 2.0));
    assert_eq!(diff.graph.len(), limit);
    assert_eq!(diff.summary.blocks.by_name, limit);

    // Every block renamed: far past the candidate cap, so the pass is skipped and says so.
    let diff = compare(&base, &chain("h", limit, 1.0));
    assert_eq!(diff.notes, [Note::NamesDisjoint, Note::RenamePassSkipped]);
    assert_eq!(diff.summary.blocks.base_only, limit);
    assert_eq!(diff.summary.blocks.head_only, limit);
    assert_eq!(
        diff.graph.len(),
        2 * limit + 1,
        "every block, and the output's driver"
    );
    assert_eq!(render_markdown(&diff).lines().count(), 2 + 2 * limit + 1);
}
