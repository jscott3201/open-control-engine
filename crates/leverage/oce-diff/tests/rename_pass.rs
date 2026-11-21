//! The rename pass (diff note, "Identity"; decision 11) through the public diff: the hard gates,
//! agreement and refusal, mutual uniqueness, the rounds and their limit, both caps at their
//! boundaries, the switch and the hint, and `graph_equal` never depending on any of it.
//!
//! Example (c) of the note pins refusal on disagreement and on a changed parameter; here each
//! refusal also has its control, the same pair with the one difference taken away, which is
//! matched.

mod support;

use oce_author::{Literal, Sequence, SignalType};
use oce_diff::{
    Confidence, Diff, DiffOptions, GraphChange, MAX_RENAME_CANDIDATES, MAX_RENAME_PAIRS,
    MAX_RENAME_ROUNDS, Note, diff,
};

use support::{block, boundary, compare, compare_without_renames, example, link, parse, sequence};

/// The `(from, subject, confidence)` of every `renamed` entry, in the diff's order.
fn renames(diff: &Diff) -> Vec<(String, String, Confidence)> {
    diff.graph
        .iter()
        .filter_map(|change| match change {
            GraphChange::Renamed {
                subject,
                from,
                confidence,
                ..
            } => Some((from.clone(), subject.clone(), *confidence)),
            _ => None,
        })
        .collect()
}

/// The subjects of the entries of one kind, in the diff's order.
fn subjects(diff: &Diff, kind: &str) -> Vec<String> {
    support::kinds(&support::json(diff)["graph"])
        .into_iter()
        .filter(|(found, _)| found == kind)
        .map(|(_, subject)| subject)
        .collect()
}

fn model(body: &str) -> Sequence {
    parse(&format!("sequence s\nderived_from none\n{body}"))
}

/// A chain of `length` identical blocks named `<prefix>1` … driven from the boundary input `u`:
/// only the first has an anchor at the start, so each round can match exactly one more.
fn chain(prefix: &str, length: usize) -> Sequence {
    let mut model = sequence("chain");
    model.inputs.push(boundary("u", SignalType::Real));
    for i in 1..=length {
        model
            .blocks
            .push(block(&format!("{prefix}{i}"), "CDL.Reals.Abs", &[]));
        let source = if i == 1 {
            "u".to_owned()
        } else {
            format!("{prefix}{}.y", i - 1)
        };
        model
            .connections
            .push(link(&source, &format!("{prefix}{i}.u")));
    }
    model
}

#[test]
fn a_chain_that_needs_every_round_is_matched_whole() {
    let diff = compare(
        &chain("b", MAX_RENAME_ROUNDS),
        &chain("h", MAX_RENAME_ROUNDS),
    );
    let expected: Vec<_> = (1..=MAX_RENAME_ROUNDS)
        .map(|i| (format!("b{i}"), format!("h{i}"), Confidence::Exact))
        .collect();
    assert_eq!(renames(&diff), expected);
    assert_eq!(diff.summary.blocks.renamed, MAX_RENAME_ROUNDS);
    assert_eq!(diff.summary.blocks.base_only, 0);
    assert_eq!(diff.summary.blocks.head_only, 0);
    // Every link agrees once the renames are mapped: nothing is rewired.
    assert!(subjects(&diff, "rewired").is_empty());
    assert_eq!(diff.notes, [Note::NamesDisjoint]);
}

#[test]
fn a_chain_that_needs_more_rounds_stops_at_the_limit() {
    let length = MAX_RENAME_ROUNDS + 2;
    let diff = compare(&chain("b", length), &chain("h", length));
    let mut expected: Vec<_> = (1..MAX_RENAME_ROUNDS)
        .map(|i| (format!("b{i}"), format!("h{i}"), Confidence::Exact))
        .collect();
    // The last matched block's output runs to a block no round reached: seen in part.
    expected.push((
        format!("b{MAX_RENAME_ROUNDS}"),
        format!("h{MAX_RENAME_ROUNDS}"),
        Confidence::High,
    ));
    assert_eq!(renames(&diff), expected);
    assert_eq!(subjects(&diff, "removed"), ["b10", "b9"]);
    assert_eq!(subjects(&diff, "added"), ["h10", "h9"]);
    assert_eq!(diff.summary.blocks.renamed, MAX_RENAME_ROUNDS);
    assert_eq!(diff.summary.blocks.base_only, 2);
    assert_eq!(diff.summary.blocks.head_only, 2);
    // Not a cap: the pass ran, so there is no skipped note.
    assert_eq!(diff.notes, [Note::NamesDisjoint]);
}

