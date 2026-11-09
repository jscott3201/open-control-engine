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
