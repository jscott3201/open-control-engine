//! Interning identity, classification and subject sharing of Step 9 endpoints.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_model::{ConnectorId, Dir, ValueType};

use super::{EndpointId, EndpointLookups, EndpointTable};
use crate::resolve::specialize::Specialization;

struct Fixture<'a> {
    specialization: Specialization,
    boundary_in: HashSet<&'a str>,
    boundary_out: HashSet<&'a str>,
    conn_of_iri: HashMap<&'a str, ConnectorId>,
    connector_dirs: Vec<Dir>,
    by_id: HashMap<&'a str, &'a crate::dto::Node>,
    boundary_types: HashMap<&'a str, Option<ValueType>>,
}

impl<'a> Fixture<'a> {
    fn new() -> Self {
        Self {
            specialization: Specialization::default(),
            boundary_in: HashSet::from(["urn:top:u"]),
            boundary_out: HashSet::from(["urn:top:y"]),
            conn_of_iri: HashMap::from([("urn:child:u", ConnectorId(0))]),
            connector_dirs: vec![Dir::In],
            by_id: HashMap::new(),
            boundary_types: HashMap::from([
                ("urn:top:u", Some(ValueType::Real)),
                ("urn:top:y", None),
            ]),
        }
    }

    fn table(&'a self) -> EndpointTable<'a> {
        EndpointTable::new(EndpointLookups {
            specialization: &self.specialization,
            boundary_in: &self.boundary_in,
            boundary_out: &self.boundary_out,
            conn_of_iri: &self.conn_of_iri,
            connector_dirs: &self.connector_dirs,
            by_id: &self.by_id,
            boundary_types: &self.boundary_types,
        })
    }
}

#[test]
fn byte_equal_iris_share_one_id_whatever_their_allocation() {
    let fixture = Fixture::new();
    let mut table = fixture.table();
    let first = String::from("urn:child:u");
    let second = String::from("urn:child:u");
    let other = String::from("urn:child:v");
    let id = table.intern_source(&first);
    assert_eq!(id, EndpointId(0));
    // The source memo hits on the same slice, and a byte-equal copy reaches the same id by content.
    assert_eq!(table.intern_source(&first), id);
    assert_eq!(table.intern(&second), id);
    assert_eq!(table.intern_source(&second), id);
    assert_eq!(table.intern(&other), EndpointId(1));
    // A memo miss after another source still answers by content, never by the stale memo.
    assert_eq!(table.intern_source(&other), EndpointId(1));
    assert_eq!(table.intern_source(&first), id);
    // The target memo answers the same way.
    assert_eq!(table.intern_target(&first), id);
    assert_eq!(table.intern_target(&first), id);
    assert_eq!(table.intern_target(&second), id);
    assert_eq!(table.intern_target(&other), EndpointId(1));
    assert_eq!(table.intern_target(&first), id);
}

#[test]
fn classification_matches_every_lookup_table() {
    let fixture = Fixture::new();
    let mut table = fixture.table();
    let input = table.intern("urn:top:u");
    let output = table.intern("urn:top:y");
    let child = table.intern("urn:child:u");
    let missing = table.intern("urn:nowhere");

    let input = table.get(input);
    assert!(input.boundary_in && !input.boundary_out && !input.inactive);
    assert_eq!(input.boundary_type, Some(ValueType::Real));
    assert_eq!((input.connector, input.dir), (None, None));

    let output = table.get(output);
    assert!(!output.boundary_in && output.boundary_out);
    assert_eq!(
        output.boundary_type, None,
        "a failed type derivation stays None"
    );

    let child = table.get(child);
    assert_eq!(
        (child.connector, child.dir),
        (Some(ConnectorId(0)), Some(Dir::In))
    );
    assert!(!child.boundary_in && !child.boundary_out);

    let missing = table.get(missing);
    assert!(missing.connector.is_none() && missing.node.is_none());
    assert_eq!(
        table.boundary_inputs_read(),
        HashSet::from(["urn:top:u"]),
        "only interned boundary inputs count as read"
    );
}

#[test]
fn subjects_are_minted_once_per_endpoint_and_shared() {
    let fixture = Fixture::new();
    let mut table = fixture.table();
    let long = format!("urn:child:{}", "x".repeat(4096));
    let id = table.intern_source(&long);
    let first = table.subject(id);
    let second = table.subject(id);
    assert!(
        Arc::ptr_eq(&first, &second),
        "a repeated subject must not copy the IRI"
    );
    assert_eq!(&*first, long.as_str());
}

/// Run Step 9 over `edges` against the fixture and render everything it produced.
fn collect_rendered(fixture: &Fixture<'_>, edges: &[(&str, &str)]) -> String {
    let lookups = EndpointLookups {
        specialization: &fixture.specialization,
        boundary_in: &fixture.boundary_in,
        boundary_out: &fixture.boundary_out,
        conn_of_iri: &fixture.conn_of_iri,
        connector_dirs: &fixture.connector_dirs,
        by_id: &fixture.by_id,
        boundary_types: &fixture.boundary_types,
    };
    let mut connectors = vec![oce_model::Connector::new(
        ConnectorId(0),
        oce_model::BlockId(0),
        Dir::In,
        ValueType::Real,
        0,
    )];
    let mut diags = Vec::new();
    let collected = crate::resolve::connections::collect(
        edges.iter().copied(),
        lookups,
        &mut connectors,
        &mut diags,
    );
    let mut drivers: Vec<_> = collected
        .boundary_output_drivers
        .iter()
        .map(|(output, set)| {
            let mut set: Vec<_> = set.iter().cloned().collect();
            set.sort();
            (output.clone(), set)
        })
        .collect();
    drivers.sort();
    let mut sources: Vec<_> = collected.boundary_output_sources.into_iter().collect();
    sources.sort();
    let mut read: Vec<_> = collected.boundary_inputs_read.into_iter().collect();
    read.sort();
    format!(
        "{diags:?}\n{:?}\n{:?}\n{:?}\n{drivers:?}\n{sources:?}\n{read:?}\n{:?}",
        collected.connections,
        collected.external_inputs,
        collected.pass_through_pairs,
        connectors[0].iri,
    )
}

/// The same edge list built from one shared source allocation and from a separate allocation per
/// edge collects byte-identical output: the slice-identity memo never decides a result.
#[test]
fn shared_and_separate_source_allocations_collect_identically() {
    let fixture = Fixture::new();
    // One long missing source per arm (the report-once set spans arms), each fanned out twice so
    // the shared build exercises the source memo.
    let [leaf, output, input] =
        ["leaf", "output", "input"].map(|arm| format!("urn:missing:{arm}:{}", "x".repeat(4096)));
    let shape: [(&str, &str); 10] = [
        (&leaf, "urn:child:u"),
        (&leaf, "urn:child:u"),
        (&output, "urn:top:y"),
        (&output, "urn:top:y"),
        (&input, "urn:top:u"),
        (&input, "urn:top:u"),
        ("urn:top:u", "urn:child:u"),
        ("urn:top:u", "urn:child:u"),
        ("urn:top:u", "urn:top:y"),
        ("urn:top:u", "urn:top:y"),
    ];
    let shared = collect_rendered(&fixture, &shape);

    let owned: Vec<(String, String)> = shape
        .iter()
        .map(|(source, target)| ((*source).to_owned(), (*target).to_owned()))
        .collect();
    let separate: Vec<(&str, &str)> = owned
        .iter()
        .map(|(source, target)| (source.as_str(), target.as_str()))
        .collect();
    assert!(
        !std::ptr::eq(separate[0].0, separate[1].0),
        "each edge owns its source"
    );
    let separate = collect_rendered(&fixture, &separate);

    assert_eq!(shared, separate);
    for expected in [
        "connection source not found",
        "boundary-output source not found",
        "boundary-input target not found",
        "boundary-input:urn:top:u",
    ] {
        assert!(shared.contains(expected), "missing {expected}: {shared}");
    }
    assert_eq!(shared.matches("not found").count(), 3, "{shared}");
}

/// The same edge list built with one shared allocation per repeated target (as lowering hands a
/// swapped fan-out over) and with a separate allocation per edge collects byte-identical output:
/// the target memo never decides a result (GitHub issue #480).
#[test]
fn shared_and_separate_target_allocations_collect_identically() {
    let fixture = Fixture::new();
    let [missing, other] = ["target", "other"]
        .map(|name| Arc::<str>::from(format!("urn:missing:{name}:{}", "x".repeat(4096))));
    let shape: Vec<(&str, Arc<str>)> = vec![
        ("urn:top:u", Arc::clone(&missing)),
        ("urn:top:u", Arc::clone(&missing)),
        ("urn:top:u", Arc::from("urn:child:u")),
        ("urn:top:u", Arc::clone(&missing)),
        ("urn:child:u", Arc::clone(&other)),
        ("urn:child:u", Arc::clone(&other)),
        ("urn:top:u", Arc::from("urn:top:y")),
        ("urn:top:u", Arc::from("urn:top:y")),
    ];
    let shared: Vec<(&str, &str)> = shape
        .iter()
        .map(|(source, target)| (*source, &**target))
        .collect();
    assert!(
        std::ptr::eq(shared[0].1, shared[1].1),
        "a run shares its target"
    );
    let shared = collect_rendered(&fixture, &shared);

    let owned: Vec<(&str, String)> = shape
        .iter()
        .map(|(source, target)| (*source, target.to_string()))
        .collect();
    let separate: Vec<(&str, &str)> = owned
        .iter()
        .map(|(source, target)| (*source, target.as_str()))
        .collect();
    assert!(
        !std::ptr::eq(separate[0].1, separate[1].1),
        "each edge owns its target"
    );
    let separate = collect_rendered(&fixture, &separate);

    assert_eq!(shared, separate);
    assert_eq!(shared.matches("not found").count(), 2, "{shared}");
}
