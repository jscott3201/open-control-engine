//! JSON-LD `@context` expansion of a CXF document's identity and typing tokens (doc 04 R-3).
//!
//! One uniform pre-resolve pass, run by `resolve()` on a working clone of the DTO before the
//! `@id` index is built, so compact and expanded spellings of the same subject IRI key the same
//! point, block, model, and datatype. The pass transforms the typed DTO slots only, in two
//! classes:
//!
//! - **Identity slots** — every [`Node`] `@id` and every [`IriRef`] in `containsBlock`,
//!   `hasInput`, `hasOutput`, `hasParameter`, `hasConstant`, `isConnectedTo`, and
//!   `hasInstance`. A whole-token `@context` term match substitutes. A CURIE with a
//!   declared prefix expands, and the JOINED result must itself pass [`is_absolute_iri`]
//!   — a whitespace-bearing suffix (`ex:c d`) refuses even under a declared prefix, so
//!   expansion never mints a key the engine's own importer would refuse. A token whose
//!   first `:` is followed by `//` is never CURIE-split (per JSON-LD 1.1 §4.1.5, Compact
//!   IRIs, such a value is interpreted as an IRI rather than CURIE-expanded, so it can
//!   never be prefix-rescued); it and every undeclared-prefix token survive as written
//!   only when they are themselves syntactically absolute IRIs — the same
//!   [`is_absolute_iri`] predicate a `@context` binding value must pass, so a spelling
//!   refused as a binding can never load as a durable identity on those arms.
//!   Everything else — no `:` at all, a prefix that is not an RFC 3986 scheme on either
//!   arm (`:x`, `2024:x`, `2024://x`), or a valid scheme whose remainder is empty or
//!   carries whitespace (`ab:`, `ab:c d`) — has no canonical absolute form and — `@base`
//!   being a refused construct, below — is refused with [`DiagCode::RelativeIri`], because
//!   a node the document cannot name canonically has no identity to key on. An **empty**
//!   `@id` is left alone: the resolver's own `MissingConnectorId` arm owns that case.
//! - **Typing slots** — every `@type` value and the `isOfDataType` reference: the identical
//!   expansion, **best-effort**. Whatever does not expand stays verbatim and is never refused
//!   here — the downstream `ClassNotFound` / `UnresolvedReference` no-match diagnostics are the
//!   pinned behavior for junk typing tokens (`resolve_errors.rs` pins `@type: "NotAnIri"` →
//!   `ClassNotFound`).
//!
//! Deliberately excluded from expansion: the [`crate::dto::TermAttr::Iri`] arm (`unit`/`quantity`/
//! `displayUnit` are lexical terms consumed verbatim by attribute parsing), the `@type` annotation
//! inside typed literals (an XSD datatype, not a graph identity), and the lossless passthrough
//! (`other`) maps. Their enclosing modeled objects are still checked for a direct `@context`, which
//! is refused rather than applied. The remaining semantic payload body is carried verbatim for
//! re-emit (`_spec/05` R-SEM-6). A generic JSON walk over `@id` keys would violate all three
//! exclusions at once and would refuse the shipped `"degC"` unit term.
//!
//! `@context` handling: the supported form is an **inline prefix map** — a single map, or a
//! list of maps merged in order with later bindings overriding earlier ones (JSON-LD context
//! processing order). Within a map, `@base`, `@import`, and `@vocab` are refused as
//! `NonSubsetConstruct`: all change identity semantics this engine does not implement, and
//! skipping them silently
//! would reintroduce the exact spelling-dependent-identity hazard this pass exists to close.
//! Every other `@`-keyword entry (`@version`, `@protected`, …) is skipped; a non-`@` key with
//! a simple-string value declares a prefix term, and that value must itself be a syntactically
//! absolute IRI (see [`is_absolute_iri`]) or the binding is refused as `NonSubsetConstruct` —
//! a prefix bound to a relative value would smuggle relative identities past the per-token
//! guard. A value that is a compact IRI through another active prefix is also refused: recursive
//! context-term expansion is outside the subset, and retaining the compact spelling would split
//! canonical identity. Any other value shape is refused as `MalformedDocument`. A **remote
//! context** — the whole `@context` a string IRI, or a string element inside the list — is refused
//! as `NonSubsetConstruct`: a deterministic embedded engine dereferences nothing at load.
//! A direct `@context` on any `@graph` node, followed identity/typing reference, or modeled
//! value/term object is also refused as `NonSubsetConstruct`, regardless of its value shape.
//! Applying only the document context while silently retaining a scoped context would let the
//! author and engine assign different identities or datatypes to the same semantic value.
//! Context-shape validation runs BEFORE slot expansion and its refusals return alone, so a
//! `RelativeIri` refusal's "declares no @base" clause is literally true whenever it fires.

