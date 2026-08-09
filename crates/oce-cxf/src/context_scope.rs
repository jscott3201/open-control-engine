//! Rejection of node-scoped JSON-LD contexts before identity expansion.

use oce_diag::{DiagCode, Diagnostic};

use crate::dto::CxfDocument;

pub(super) fn collect_refusals(doc: &CxfDocument, diags: &mut Vec<Diagnostic>) {
    for (index, node) in doc.graph.iter().enumerate() {
        let (subject, owner) = if node.id.is_empty() {
            let subject = format!("@graph[{index}]");
            (subject.clone(), format!("node at `{subject}`"))
        } else {
            (node.id.clone(), format!("node `{}`", node.id))
        };
        let mut refuse = |location: String| {
            diags.push(
                Diagnostic::error(
                    DiagCode::NonSubsetConstruct,
                    format!(
                        "{location} declares a scoped `@context`; node-scoped contexts are not \
                         supported; declare required bindings in the document-level `@context`"
                    ),
                )
                .with_subject(subject.clone()),
            );
        };
        if node.other.contains_key("@context") {
            refuse(owner.clone());
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
                    refuse(format!("{slot} reference `{}` on {owner}", reference.id));
                }
            }
        }
        if let Some(reference) = &node.is_of_data_type
            && reference.other.contains_key("@context")
        {
            refuse(format!(
                "S231:isOfDataType reference `{}` on {owner}",
                reference.id
            ));
        }
    }
}
