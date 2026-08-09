//! Rejection of node-scoped JSON-LD contexts before identity expansion.

use std::{collections::HashMap, sync::Arc};

use oce_diag::{DiagCode, Diagnostic};

use crate::dto::{CxfDocument, CxfValue, TermAttr};

pub(super) fn collect_refusals(doc: &CxfDocument, diags: &mut Vec<Diagnostic>) {
    let mut subjects = HashMap::<&str, Arc<str>>::new();
    for (index, node) in doc.graph.iter().enumerate() {
        let mut subject = None;
        let mut refuse = |location: String| {
            let subject = Arc::clone(subject.get_or_insert_with(|| {
                if node.id.is_empty() {
                    Arc::from(format!("@graph[{index}]"))
                } else {
                    Arc::clone(
                        subjects
                            .entry(node.id.as_str())
                            .or_insert_with(|| Arc::from(node.id.as_str())),
                    )
                }
            }));
            diags.push(
                Diagnostic::error(
                    DiagCode::NonSubsetConstruct,
                    format!(
                        "{location} declares a scoped `@context`; node-scoped contexts are not \
                         supported; declare required bindings in the document-level `@context`"
                    ),
                )
                .with_subject(subject),
            );
        };
        if node.other.contains_key("@context") {
            let owner = if node.id.is_empty() {
                format!("node at `@graph[{index}]`")
            } else {
                format!("node `{}`", node.id)
            };
            refuse(owner);
        }
        for (slot, references) in [
            ("S231:hasInput", node.has_input.as_slice()),
            ("S231:hasOutput", node.has_output.as_slice()),
            ("S231:hasParameter", node.has_parameter.as_slice()),
            ("S231:hasConstant", node.has_constant.as_slice()),
            ("S231:containsBlock", node.contains_block.as_slice()),
            ("S231:hasInstance", node.has_instance.as_slice()),
            ("S231:isConnectedTo", node.is_connected_to.as_slice()),
        ] {
            for reference in references {
                if reference.other.contains_key("@context") {
                    refuse(format!("{slot} reference `{}`", reference.id));
                }
            }
        }
        if let Some(reference) = &node.is_of_data_type
            && reference.other.contains_key("@context")
        {
            refuse(format!("S231:isOfDataType reference `{}`", reference.id));
        }
        for (slot, value) in [
            ("S231:value", node.value.as_ref()),
            ("S231:min", node.min.as_ref()),
            ("S231:max", node.max.as_ref()),
        ] {
            if let Some(value) = value {
                collect_value_refusals(value, slot, &mut refuse);
            }
        }
        for (slot, term) in [
            ("S231:unit", node.unit.as_ref()),
            ("S231:quantity", node.quantity.as_ref()),
            ("S231:displayUnit", node.display_unit.as_ref()),
        ] {
            if term.is_some_and(term_declares_context) {
                refuse(format!("{slot} term"));
            }
        }
    }
}

fn collect_value_refusals(value: &CxfValue, slot: &str, refuse: &mut impl FnMut(String)) {
    match value {
        CxfValue::Typed { extra, .. } if extra.contains_key("@context") => {
            refuse(format!("{slot} typed literal"));
        }
        CxfValue::List(values) => {
            for value in values {
                collect_value_refusals(value, slot, refuse);
            }
        }
        _ => {}
    }
}

fn term_declares_context(term: &TermAttr) -> bool {
    match term {
        TermAttr::Typed { extra, .. } | TermAttr::Iri { extra, .. } => {
            extra.contains_key("@context")
        }
        TermAttr::Other(serde_json::Value::Object(value)) => value.contains_key("@context"),
        TermAttr::Bare(_) | TermAttr::Other(_) => false,
    }
}
