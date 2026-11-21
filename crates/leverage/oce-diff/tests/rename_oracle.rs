//! An oracle for the rename pass: a second implementation of the diff note's "Identity" rules,
//! written pair by pair exactly as the note states them, against the crate's own, which never
//! visits a pair (it groups blocks by what they see). The two share no code and no data
//! structure, and must agree on every pair of two enumerated families of small models, no random
//! number: 256 wirings of each side (65,536 pairs), and 64 sides a validator may refuse, with a
//! boundary declared on one side only or a link to a block nobody declared (4,096 pairs).
//!
//! The reference is quadratic in the unmatched blocks and reads every link for every pair in
//! every round. That is what makes it easy to check against the note by eye, and what the
//! crate's implementation is measured against in `src/work_tests.rs`.

mod support;

use std::collections::BTreeMap;

use oce_author::{Endpoint, Sequence, SignalType};
use oce_diff::{Confidence, GraphChange, MAX_RENAME_ROUNDS};

use support::{block, boundary, compare, link, sequence};

/// One end of a link, as the reference reads it through the current matching.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Seen {
    /// A boundary declared on both sides, or a port of a matched block in head names.
    Anchor(String),
    /// A port of a block that is not matched yet.
    Unmatched,
    /// Anything else: it can never agree with anything.
    Never,
}

/// The reference's view of one side.
struct Side<'a> {
    model: &'a Sequence,
    /// For a matched block of this side, the head block's name.
    matched: BTreeMap<String, String>,
}

impl Side<'_> {
    fn declares(&self, name: &str) -> bool {
        self.model
            .inputs
            .iter()
            .chain(&self.model.outputs)
            .any(|boundary| boundary.name.as_str() == name)
    }

    fn has_block(&self, name: &str) -> bool {
        self.model
            .blocks
            .iter()
            .any(|block| block.name.as_str() == name)
    }

    fn see(&self, end: &Endpoint, other: &Side<'_>) -> Seen {
        match end {
            Endpoint::Boundary(name) => {
                if self.declares(name.as_str()) && other.declares(name.as_str()) {
                    Seen::Anchor(name.as_str().to_owned())
                } else {
                    Seen::Never
                }
            }
            Endpoint::Port { block, port } => match self.matched.get(block.as_str()) {
                Some(head) => Seen::Anchor(format!("{head}.{port}")),
                None if self.has_block(block.as_str()) => Seen::Unmatched,
                None => Seen::Never,
            },
            _ => Seen::Never,
        }
    }

    /// What the block `name` sees at each of its ports: `(is an output, port)` to the sorted
    /// ends on the other side of its links.
    fn ports(&self, name: &str, other: &Side<'_>) -> BTreeMap<(bool, String), Vec<Seen>> {
        let mut ports: BTreeMap<(bool, String), Vec<Seen>> = BTreeMap::new();
        for connection in &self.model.connections {
            if let Endpoint::Port { block, port } = &connection.target
                && block.as_str() == name
            {
                ports
                    .entry((false, port.as_str().to_owned()))
                    .or_default()
                    .push(self.see(&connection.source, other));
            }
            if let Endpoint::Port { block, port } = &connection.source
                && block.as_str() == name
            {
                ports
                    .entry((true, port.as_str().to_owned()))
                    .or_default()
                    .push(self.see(&connection.target, other));
            }
        }
        for ends in ports.values_mut() {
            ends.sort();
        }
        ports
    }
}

/// How one port of a pair compares.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Port {
    Agrees,
    Disagrees,
    Unknown,
}