#[test]
fn two_identical_candidates_for_one_block_stay_removed_plus_added() {
    let base = model("in u : Real\nblock a = CDL.Reals.Abs\nu -> a.u\n");
    let head = model(
        "in u : Real\nblock x = CDL.Reals.Abs\nblock y = CDL.Reals.Abs\nu -> x.u\nu -> y.u\n",
    );
    for diff in [compare(&base, &head), compare(&head, &base)] {
        assert!(renames(&diff).is_empty());
        assert_eq!(diff.summary.blocks.renamed, 0);
    }
    let diff = compare(&base, &head);
    assert_eq!(subjects(&diff, "removed"), ["a"]);
    assert_eq!(subjects(&diff, "added"), ["x", "y"]);

    // Two against two is as ambiguous as one against two.
    let base = model(
        "in u : Real\nblock a = CDL.Reals.Abs\nblock b = CDL.Reals.Abs\nu -> a.u\nu -> b.u\n",
    );
    assert!(renames(&compare(&base, &head)).is_empty());
}

#[test]
fn a_second_port_that_tells_two_candidates_apart_makes_each_unique() {
    let base = model(
        "in u : Real\nout p : Real\nout q : Real\nblock a = CDL.Reals.Abs\nblock b = CDL.Reals.Abs\n\
         u -> a.u\nu -> b.u\na.y -> p\nb.y -> q\n",
    );
    let head = model(
        "in u : Real\nout p : Real\nout q : Real\nblock x = CDL.Reals.Abs\nblock y = CDL.Reals.Abs\n\
         u -> x.u\nu -> y.u\ny.y -> p\nx.y -> q\n",
    );
    assert_eq!(
        renames(&compare(&base, &head)),
        [
            ("b".to_owned(), "x".to_owned(), Confidence::Exact),
            ("a".to_owned(), "y".to_owned(), Confidence::Exact)
        ]
    );
}

#[test]
fn example_c_refuses_the_crossed_operands_and_the_changed_parameter() {
    let (base, head) = example("plant_reset");
    let diff = compare(&base, &head);
    assert_eq!(
        renames(&diff),
        [
            ("THigh".to_owned(), "TMax".to_owned(), Confidence::Exact),
            ("TLow".to_owned(), "TMin".to_owned(), Confidence::Exact),
            ("sel".to_owned(), "sw".to_owned(), Confidence::High)
        ]
    );
    assert_eq!(subjects(&diff, "removed"), ["ena", "hold"]);
    assert_eq!(subjects(&diff, "added"), ["onDelay", "plaOn"]);
}

#[test]
fn example_c_with_the_operands_uncrossed_and_the_delay_restored_is_matched_whole() {
    // The control for the two refusals: take each difference away and the pair is matched.
    let (base, head) = example("plant_reset");
    let mut uncrossed = head.clone();
    uncrossed.connections[0] = link("uPlaEna", "plaOn.u1");
    uncrossed.connections[1] = link("uReq", "plaOn.u2");
    let diff = compare(&base, &uncrossed);
    assert!(
        renames(&diff).contains(&("ena".to_owned(), "plaOn".to_owned(), Confidence::High)),
        "{:?}",
        renames(&diff)
    );
    assert_eq!(subjects(&diff, "removed"), ["hold"]);

    let mut restored = uncrossed.clone();
    restored.blocks[1].assignments[0].value = Literal::Integer(300).into();
    let diff = compare(&base, &restored);
    assert_eq!(diff.summary.blocks.renamed, 5);
    assert!(
        renames(&diff)
            .iter()
            .all(|(_, _, confidence)| *confidence == Confidence::Exact)
    );
    assert_eq!(diff.graph.len(), 5, "five renames and nothing else");
    assert!(!diff.summary.graph_equal, "a rename is a graph difference");
}