use std::collections::{BTreeMap, BTreeSet};

use oce_diag::{DiagCode, Diagnostic};

use crate::dto::{Context, CxfDocument, IriRef, Node, OneOrMany};

#[path = "context_scope.rs"]
mod context_scope;
#[path = "property_keys.rs"]
mod property_keys;

/// Declared `@context` prefix/term substitutions: term → IRI.
type PrefixTable = BTreeMap<String, String>;

/// Expand `doc`'s identity and typing tokens against its own `@context`.
///
/// Returns the expanded working clone the resolver keys on, or every refusal diagnostic when
/// any identity token cannot be canonicalized, a non-keyword `@context` term is not a simple
/// string IRI, or a scoped context appears on an `@graph` node, followed reference, or modeled
/// value/term object. The input document is never mutated — the DTO keeps the raw source (R-3's
/// "retain the raw string" lives in the Layer-A document, not in the resolved graph).
///
/// `raw` is the authored JSON `doc` was parsed from, when the caller has it; property-key
/// normalization reads it only to see a compact key the DTO cannot represent (an explicit `null`
/// or `[]`), and only on a node that also spells the same property another way.
pub(crate) fn expand_document(
    doc: &CxfDocument,
    raw: Option<&[u8]>,
) -> Result<CxfDocument, Vec<Diagnostic>> {
    let mut context_diags = Vec::new();
    let table = prefix_table(&doc.context, &mut context_diags);
    let mut expanded = doc.clone();
    // Non-compact spellings of modeled properties move into their typed fields first, so the
    // scoped-context refusal and every later stage read one spelling (issue #383). A document
    // whose `@context` is already refused is not normalized: its prefix table may be partial,
    // and its refusal list stays exactly the context refusals.
    if context_diags.is_empty() {
        property_keys::normalize(&mut expanded, &table, raw, &mut context_diags);
    }
    context_scope::collect_refusals(&expanded, &mut context_diags);
    if !context_diags.is_empty() {
        // Context-shape and property-key refusals return alone, before any slot is inspected:
        // whenever the slot pass below reports `RelativeIri`, "declares no @base" is literally
        // true, because a document declaring one was already refused here.
        return Err(context_diags);
    }
    let mut diags = Vec::new();
    for node in &mut expanded.graph {
        expand_node(node, &table, &mut diags);
    }
    if diags.is_empty() {
        Ok(expanded)
    } else {
        Err(diags)
    }
}

/// Build the prefix/term table from the document `@context`: a single inline map, or a list of
/// inline maps merged in order (later bindings win). Remote references, `@base`, `@import`,
/// `@vocab`, and malformed term values are refused — see the module docs for why each is a
/// refusal rather than a skip.
fn prefix_table(context: &Context, diags: &mut Vec<Diagnostic>) -> PrefixTable {
    let mut table = PrefixTable::new();
    match context {
        Context::Map(map) => merge_context_entries(map.iter(), &mut table, diags),
        Context::List(entries) => {
            for entry in entries {
                match entry {
                    serde_json::Value::Object(map) => {
                        merge_context_entries(map.iter(), &mut table, diags);
                    }
                    serde_json::Value::String(reference) => {
                        diags.push(remote_context_refusal(reference));
                    }
                    other => diags.push(Diagnostic::error(
                        DiagCode::MalformedDocument,
                        format!("@context list entry must be an inline map, got `{other}`"),
                    )),
                }
            }
        }
        Context::Iri(reference) => diags.push(remote_context_refusal(reference)),
    }
    table
}