/// The note's rule for one port, with the three cases it reads as disagreement.
fn port(old: Option<&Vec<Seen>>, new: Option<&Vec<Seen>>) -> Port {
    let (Some(old), Some(new)) = (old, new) else {
        return Port::Disagrees; // linked on one side only
    };
    if old.len() != new.len() || old.iter().chain(new).any(|seen| *seen == Seen::Never) {
        return Port::Disagrees;
    }
    let anchors = |ends: &Vec<Seen>| -> Vec<Seen> {
        ends.iter()
            .filter(|seen| matches!(seen, Seen::Anchor(_)))
            .cloned()
            .collect()
    };
    if anchors(old) != anchors(new) {
        return Port::Disagrees; // at least one is an anchor and they differ
    }
    if old.contains(&Seen::Unmatched) {
        Port::Unknown // what is left on both sides are unmatched blocks
    } else {
        Port::Agrees
    }
}

/// Every port of the pair, compared.
fn compared(base: &Side<'_>, head: &Side<'_>, b: &str, h: &str) -> Vec<Port> {
    let (old, new) = (base.ports(b, head), head.ports(h, base));
    let mut keys: Vec<&(bool, String)> = old.keys().chain(new.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .map(|key| port(old.get(key), new.get(key)))
        .collect()
}

/// The hard gates: the same class and the same assignments, order ignored.
fn gated(base: &Sequence, head: &Sequence, b: &str, h: &str) -> bool {
    let find = |model: &Sequence, name: &str| {
        model
            .blocks
            .iter()
            .find(|block| block.name.as_str() == name)
            .cloned()
    };
    let (Some(old), Some(new)) = (find(base, b), find(head, h)) else {
        return false;
    };
    let assignments = |block: &oce_author::Block| -> Vec<String> {
        let mut list: Vec<String> = block
            .assignments
            .iter()
            .map(|assignment| format!("{} = {:?}", assignment.name, assignment.value))
            .collect();
        list.sort();
        list
    };
    old.class_path == new.class_path && assignments(&old) == assignments(&new)
}

/// The reference: the note's three steps, pair by pair. Returns `(from, to, confidence)` sorted.
fn reference(base: &Sequence, head: &Sequence) -> Vec<(String, String, Confidence)> {
    let class_of = |model: &Sequence, name: &str| {
        model
            .blocks
            .iter()
            .find(|block| block.name.as_str() == name)
            .map(|block| block.class_path.clone())
    };
    let names = |model: &Sequence| -> Vec<String> {
        model
            .blocks
            .iter()
            .map(|block| block.name.as_str().to_owned())
            .collect()
    };
    // Steps 1 and 2: the same name and class is the same block.
    let mut old = Side {
        model: base,
        matched: BTreeMap::new(),
    };
    let mut new = Side {
        model: head,
        matched: BTreeMap::new(),
    };
    for name in names(base) {
        if class_of(base, &name).is_some() && class_of(base, &name) == class_of(head, &name) {
            old.matched.insert(name.clone(), name.clone());
            new.matched.insert(name.clone(), name);
        }
    }
    // Step 3: rounds over what is left.
    let mut renamed: Vec<(String, String)> = Vec::new();
    for _ in 0..MAX_RENAME_ROUNDS {
        let loose_old: Vec<String> = names(base)
            .into_iter()
            .filter(|name| !old.matched.contains_key(name))
            .collect();
        let loose_new: Vec<String> = names(head)
            .into_iter()
            .filter(|name| !new.matched.contains_key(name))
            .collect();
        let mut candidates: Vec<(String, String)> = Vec::new();
        for b in &loose_old {
            for h in &loose_new {
                if !gated(base, head, b, h) {
                    continue;
                }
                let ports = compared(&old, &new, b, h);
                if !ports.contains(&Port::Disagrees) && ports.contains(&Port::Agrees) {
                    candidates.push((b.clone(), h.clone()));
                }
            }
        }
        let accepted: Vec<(String, String)> = candidates
            .iter()
            .filter(|(b, h)| {
                candidates.iter().filter(|(other, _)| other == b).count() == 1
                    && candidates.iter().filter(|(_, other)| other == h).count() == 1
            })
            .cloned()
            .collect();
        if accepted.is_empty() {
            break;
        }
        for (b, h) in accepted {
            old.matched.insert(b.clone(), h.clone());
            new.matched.insert(h.clone(), h.clone());
            renamed.push((b, h));
        }
    }
    let mut result: Vec<(String, String, Confidence)> = renamed
        .into_iter()
        .map(|(b, h)| {
            let exact = compared(&old, &new, &b, &h)
                .iter()
                .all(|port| *port == Port::Agrees);
            let confidence = if exact {
                Confidence::Exact
            } else {
                Confidence::High
            };
            (b, h, confidence)
        })
        .collect();
    result.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    result
}

/// What the crate reports, in the reference's shape.
fn reported(base: &Sequence, head: &Sequence) -> Vec<(String, String, Confidence)> {
    let mut result: Vec<(String, String, Confidence)> = compare(base, head)
        .graph
        .into_iter()
        .filter_map(|change| match change {
            GraphChange::Renamed {
                subject,
                from,
                confidence,
                ..
            } => Some((from, subject, confidence)),
            _ => None,
        })
        .collect();
    result.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    result
}

/// The link slots of the family, with `x1`, `x2`, `x3` standing for a side's three blocks.
const SLOTS: [(&str, &str); 8] = [
    ("u", "x1.u"),
    ("u", "x2.u"),
    ("x1.y", "x2.u"),
    ("x2.y", "x3.u"),
    ("x1.y", "y"),
    ("x1.y", "m.u"),
    ("m.y", "x3.u"),
    // A second unmatched end at one port: `x1.y` can now drive two blocks no round has matched.
    ("x1.y", "x3.u"),
];

/// A side of the family: two blocks of one class and one of another, named `names` for `x1`,
/// `x2` and `x3`, a block `m` both sides share, and the links whose bit is set in `wiring`.
fn member(names: [&str; 3], wiring: usize) -> Sequence {
    let mut model = sequence("s");
    model.inputs.push(boundary("u", SignalType::Real));
    model.outputs.push(boundary("y", SignalType::Real));
    model.blocks.push(block(names[0], "CDL.Reals.Abs", &[]));
    model.blocks.push(block(names[1], "CDL.Reals.Abs", &[]));
    model.blocks.push(block(names[2], "CDL.Reals.Sqrt", &[]));
    model.blocks.push(block("m", "CDL.Reals.Abs", &[]));
    for (slot, (source, target)) in SLOTS.iter().enumerate() {
        if wiring & (1 << slot) != 0 {
            let named = |end: &str| {
                end.replace("x1", names[0])
                    .replace("x2", names[1])
                    .replace("x3", names[2])
            };
            model.connections.push(link(&named(source), &named(target)));
        }
    }
    model
}

/// Compare the crate with the reference on every pair of wirings of the family, the head's
/// blocks named `head_names`. Returns how many pairs have a rename, a `high` rename, and all
/// three blocks matched.
fn agree_on_the_family(head_names: [&str; 3]) -> (usize, usize, usize) {
    let wirings = 1 << SLOTS.len();
    let bases: Vec<Sequence> = (0..wirings)
        .map(|wiring| member(["b1", "b2", "b3"], wiring))
        .collect();
    let heads: Vec<Sequence> = (0..wirings)
        .map(|wiring| member(head_names, wiring))
        .collect();
    let (mut with_a_rename, mut with_high, mut with_three) = (0, 0, 0);
    for (b, base) in bases.iter().enumerate() {
        for (h, head) in heads.iter().enumerate() {
            let expected = reference(base, head);
            assert_eq!(
                reported(base, head),
                expected,
                "head named {head_names:?}: base wiring {b:#010b}, head wiring {h:#010b}"
            );
            with_a_rename += usize::from(!expected.is_empty());
            with_high += usize::from(
                expected
                    .iter()
                    .any(|(_, _, confidence)| *confidence == Confidence::High),
            );
            with_three += usize::from(expected.len() == 3);
        }
    }
    (with_a_rename, with_high, with_three)
}

#[test]
fn the_crate_and_the_notes_rules_pair_by_pair_agree_on_every_small_model() {
    // The family is not vacuous: it holds renames, partly seen ones, and whole matches. The
    // counts are a function of the family alone, so they are pinned.
    assert_eq!(
        agree_on_the_family(["h1", "h2", "h3"]),
        (5_478, 2_638, 94),
        "pairs with a rename, with a `high` rename, and with all three blocks matched"
    );
}

#[test]
fn the_names_a_side_gives_its_blocks_do_not_matter() {
    // The head's blocks named in the opposite order: what matches is the same, so the counts
    // are, and the ends at one port now sort differently on the two sides.
    assert_eq!(agree_on_the_family(["h3", "h2", "h1"]), (5_478, 2_638, 94));
}

/// A side a validator may refuse: `u` and `y` each declared or not, and four links, one of them
/// from a block nobody declared.
fn refused(prefix: &str, variant: usize) -> Sequence {
    const LINKS: [(&str, &str); 4] = [
        ("u", "x1.u"),
        ("x1.y", "y"),
        ("ghost.y", "x1.u2"),
        ("x1.y", "x2.u"),
    ];
    let mut model = sequence("s");
    if variant & 1 != 0 {
        model.inputs.push(boundary("u", SignalType::Real));
    }
    if variant & 2 != 0 {
        model.outputs.push(boundary("y", SignalType::Real));
    }
    model
        .blocks
        .push(block(&format!("{prefix}1"), "CDL.Reals.Abs", &[]));
    model
        .blocks
        .push(block(&format!("{prefix}2"), "CDL.Reals.Sqrt", &[]));
    for (slot, (source, target)) in LINKS.iter().enumerate() {
        if variant & (4 << slot) != 0 {
            let named = |end: &str| end.replace('x', prefix);
            model.connections.push(link(&named(source), &named(target)));
        }
    }
    model
}

#[test]
fn the_crate_and_the_notes_rules_agree_on_models_a_validator_refuses() {
    let bases: Vec<Sequence> = (0..64).map(|variant| refused("b", variant)).collect();
    let heads: Vec<Sequence> = (0..64).map(|variant| refused("h", variant)).collect();
    let (mut with_a_rename, mut refused_for_a_boundary) = (0, 0);
    for (b, base) in bases.iter().enumerate() {
        for (h, head) in heads.iter().enumerate() {
            let expected = reference(base, head);
            assert_eq!(
                reported(base, head),
                expected,
                "base variant {b:#08b}, head variant {h:#08b}"
            );
            with_a_rename += usize::from(!expected.is_empty());
            // The same wiring with `u` declared on both sides, where this pair has it on one.
            if (b ^ h) & 1 != 0 && expected.is_empty() {
                let both = reference(&refused("b", b | 1), &refused("h", h | 1));
                refused_for_a_boundary += usize::from(!both.is_empty());
            }
        }
    }
    assert_eq!(
        (with_a_rename, refused_for_a_boundary),
        (14, 20),
        "pairs with a rename, and pairs refused only because one side does not declare `u`"
    );
}

#[test]
fn the_reference_reads_the_notes_example_c_as_the_note_does() {
    // The reference itself is held to the note: example (c), round by round.
    let (base, head) = support::example("plant_reset");
    assert_eq!(
        reference(&base, &head),
        [
            ("THigh".to_owned(), "TMax".to_owned(), Confidence::Exact),
            ("TLow".to_owned(), "TMin".to_owned(), Confidence::Exact),
            ("sel".to_owned(), "sw".to_owned(), Confidence::High)
        ]
    );
    let (base, head) = support::example("pressure_request");
    assert_eq!(
        reference(&base, &head),
        [(
            "seventySet".to_owned(),
            "setFrac".to_owned(),
            Confidence::Exact
        )]
    );
}