#[test]
fn the_gate_compares_assignments_bit_for_bit_and_ignores_their_order() {
    let pair = |old: &[(&str, Literal)], new: &[(&str, Literal)]| {
        let side = |name: &str, assignments: &[(&str, Literal)]| {
            let mut model = sequence("s");
            model.inputs.push(boundary("u", SignalType::Real));
            model
                .blocks
                .push(block(name, "CDL.Reals.Limiter", assignments));
            model.connections.push(link("u", &format!("{name}.u")));
            model
        };
        compare(&side("old", old), &side("new", new))
            .summary
            .blocks
            .renamed
    };
    let (max, min) = (("uMax", Literal::Real(1.0)), ("uMin", Literal::Real(0.0)));
    assert_eq!(
        pair(&[max.clone(), min.clone()], &[max.clone(), min.clone()]),
        1
    );
    assert_eq!(
        pair(&[max.clone(), min.clone()], &[min.clone(), max.clone()]),
        1,
        "order"
    );
    assert_eq!(
        pair(&[max.clone(), min.clone()], std::slice::from_ref(&max)),
        0,
        "a missing assignment"
    );
    assert_eq!(
        pair(
            &[max.clone(), min.clone()],
            &[max.clone(), ("uMin", Literal::Real(-0.0))]
        ),
        0,
        "-0.0 against 0.0"
    );
    assert_eq!(
        pair(
            &[max.clone(), min.clone()],
            &[max.clone(), ("uMin", Literal::Integer(0))]
        ),
        0,
        "an Integer literal against the equal Real"
    );
    let nan = |bits: u64| ("uMin", Literal::Real(f64::from_bits(bits)));
    assert_eq!(
        pair(&[nan(0x7ff8_0000_0000_0001)], &[nan(0x7ff8_0000_0000_0001)]),
        1
    );
    assert_eq!(
        pair(&[nan(0x7ff8_0000_0000_0001)], &[nan(0x7ff8_0000_0000_0002)]),
        0,
        "a NaN equals only its own payload"
    );
}

#[test]
fn the_gate_compares_a_reference_by_the_parameter_it_names() {
    let side = |name: &str, parameter: &str| {
        model(&format!(
            "param a : Real = 1.0\nparam b : Real = 1.0\nin u : Real\n\
             block {name} = CDL.Reals.MultiplyByParameter(k = {parameter})\nu -> {name}.u\n"
        ))
    };
    assert_eq!(
        compare(&side("old", "a"), &side("new", "a"))
            .summary
            .blocks
            .renamed,
        1
    );
    // `a` and `b` hold the same value; the gate does not look.
    assert_eq!(
        compare(&side("old", "a"), &side("new", "b"))
            .summary
            .blocks
            .renamed,
        0
    );
    // The class is a gate as well.
    let other = model("in u : Real\nblock new = CDL.Reals.Abs\nu -> new.u\n");
    let abs = model("in u : Real\nblock old = CDL.Reals.Sqrt\nu -> old.u\n");
    assert_eq!(compare(&abs, &other).summary.blocks.renamed, 0);
}

#[test]
fn a_pair_with_no_agreeing_port_is_never_matched() {
    // Nothing connects the two blocks to anything: same class, same assignments, no evidence.
    let base = model("block old = CDL.Reals.Abs\n");
    let head = model("block new = CDL.Reals.Abs\n");
    let diff = compare(&base, &head);
    assert!(renames(&diff).is_empty());
    assert_eq!(subjects(&diff, "removed"), ["old"]);
    assert_eq!(subjects(&diff, "added"), ["new"]);
}