/// The refusal for a remote `@context` reference (a string IRI in either context shape).
fn remote_context_refusal(reference: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::NonSubsetConstruct,
        format!(
            "@context references the remote context `{reference}`; a deterministic embedded \
             engine cannot dereference remote contexts at load — inline the bindings as a \
             prefix map"
        ),
    )
    .with_subject(reference.to_owned())
}

/// Merge one inline context map's entries into `table`. Entries are processed in order and a
/// later binding for the same term overrides an earlier one (JSON-LD context processing
/// order), which is what gives a list of maps its later-wins semantics.
fn merge_context_entries<'a>(
    entries: impl Iterator<Item = (&'a String, &'a serde_json::Value)>,
    table: &mut PrefixTable,
    diags: &mut Vec<Diagnostic>,
) {
    let entries: Vec<_> = entries.collect();
    let local_terms: BTreeSet<&str> = entries.iter().map(|(term, _)| term.as_str()).collect();
    for &(term, value) in &entries {
        if term == "@base" || term == "@import" || term == "@vocab" {
            // These keywords change identity semantics this engine does not implement.
            // Skipping them silently would be the spelling-dependent-identity hazard again:
            // the document means one set of subject IRIs, the engine would key another.
            diags.push(
                Diagnostic::error(
                    DiagCode::NonSubsetConstruct,
                    format!(
                        "@context declares `{term}`, which changes identity semantics this \
                         engine does not implement; write prefix terms or absolute IRIs \
                         instead"
                    ),
                )
                .with_subject(term.clone()),
            );
            continue;
        }
        if term.starts_with('@') {
            // Remaining JSON-LD keywords (`@version`, `@protected`, …) are legal context
            // entries that do not affect identity; skipped.
            continue;
        }
        match value {
            serde_json::Value::String(iri) => {
                if let Some(prefix) = nested_compact_prefix(iri, &local_terms, table) {
                    diags.push(
                        Diagnostic::error(
                            DiagCode::NonSubsetConstruct,
                            format!(
                                "@context term `{term}` binds the nested compact IRI `{iri}` \
                                 through prefix `{prefix}`; nested compact-IRI term definitions \
                                 are not supported — bind the expanded absolute IRI instead"
                            ),
                        )
                        .with_subject(term.clone()),
                    );
                } else if is_absolute_iri(iri) {
                    table.insert(term.clone(), iri.clone());
                } else {
                    // A prefix bound to a non-absolute value would concatenate every CURIE under
                    // it into a RELATIVE identity that the per-token guard cannot see (the joined
                    // token contains a `:` from the CURIE spelling), defeating the canonical-key
                    // contract. Refuse the binding itself, up front.
                    diags.push(
                        Diagnostic::error(
                            DiagCode::NonSubsetConstruct,
                            format!(
                                "@context term `{term}` binds `{iri}`, which is not an absolute \
                                 IRI; a prefix must bind an absolute IRI (a scheme, `:`, then a \
                                 nonempty remainder with no whitespace)"
                            ),
                        )
                        .with_subject(term.clone()),
                    );
                }
            }
            other => diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "@context term `{term}` must map to a simple string IRI, \
                         got `{other}`"
                    ),
                )
                .with_subject(term.clone()),
            ),
        }
    }
}

/// The active prefix of a nested compact-IRI term definition, if one exists. Prefixes from earlier
/// context maps and every term in the current map are active; a `//` suffix is an absolute IRI arm,
/// not a compact IRI (JSON-LD 1.1 §4.1.5).
fn nested_compact_prefix<'a>(
    iri: &'a str,
    local_terms: &BTreeSet<&str>,
    table: &PrefixTable,
) -> Option<&'a str> {
    let (prefix, suffix) = iri.split_once(':')?;
    (!suffix.starts_with("//") && (table.contains_key(prefix) || local_terms.contains(prefix)))
        .then_some(prefix)
}

