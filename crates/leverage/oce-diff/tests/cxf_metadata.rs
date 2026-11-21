//! The one metadata fact a topology holds: its root (owner ruling of 2026-11-21 on the GitHub
//! #715 review; issue #666).
//!
//! Every name at the CXF level is stripped of its root, so a renamed root is no graph entry. It
//! is reported as the authoring level reports it, a `sequence-renamed` metadata entry, so the
//! diff is not `equal` and `oce diff` exits 1 for it, as for an authoring rename. Nothing else of
//! the metadata is in a topology, so `metadata_equal` is `false` beside that entry and `null`
//! (unknown) without it, and the strict reader accepts exactly those two shapes.

mod support;

use oce_diff::{Diff, MetadataChange, render_markdown, render_text};
use serde_json::json;

use support::loaded::{compare_loaded, loaded};
use support::parse;

/// A gain between an input and an output, in a sequence named `name`.
fn named(name: &str) -> support::loaded::Loaded {
    loaded(&parse(&format!(
        "sequence {name}\nderived_from none\n\nin u : Real\nout y : Real\n\n\
         block g = CDL.Reals.MultiplyByParameter(k = 2.0)\n\nu -> g.u\ng.y -> y\n"
    )))
}

#[test]
fn a_renamed_root_moves_the_id_and_is_a_sequence_renamed_entry() {
    let (base, head) = (named("s"), named("t"));
    assert_ne!(
        base.id, head.id,
        "the root's name is inside every name the id covers"
    );
    let diff = compare_loaded(&base, &head);
    assert_eq!(
        diff.metadata,
        [MetadataChange::SequenceRenamed {
            subject: "sequence".to_owned(),
            old: Some("s".to_owned()),
            new: Some("t".to_owned()),
        }]
    );
    assert!(!diff.summary.equal);
    assert_eq!(diff.summary.metadata_equal, Some(false));
    assert!(diff.summary.graph_equal && diff.summary.interface_equal);
    assert!(diff.graph.is_empty() && diff.interface.is_empty());
    assert_eq!(
        (diff.base.sequence.as_str(), diff.head.sequence.as_str()),
        ("s", "t")
    );
    assert_eq!(diff.base.derived_from, None);
    assert_eq!(
        render_text(&diff),
        "# oce diff s -> t\n\
         # graph equal; parameters not separable; interface equal; metadata differs\n\
         # metadata\n\
         ~ sequence t   # was s\n"
    );
    assert!(render_markdown(&diff).contains("| sequence-renamed |"));
    // The other way round, and the same name: no entry, and the flag is unknown.
    let back = compare_loaded(&head, &base);
    assert_eq!(support::json(&back)["metadata"][0]["old"], json!("t"));
    let same = compare_loaded(&base, &named("s"));
    assert!(same.summary.equal && same.metadata.is_empty());
    assert_eq!(same.summary.metadata_equal, None);
}

#[test]
fn the_strict_reader_takes_the_metadata_flag_a_cxf_diff_writes_and_no_other() {
    let renamed = compare_loaded(&named("s"), &named("t"));
    let same = compare_loaded(&named("s"), &named("s"));
    for diff in [&renamed, &same] {
        let read: Diff = serde_json::from_str(&diff.to_json()).expect("reads back");
        assert_eq!(&read, diff);
    }
    let read = |mut value: serde_json::Value, flag: serde_json::Value| {
        value["summary"]["metadata_equal"] = flag;
        serde_json::from_value::<Diff>(value)
    };
    let renamed = support::json(&renamed);
    let same = support::json(&same);
    // Beside the entry: `false` only.
    assert!(read(renamed.clone(), json!(false)).is_ok());
    for flag in [json!(null), json!(true)] {
        let error = read(renamed.clone(), flag.clone()).expect_err("refused");
        assert!(
            error.to_string().contains("metadata_equal"),
            "{flag}: {error}"
        );
    }
    // Without it: `null` only. `true` is what an authoring diff writes and is refused here.
    assert!(read(same.clone(), json!(null)).is_ok());
    for flag in [json!(false), json!(true)] {
        let error = read(same.clone(), flag.clone()).expect_err("refused");
        assert!(
            error.to_string().contains("metadata_equal"),
            "{flag}: {error}"
        );
    }
    // `parameters_equal` is never known at this level.
    let mut known = same.clone();
    known["summary"]["parameters_equal"] = json!(true);
    assert!(serde_json::from_value::<Diff>(known).is_err());
    // The only metadata entry this level writes is a renamed root.
    let mut comment = renamed.clone();
    comment["metadata"] = json!([{ "kind": "comment-changed", "subject": "sequence",
                                   "old": null, "new": " x" }]);
    assert!(serde_json::from_value::<Diff>(comment).is_err());
    // An authoring diff is held to the sections only: `true` beside an empty metadata section.
    let mut authoring = same;
    authoring["source"] = json!("authoring");
    authoring["summary"]["parameters_equal"] = json!(true);
    authoring["summary"]["metadata_equal"] = json!(true);
    assert!(serde_json::from_value::<Diff>(authoring).is_ok());
}