#[test]
fn a_port_linked_on_one_side_only_is_a_disagreement() {
    let base = model("in u : Real\nout y : Real\nblock old = CDL.Reals.Abs\nu -> old.u\n");
    let head =
        model("in u : Real\nout y : Real\nblock new = CDL.Reals.Abs\nu -> new.u\nnew.y -> y\n");
    assert!(renames(&compare(&base, &head)).is_empty());
    assert!(renames(&compare(&head, &base)).is_empty());
    // An output with one more target is the same refusal.
    let base = model(
        "in u : Real\nout y : Real\nout z : Real\nblock old = CDL.Reals.Abs\nu -> old.u\nold.y -> y\n",
    );
    let head = model(
        "in u : Real\nout y : Real\nout z : Real\nblock new = CDL.Reals.Abs\nu -> new.u\nnew.y -> y\nnew.y -> z\n",
    );
    assert!(renames(&compare(&base, &head)).is_empty());
}

#[test]
fn a_boundary_declared_on_one_side_only_is_no_anchor_and_blocks_the_rename() {
    let base = model(
        "in u : Real\nin old_in : Real\nblock old = CDL.Reals.Add\nu -> old.u1\nold_in -> old.u2\n",
    );
    let head = model(
        "in u : Real\nin new_in : Real\nblock new = CDL.Reals.Add\nu -> new.u1\nnew_in -> new.u2\n",
    );
    let diff = compare(&base, &head);
    assert!(renames(&diff).is_empty(), "boundaries are never renamed");
    assert_eq!(
        support::kinds(&support::json(&diff)["interface"]),
        [
            ("added".to_owned(), "new_in".to_owned()),
            ("removed".to_owned(), "old_in".to_owned())
        ]
    );
}

#[test]
fn a_swap_of_two_names_across_classes_is_two_renames() {
    let base = model(
        "in u : Real\nout y : Real\nblock a = CDL.Reals.Abs\nblock b = CDL.Reals.Sqrt\n\
         u -> a.u\na.y -> b.u\nb.y -> y\n",
    );
    let head = model(
        "in u : Real\nout y : Real\nblock b = CDL.Reals.Abs\nblock a = CDL.Reals.Sqrt\n\
         u -> b.u\nb.y -> a.u\na.y -> y\n",
    );
    let diff = compare(&base, &head);
    assert_eq!(
        renames(&diff),
        [
            ("b".to_owned(), "a".to_owned(), Confidence::Exact),
            ("a".to_owned(), "b".to_owned(), Confidence::Exact)
        ]
    );
    assert_eq!(diff.summary.blocks.by_name, 0);
    assert_eq!(diff.notes, [Note::NamesDisjoint]);
    assert!(subjects(&diff, "rewired").is_empty());
}

#[test]
fn a_swap_of_two_names_within_one_class_is_matched_by_name_not_renamed() {
    // Names are the identity: `a` is still `a`, with another gain and other wiring.
    let base = model(
        "in u : Real\nout y : Real\nblock a = CDL.Reals.MultiplyByParameter(k = 1.0)\n\
         block b = CDL.Reals.MultiplyByParameter(k = 2.0)\nu -> a.u\na.y -> b.u\nb.y -> y\n",
    );
    let head = model(
        "in u : Real\nout y : Real\nblock b = CDL.Reals.MultiplyByParameter(k = 1.0)\n\
         block a = CDL.Reals.MultiplyByParameter(k = 2.0)\nu -> b.u\nb.y -> a.u\na.y -> y\n",
    );
    let diff = compare(&base, &head);
    assert!(renames(&diff).is_empty());
    assert_eq!(diff.summary.blocks.by_name, 2);
    assert_eq!(subjects(&diff, "reparameterized"), ["a.k", "b.k"]);
    assert_eq!(subjects(&diff, "rewired"), ["a.u", "b.u", "y"]);
}

#[test]
fn turning_the_pass_off_leaves_every_unmatched_block_removed_or_added() {
    let (base, head) = example("pressure_request");
    let off = compare_without_renames(&base, &head);
    assert_eq!(off.notes, [Note::RenamePassOff]);
    assert!(renames(&off).is_empty());
    assert_eq!(subjects(&off, "removed"), ["openLong", "seventySet"]);
    assert_eq!(subjects(&off, "added"), ["setFrac", "starvedLong"]);
    // A renamed source is now a different block, so its target is rewired.
    assert_eq!(
        subjects(&off, "rewired"),
        ["short.u1", "want.u1", "want.u2"]
    );
    assert!(!off.summary.graph_equal);
    // The note is carried even when nothing was left to pair.
    let same = compare_without_renames(&base, &base);
    assert_eq!(same.notes, [Note::RenamePassOff]);
    assert!(same.summary.graph_equal);
    assert!(same.summary.equal, "a note is not a difference");
}

