//! Each lookup answers what the search it replaced answered, for shared and separate allocations.

use std::sync::Arc;

use oce_model::{BlockId, Connector, ConnectorId, Dir, ModelGraph, ValueType};

use super::{BoundaryNodes, boundary_driven, targeted_iris};

fn input(id: u32, iri: Option<Arc<str>>) -> Connector {
    let mut connector = Connector::new(ConnectorId(id), BlockId(0), Dir::In, ValueType::Real, id);
    connector.iri = iri;
    connector
}

/// A graph whose boundary-driven inputs carry `a` shared, `b` as two separate allocations, one
/// with no IRI, and an id past the arena.
fn graph() -> ModelGraph {
    let a: Arc<str> = Arc::from("urn:x#a");
    let connectors = vec![
        input(0, Some(Arc::clone(&a))),
        input(1, Some(Arc::clone(&a))),
        input(2, Some(Arc::from("urn:x#b"))),
        input(3, Some(Arc::from("urn:x#b"))),
        input(4, None),
        input(5, Some(Arc::from("urn:x#c"))),
    ];
    ModelGraph {
        connectors,
        external_inputs: [0, 1, 2, 3, 4, 9, 1].map(ConnectorId).to_vec(),
        ..ModelGraph::default()
    }
}

#[test]
fn the_driven_set_answers_as_the_list_search_did() {
    let g = graph();
    let set = boundary_driven(&g);
    for id in 0..12 {
        assert_eq!(
            set.contains(&id),
            g.external_inputs.contains(&ConnectorId(id)),
            "{id}"
        );
    }
}

#[test]
fn the_targeted_iris_answer_as_the_connector_search_did() {
    let g = graph();
    let iris = targeted_iris(&g);
    for iri in ["urn:x#a", "urn:x#b", "urn:x#c", "urn:x#", "", "urn:x#ab"] {
        let searched = g.external_inputs.iter().any(|id| {
            g.connectors
                .get(id.0 as usize)
                .and_then(|connector| connector.iri.as_deref())
                == Some(iri)
        });
        assert_eq!(iris.contains(iri), searched, "{iri}");
    }
    assert_eq!(iris.len(), 2);
}

#[test]
fn a_boundary_node_is_found_by_its_text_whatever_allocation_names_it() {
    let shared: Arc<str> = Arc::from("urn:x#a");
    let separate: Arc<str> = Arc::from("urn:x#a");
    let other: Arc<str> = Arc::from("urn:x#b");
    // A view of the same allocation with a different length is a different text.
    let prefix = &shared[..5];
    let mut nodes = BoundaryNodes::default();
    assert_eq!(nodes.find(&shared), None);
    nodes.add(&shared, 0);
    assert_eq!(nodes.find(&shared), Some(0));
    assert_eq!(nodes.find(&separate), Some(0));
    assert_eq!(nodes.find(prefix), None);
    assert_eq!(nodes.find(&other), None);
    nodes.add(&other, 1);
    // The last answer is for `other`; the earlier node is still found, by either allocation.
    assert_eq!(nodes.find(&shared), Some(0));
    // Asked right after the whole allocation, a view of its start is still another text.
    assert_eq!(nodes.find(prefix), None);
    assert_eq!(nodes.find(&other), Some(1));
    assert_eq!(nodes.find(&separate), Some(0));
}