/// Whether `value` has syntactically absolute-IRI form: everything before the FIRST colon is
/// a nonempty ASCII scheme per RFC 3986 §3.1 — a letter, then letters, digits, `+`, `-`, or
/// `.` — and the remainder after that colon is nonempty and free of whitespace. The whole
/// remainder rule is engine policy, not conformance: a canonical durable key is nonempty and
/// whitespace-free — ASCII and Unicode alike — by choice. RFC 3986 agrees on ASCII
/// whitespace (no URI production admits one), but RFC 3987's `ucschar` (§2.2) admits
/// U+00A0, U+2028, and other `char::is_whitespace` characters, so this predicate refuses
/// some RFC 3987-conformant IRIs; and `ab:` is a legal URI (`path-empty`, RFC 3986 §3.3)
/// that carries no name to key a durable identity on. The contract stays syntactic form,
/// not semantic reachability: `S231:x` passes when `S231` is not an active prefix because it is an
/// RFC-valid scheme. [`merge_context_entries`] separately refuses a binding value when its scheme
/// position names an active prefix, closing the nested compact-IRI ambiguity before this predicate
/// runs. The predicate guards both directions of the remaining binding/identity symmetry:
/// `@context` binding values (so the legal JSON-LD binding `{"ex": "urn:"}` refuses as
/// `NonSubsetConstruct`), the two keep-as-written identity arms of [`expand_token`], and the
/// declared-prefix arm's joined expansion.
fn is_absolute_iri(value: &str) -> bool {
    match value.split_once(':') {
        Some((scheme, rest)) => {
            let mut chars = scheme.chars();
            let scheme_ok = match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                }
                _ => false,
            };
            scheme_ok && !rest.is_empty() && !rest.contains(char::is_whitespace)
        }
        None => false,
    }
}

/// The outcome of expanding one token against the prefix table.
enum Expansion {
    /// A whole-token term match, or a CURIE with a declared prefix whose joined expansion
    /// passes [`is_absolute_iri`]: the canonical form.
    Expanded(String),
    /// Kept as written: a `//`-suffix or undeclared-prefix token that is itself a
    /// syntactically absolute IRI ([`is_absolute_iri`]).
    Verbatim,
    /// No canonical absolute form: no `:` and no whole-token term match; a prefix that is
    /// not an RFC 3986 scheme (empty, or scheme-invalid like `2024`), whether or not the
    /// suffix starts with `//`; a valid scheme whose remainder is empty or carries
    /// whitespace (`ab:`, `ab:c d`); or a declared-prefix CURIE whose joined expansion
    /// fails the predicate (a whitespace-bearing suffix, `ex:c d` with `ex` bound).
    Relative,
}

/// Classify and expand one token. Pure; the caller decides what `Relative` means per slot.
///
/// [`is_absolute_iri`] gates the outcome of every arm. The two keep-as-written arms are
/// symmetric with the `@context` binding check: a token survives verbatim only when the
/// predicate accepts it, whether its suffix starts with `//` (never CURIE-split) or its
/// prefix is simply undeclared — so a spelling refused as a binding value cannot become a
/// durable key on those arms. The declared-prefix arm concatenates and then runs the same
/// predicate on the JOINED result: a declared prefix rescues a scheme-invalid CURIE
/// spelling (`2024:x` with `2024` bound expands, since the binding supplies the scheme)
/// but never a whitespace-bearing suffix (`ex:c d` refuses with `ex` bound). No arm can
/// produce a durable key the predicate refuses.
fn expand_token(token: &str, table: &PrefixTable) -> Expansion {
    // A whole-token term match wins over CURIE splitting (JSON-LD term semantics): a token
    // that IS a declared term substitutes even though it contains no `:`.
    if let Some(iri) = table.get(token) {
        return Expansion::Expanded(iri.clone());
    }
    match token.split_once(':') {
        // A value whose suffix begins with `//` is interpreted as an IRI, not
        // CURIE-expanded (JSON-LD 1.1 §4.1.5, Compact IRIs), so this token is
        // never CURIE-split and never prefix-rescued: `http://…` stays absolute even when
        // its scheme collides with a declared prefix name, and a scheme-invalid spelling
        // (`2024://x`) has no canonical form at all. It must refuse here rather than fall
        // through to the prefix lookup, which would mint `<bound-iri>//x` as a durable key.
        Some((_, suffix)) if suffix.starts_with("//") => {
            if is_absolute_iri(token) {
                Expansion::Verbatim
            } else {
                Expansion::Relative
            }
        }
        Some((prefix, suffix)) => match table.get(prefix) {
            // The JOINED expansion must pass the same predicate the other arms enforce.
            // Every accepted binding contributes a valid scheme and a clean nonempty
            // remainder head, so the join can only fail on an authored suffix carrying
            // whitespace (`ex:c d`) — expanding that would mint a key the engine's own
            // importer refuses, splitting compact/expanded twins of one subject.
            Some(iri) => {
                let joined = format!("{iri}{suffix}");
                if is_absolute_iri(&joined) {
                    Expansion::Expanded(joined)
                } else {
                    Expansion::Relative
                }
            }
            None if is_absolute_iri(token) => Expansion::Verbatim,
            // `2024:x` and `:x` (no RFC 3986 scheme) and `ab:` / `ab:c d` (valid scheme,
            // empty or whitespace-bearing remainder) expand against nothing and have no
            // absolute form: refusing beats minting a durable key from a malformed spelling
            // with zero diagnostics.
            None => Expansion::Relative,
        },
        None => Expansion::Relative,
    }
}