#[test]
fn the_content_id_hint_never_changes_the_result() {
    let mut hinted = DiffOptions::new();
    hinted.canonical_ids_equal = true;
    // The statement holds: reordered blocks load to one graph with one canonical id. The fast
    // path is that nothing is left to pair, hinted or not, and the graph is equal as the id says.
    let base = model(
        "in u : Real\nblock a = CDL.Reals.Abs\nblock b = CDL.Reals.Sqrt\nu -> a.u\nu -> b.u\n",
    );
    let head = model(
        "in u : Real\nblock b = CDL.Reals.Sqrt\nblock a = CDL.Reals.Abs\nu -> b.u\nu -> a.u\n",
    );
    let with = diff(&base, &head, &hinted);
    assert_eq!(with, compare(&base, &head));
    assert!(with.notes.is_empty());
    assert!(with.summary.graph_equal, "equal ids are an equal graph");
    assert!(
        !with.summary.equal,
        "and do not prove an empty authoring diff"
    );
    // The statement is false for every pair below (their blocks differ, so their ids do). A
    // wrong hint costs nothing: the pass runs as it would without it, byte for byte.
    let mut false_hints = 0;
    for name in support::EXAMPLES.into_iter().chain(["every_kind"]) {
        let (base, head) = example(name);
        let with = diff(&base, &head, &hinted);
        let without = compare(&base, &head);
        assert_eq!(with, without, "{name}");
        assert_eq!(with.to_json(), without.to_json(), "{name}");
        false_hints += usize::from(!without.summary.graph_equal);
    }
    assert!(
        false_hints >= 3,
        "{false_hints} pairs give the hint the lie"
    );
    let (base, head) = example("pressure_request");
    assert_eq!(
        renames(&diff(&base, &head, &hinted)),
        [(
            "seventySet".to_owned(),
            "setFrac".to_owned(),
            Confidence::Exact
        )]
    );
    // The switch still switches: `renames: false` beside the hint turns the pass off.
    hinted.renames = false;
    assert_eq!(
        diff(&base, &head, &hinted),
        compare_without_renames(&base, &head)
    );
}

/// `count` unconnected constants named `<prefix><i>`, each with its own value, so each is a
/// gated pair with exactly one block of the other side.
fn constants(model: &mut Sequence, prefix: &str, count: usize) {
    for i in 0..count {
        model.blocks.push(block(
            &format!("{prefix}{i}"),
            "CDL.Integers.Sources.Constant",
            &[("k", Literal::Integer(i64::try_from(i).expect("fits")))],
        ));
    }
}

/// A side with one anchored block named `name` (the rename a running pass finds) and `count`
/// unmatched blocks in all.
fn candidates(name: &str, prefix: &str, count: usize) -> Sequence {
    let mut model = sequence("s");
    model.inputs.push(boundary("u", SignalType::Real));
    model.blocks.push(block(name, "CDL.Reals.Abs", &[]));
    model.connections.push(link("u", &format!("{name}.u")));
    constants(&mut model, prefix, count - 1);
    model
}

