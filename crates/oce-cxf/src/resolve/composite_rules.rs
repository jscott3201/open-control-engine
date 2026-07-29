//! CXF-lowering-subset contract-rule identities and the published rule-catalog generator.
//!
//! Every composite-subset CONTRACT rejection emitted by `composite.rs` carries a stable,
//! machine-readable rule identity by convention, without extending the shared
//! [`oce_diag::Diagnostic`] struct: the rejection message begins with the `composite/<rule-id>: `
//! prefix rendered by [`CompositeRule::message`], and the checked-in catalog
//! `tools/reference-catalog/oce-cxf.composite-rules.json` maps each rule id to its
//! [`DiagCode`] string, message prefix, and one-line summary. Emission sites and the catalog
//! generator both read the rule constants below — one table, so tag and catalog cannot drift.
//! The regenerate-and-diff byte golden in `composite_rules_tests.rs` keeps the checked-in
//! artifact aligned with this table.
//!
//! Boundary: the generic lookup-miss (`UnresolvedReference`) and grounding (`GroundingFailed`)
//! diagnostics in `composite.rs` are shared machinery, not contract rules — they stay untagged.
//!
//! Determinism contract: catalog entries are written in `COMPOSITE_RULES` slice order, object
//! fields in fixed source order, and every string serializes through `serde_json`. Repeated
//! generation is byte-identical; the checked-in artifact is the byte golden.

use oce_diag::DiagCode;

/// One composite-subset contract rule: its stable id (the `composite/<id>: ` tag), the
/// [`DiagCode`] its rejections carry, and a one-line summary for the published catalog.
pub(super) struct CompositeRule {
    /// Stable kebab-case rule id; `composite/<id>: ` is the machine-readable message prefix.
    pub(super) id: &'static str,
    /// The diagnostic code every rejection under this rule carries.
    pub(super) code: DiagCode,
    /// One-line rule summary, published in the catalog artifact only.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) summary: &'static str,
}

impl CompositeRule {
    /// Render the tagged rejection message: the `composite/<rule-id>: ` prefix followed by the
    /// human-readable detail. The prefix derives from [`CompositeRule::id`] — the same field the
    /// catalog generator publishes — so emitted tags and the catalog share one identity source.
    pub(super) fn message(&self, detail: impl std::fmt::Display) -> String {
        format!("composite/{}: {detail}", self.id)
    }

    /// The exact `composite/<rule-id>: ` prefix string published in the catalog.
    #[cfg(test)]
    pub(super) fn message_prefix(&self) -> String {
        format!("composite/{}: ", self.id)
    }
}

/// `composite/root-count`: nested classification must select exactly one top composite root.
pub(super) const ROOT_COUNT: CompositeRule = CompositeRule {
    id: "root-count",
    code: DiagCode::MalformedDocument,
    summary: "nested classification must select exactly one top composite containsBlock root",
};

/// `composite/contains-cycle`: the nested composite `containsBlock` graph must be acyclic.
pub(super) const CONTAINS_CYCLE: CompositeRule = CompositeRule {
    id: "contains-cycle",
    code: DiagCode::MalformedDocument,
    summary: "the nested composite containsBlock graph must be acyclic",
};

/// `composite/replaceable`: `isReplaceable=true` must be resolved before import.
pub(super) const REPLACEABLE: CompositeRule = CompositeRule {
    id: "replaceable",
    code: DiagCode::UnresolvedPolymorphism,
    summary: "replaceable CXF components must be resolved before import",
};

/// `composite/banned-modelica-key`: banned Modelica construct keys must not survive lowering.
pub(super) const BANNED_MODELICA_KEY: CompositeRule = CompositeRule {
    id: "banned-modelica-key",
    code: DiagCode::NonSubsetConstruct,
    summary: "banned Modelica construct keys must not survive CXF lowering",
};

/// `composite/array-parameter`: array-valued composite parameters are outside the subset.
pub(super) const ARRAY_PARAMETER: CompositeRule = CompositeRule {
    id: "array-parameter",
    code: DiagCode::NonSubsetConstruct,
    summary: "array-valued composite parameters are outside the CXF lowering subset",
};

/// `composite/array-connector`: active array-valued connectors are outside the subset.
pub(super) const ARRAY_CONNECTOR: CompositeRule = CompositeRule {
    id: "array-connector",
    code: DiagCode::NonSubsetConstruct,
    summary: "active array-valued connector nodes require per-element encoding",
};

/// `composite/array-instance`: active array-valued block instances are outside the subset.
pub(super) const ARRAY_INSTANCE: CompositeRule = CompositeRule {
    id: "array-instance",
    code: DiagCode::NonSubsetConstruct,
    summary: "active array-valued block-instance nodes require per-element encoding",
};

/// Every CXF-lowering-subset contract rule, in catalog publication order.
#[cfg(test)]
pub(super) const COMPOSITE_RULES: [CompositeRule; 7] = [
    ROOT_COUNT,
    CONTAINS_CYCLE,
    REPLACEABLE,
    BANNED_MODELICA_KEY,
    ARRAY_PARAMETER,
    ARRAY_CONNECTOR,
    ARRAY_INSTANCE,
];

/// Filesystem path of the checked-in catalog artifact, used by `UPDATE_EXPECT` re-blessing.
#[cfg(test)]
pub(super) const CATALOG_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/oce-cxf.composite-rules.json"
);

/// Checked-in catalog bytes — the byte golden the regenerate-and-diff test compares against.
#[cfg(test)]
pub(super) const CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/oce-cxf.composite-rules.json"
));

/// Render the catalog for the live rule table as a top-level JSON object keyed by rule id, in
/// [`COMPOSITE_RULES`] order, with a trailing newline.
#[cfg(test)]
pub(super) fn render_catalog() -> String {
    let mut out = String::from("{\n");
    for (index, rule) in COMPOSITE_RULES.iter().enumerate() {
        out.push_str("  ");
        out.push_str(&json_str(rule.id));
        out.push_str(": {\n    \"diag_code\": ");
        out.push_str(&json_str(rule.code.as_str()));
        out.push_str(",\n    \"message_prefix\": ");
        out.push_str(&json_str(&rule.message_prefix()));
        out.push_str(",\n    \"summary\": ");
        out.push_str(&json_str(rule.summary));
        out.push_str("\n  }");
        out.push_str(if index + 1 == COMPOSITE_RULES.len() {
            "\n"
        } else {
            ",\n"
        });
    }
    out.push_str("}\n");
    out
}

/// JSON-escape a string through `serde_json`.
#[cfg(test)]
fn json_str(value: &str) -> String {
    serde_json::to_string(value).expect("string-to-JSON serialization is infallible")
}