/// Expand an identity token in place; a relative token is refused with `RelativeIri`.
///
/// `owner` is `None` when the token is the node's own `@id` (the diagnostic then carries the
/// token itself as subject) and `Some(node_id)` for a structural reference on that node.
fn expand_identity(
    token: &mut String,
    slot: &str,
    owner: Option<&str>,
    table: &PrefixTable,
    diags: &mut Vec<Diagnostic>,
) {
    if token.is_empty() {
        // An empty `@id` has its own pinned diagnostic (`MissingConnectorId`) downstream.
        return;
    }
    match expand_token(token, table) {
        Expansion::Expanded(iri) => *token = iri,
        Expansion::Verbatim => {}
        Expansion::Relative => {
            let (message, subject) = match owner {
                None => (
                    format!(
                        "@id `{token}` is a relative IRI reference and the document declares \
                         no @base to resolve it against"
                    ),
                    token.clone(),
                ),
                Some(owner) => (
                    format!(
                        "{slot} reference `{token}` on node `{owner}` is a relative IRI \
                         reference and the document declares no @base to resolve it against"
                    ),
                    owner.to_owned(),
                ),
            };
            diags.push(Diagnostic::error(DiagCode::RelativeIri, message).with_subject(subject));
        }
    }
}

/// Expand a typing token in place — best-effort, never refused (the downstream no-match
/// diagnostics own junk typing tokens).
fn expand_typing(token: &mut String, table: &PrefixTable) {
    if let Expansion::Expanded(iri) = expand_token(token, table) {
        *token = iri;
    }
}

/// Apply `f` to each value of a [`OneOrMany`] in place.
fn each_mut<T>(values: &mut OneOrMany<T>, mut f: impl FnMut(&mut T)) {
    match values {
        OneOrMany::None => {}
        OneOrMany::One(value) => f(value),
        OneOrMany::Many(values) => values.iter_mut().for_each(&mut f),
    }
}

/// Expand every in-scope slot of one node.
fn expand_node(node: &mut Node, table: &PrefixTable, diags: &mut Vec<Diagnostic>) {
    expand_identity(&mut node.id, "@id", None, table, diags);
    let owner = node.id.clone();
    let mut expand_refs = |slot: &str, refs: &mut OneOrMany<IriRef>| {
        each_mut(refs, |reference: &mut IriRef| {
            expand_identity(&mut reference.id, slot, Some(&owner), table, diags);
        });
    };
    expand_refs("S231:hasInput", &mut node.has_input);
    expand_refs("S231:hasOutput", &mut node.has_output);
    expand_refs("S231:hasParameter", &mut node.has_parameter);
    expand_refs("S231:hasConstant", &mut node.has_constant);
    expand_refs("S231:containsBlock", &mut node.contains_block);
    expand_refs("S231:hasInstance", &mut node.has_instance);
    expand_refs("S231:isConnectedTo", &mut node.is_connected_to);
    if let Some(types) = &mut node.r#type {
        each_mut(types, |value: &mut String| expand_typing(value, table));
    }
    if let Some(datatype) = &mut node.is_of_data_type {
        expand_typing(&mut datatype.id, table);
    }
}

#[cfg(test)]
#[path = "expand_nested_tests.rs"]
mod nested_tests;

#[cfg(test)]
#[path = "expand_tests.rs"]
mod tests;