#[test]
fn the_candidate_cap_is_accepted_at_the_cap_and_skips_the_pass_one_past() {
    let at = compare(
        &candidates("old", "b", MAX_RENAME_CANDIDATES),
        &candidates("new", "h", MAX_RENAME_CANDIDATES),
    );
    assert_eq!(at.notes, [Note::NamesDisjoint]);
    assert_eq!(
        renames(&at),
        [("old".to_owned(), "new".to_owned(), Confidence::Exact)]
    );
    assert_eq!(at.summary.blocks.base_only, MAX_RENAME_CANDIDATES - 1);

    for (base, head) in [
        (MAX_RENAME_CANDIDATES + 1, MAX_RENAME_CANDIDATES),
        (MAX_RENAME_CANDIDATES, MAX_RENAME_CANDIDATES + 1),
    ] {
        let past = compare(&candidates("old", "b", base), &candidates("new", "h", head));
        assert_eq!(past.notes, [Note::NamesDisjoint, Note::RenamePassSkipped]);
        assert!(renames(&past).is_empty());
        assert_eq!(past.summary.blocks.base_only, base);
        assert_eq!(past.summary.blocks.head_only, head);
        assert_eq!(past.summary.graph_equal, at.summary.graph_equal);
    }
}

/// A side with one anchored `CDL.Reals.Abs` named `name`, `spare` more unconnected ones, and
/// `bucket` identical unconnected blocks of another class.
fn bucket(name: &str, prefix: &str, bucket: usize, spare: usize) -> Sequence {
    let mut model = sequence("s");
    model.inputs.push(boundary("u", SignalType::Real));
    model.blocks.push(block(name, "CDL.Reals.Abs", &[]));
    model.connections.push(link("u", &format!("{name}.u")));
    for i in 0..spare {
        model
            .blocks
            .push(block(&format!("{prefix}spare{i}"), "CDL.Reals.Abs", &[]));
    }
    for i in 0..bucket {
        model
            .blocks
            .push(block(&format!("{prefix}{i}"), "CDL.Reals.Sqrt", &[]));
    }
    model
}

#[test]
fn the_pair_cap_is_accepted_at_the_cap_and_skips_the_pass_one_past() {
    // 255 x 257 identical blocks are 65,535 gated pairs; the anchored pair is the 65,536th.
    assert_eq!(255 * 257 + 1, MAX_RENAME_PAIRS);
    let at = compare(&bucket("old", "b", 255, 0), &bucket("new", "h", 257, 0));
    assert_eq!(at.notes, [Note::NamesDisjoint]);
    assert_eq!(
        renames(&at),
        [("old".to_owned(), "new".to_owned(), Confidence::Exact)]
    );
    // One more block in the anchored pair's gate: 2 x 1 pairs there, 65,537 in all.
    let past = compare(&bucket("old", "b", 255, 1), &bucket("new", "h", 257, 0));
    assert_eq!(past.notes, [Note::NamesDisjoint, Note::RenamePassSkipped]);
    assert!(renames(&past).is_empty());
    assert_eq!(past.summary.blocks.base_only, 257);
    assert_eq!(past.summary.blocks.head_only, 258);
    assert_eq!(past.summary.graph_equal, at.summary.graph_equal);
}

#[test]
fn a_cap_is_not_passed_when_one_side_has_nothing_left_to_pair() {
    // Thousands of added blocks and no removed one: there is nothing to rename, and no note
    // says a pass was skipped.
    let mut base = sequence("s");
    base.blocks.push(block("kept", "CDL.Reals.Abs", &[]));
    let mut head = base.clone();
    constants(&mut head, "h", MAX_RENAME_CANDIDATES + 1);
    let diff = compare(&base, &head);
    assert!(diff.notes.is_empty());
    assert_eq!(diff.summary.blocks.head_only, MAX_RENAME_CANDIDATES + 1);
    assert_eq!(diff.summary.blocks.by_name, 1);
}

#[test]
fn graph_equal_is_the_same_with_the_pass_on_off_and_skipped() {
    let pairs = [
        (chain("b", 3), chain("h", 3)),
        (chain("b", 3), chain("b", 3)),
        (chain("b", 12), chain("h", 12)),
        (
            candidates("old", "b", MAX_RENAME_CANDIDATES + 1),
            candidates("new", "h", MAX_RENAME_CANDIDATES + 1),
        ),
    ];
    for (base, head) in &pairs {
        let on = compare(base, head);
        let off = compare_without_renames(base, head);
        assert_eq!(on.summary.graph_equal, off.summary.graph_equal);
        assert_eq!(on.summary.graph_equal, base.blocks == head.blocks);
    }
}
