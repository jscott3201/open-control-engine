//! JSON-LD `@context` expansion of a CXF document's identity and typing tokens (doc 04 R-3).
//!
//! One uniform pre-resolve pass, run by `resolve()` on a working clone of the DTO before the
//! `@id` index is built, so compact and expanded spellings of the same subject IRI key the same
//! point, block, model, and datatype. The pass transforms the typed DTO slots only, in three
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
//!   `ClassNotFound`). Four exceptions, checked in this order and each ending the checks of its
//!   token: a typing token in any namespace that carries a Unicode format or control character
//!   inside its surrounding whitespace is refused as `MalformedDocument` (GitHub #641), because no
//!   reader can see it and it escapes every other check; a typing token that names an S231 term
//!   only once its surrounding whitespace is trimmed is refused as `MalformedDocument` (GitHub
//!   #587, [`vocabulary_namespace`]), because it would stay verbatim and be read as no S231 type,
//!   which a composite, never reading its `@type`, does not notice; a typing token in the namespace
//!   written through release 0.4.0 or in an `https:` spelling is refused as
//!   `VocabularyNamespaceMismatch`, because a type is read by its trailing term and would otherwise
//!   be read through that namespace; and a typing token in the S231 namespace that names no term of
//!   the vocabulary the engine reads is refused as `MalformedDocument`, or `ClassNotFound` for a
//!   CDL block class the registry lacks (GitHub #620, [`vocabulary_terms`]), which also matches an
//!   enumeration there by its full path.
//!
//! - **Unit IRI nodes** — a `unit` or `displayUnit` written as an IRI node
//!   ([`crate::dto::TermAttr::Iri`]), under any key that names the property (issue #631, design
//!   note `docs/design/unit-spellings.md`, decision 2). Its `@id` is expanded like a typing token,
//!   best-effort, and the expanded IRI is looked up in the closed unit table
//!   ([`crate::unit_iri`]). A mapped one becomes the unit string it stands for, so every later
//!   stage reads `K` whether the document wrote `"K"` or `{"@id": "unit:K"}`. An unmapped one
//!   keeps its IRI node, with the expanded IRI where it has one, and is refused where the node is
//!   read ([`crate::resolve`]'s term-shape rule, `non-subset-construct`); nothing is refused
//!   here. A term the shape rule refuses for another reason (a key beside `@id`, a blank `@id`, a
//!   format or control character) is left exactly as written, so that refusal quotes the authored
//!   text, and so is an `@id` that holds whitespace, which is no IRI. See [`read_unit_iris`].
//!
//! Deliberately excluded from expansion: the [`crate::dto::TermAttr::Iri`] arm of `quantity` (a
//! lexical term consumed verbatim by attribute parsing), the `@type` annotation
//! inside typed literals (an XSD datatype, not a graph identity), and the lossless passthrough
//! (`other`) maps. Their enclosing modeled objects are still checked for a direct `@context`, which
//! is refused rather than applied. The remaining semantic payload body is carried verbatim for
//! re-emit (`_spec/05` R-SEM-6). A generic JSON walk over `@id` keys would violate all three
//! exclusions at once.
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
//! canonical identity. Any other value shape is refused as `MalformedDocument`. A term **spelled
//! as an IRI or a compact IRI** (a `:` anywhere but first or last, or a `/` anywhere) must bind its
//! own expansion, or it is refused as `MalformedDocument`: a whole-token term match wins over
//! every other reading, so `"S231:ExtensionBlock": "http://example.org#Foo"` would redirect that
//! type everywhere in the document, which JSON-LD 1.1 refuses as an invalid IRI mapping (GitHub
//! issue #591; see [`iri_shaped_term_refusal`]). Such a term must still bind its own expansion
//! once the whole context is merged, or it is refused as `NonSubsetConstruct`: a later map of a
//! list may bind the prefix it was read through anew, which JSON-LD allows (see
//! [`rebound_term_refusals`]). A **remote
//! context** — the whole `@context` a string IRI, or a string element inside the list — is refused
//! as `NonSubsetConstruct`: a deterministic embedded engine dereferences nothing at load.
//! A direct `@context` on any `@graph` node, followed identity/typing reference, or modeled
//! value/term object is also refused as `NonSubsetConstruct`, regardless of its value shape.
//! Applying only the document context while silently retaining a scoped context would let the
//! author and engine assign different identities or datatypes to the same semantic value.
//! Context-shape validation runs BEFORE slot expansion and its refusals return alone, so a
//! `RelativeIri` refusal's "declares no @base" clause is literally true whenever it fires.
//!
//! A well-formed context is then held to the one S231 namespace this engine reads
//! ([`vocabulary_namespace`]): `S231` bound to any other IRI, or any prefix bound to the namespace
//! written through release 0.4.0, is refused as `VocabularyNamespaceMismatch`.
//!
//! **What expansion costs** (GitHub issue #709). A token read through a prefix is as long as the
//! IRI the prefix binds, and the document sets both that length and the number of tokens. So a
//! token is read in place ([`token_expansion`]): classifying it, comparing its expansion with a
//! namespace or with a term's mapping, and finding it relative copy nothing. An expansion this
//! pass keeps is joined through one [`ExpansionBudget`] for the document ([`expansion_budget`]),
//! which holds what expansion adds, each expansion's length less its token's, to 8 MiB. The first
//! expansion that does not fit is refused as `ResourceLimitExceeded`, naming the token and its
//! node, and the pass reads nothing after it: it raises what it raised before that token, then
//! that one error (the resolver sorts the list it returns, as it sorts every list).

use std::collections::BTreeSet;

use oce_diag::{DiagCode, Diagnostic};

use crate::diagnostic_list::DiagnosticList;
use crate::dto::{Context, CxfDocument, IriRef, Node, OneOrMany, TermAttr};
use crate::resolve::term_shape;
use crate::unit_iri::unit_of_iri;

#[path = "context_scope.rs"]
mod context_scope;
#[path = "expansion_budget.rs"]
mod expansion_budget;
#[path = "property_keys.rs"]
mod property_keys;
#[path = "token_expansion.rs"]
mod token_expansion;
#[path = "unread_qudt_has_unit.rs"]
mod unread_qudt_has_unit;
#[path = "vocabulary_namespace.rs"]
mod vocabulary_namespace;
#[path = "vocabulary_terms.rs"]
mod vocabulary_terms;

use expansion_budget::{ExpansionBudget, Site};
use token_expansion::{Expansion, Iri, PrefixTable, expand_curie, expand_token, is_absolute_iri};

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
) -> Result<CxfDocument, DiagnosticList> {
    let mut context_diags = DiagnosticList::new();
    let table = prefix_table(&doc.context, &mut context_diags);
    let mut expanded = doc.clone();
    // A well-formed `@context` must spell the S231 vocabulary in the one namespace this engine
    // reads (GitHub #527). The check needs the whole prefix table, so a document whose context
    // shape is already refused reports only those refusals.
    if context_diags.is_empty() {
        vocabulary_namespace::check_bindings(&table, &mut context_diags);
    }
    // Non-compact spellings of modeled properties move into their typed fields first, so the
    // scoped-context refusal and every later stage read one spelling (issue #383). A document
    // whose `@context` is already refused is not normalized: its prefix table may be partial,
    // and its refusal list stays exactly the context refusals.
    // One budget for the whole pass: what a key read through a prefix adds and what every
    // identity and typing token adds are charged together (GitHub issue #709).
    let mut budget = ExpansionBudget::new();
    if context_diags.is_empty() {
        property_keys::normalize(&mut expanded, &table, raw, &mut budget, &mut context_diags);
    }
    // A key whose expansion did not fit is refused, and nothing after it is read, the scoped
    // contexts included.
    if !budget.has_ended() {
        context_scope::collect_refusals(&expanded, &mut context_diags);
    }
    if !context_diags.is_empty() {
        // Context-shape and property-key refusals return alone, before any slot is inspected:
        // whenever the slot pass below reports `RelativeIri`, "declares no @base" is literally
        // true, because a document declaring one was already refused here.
        return Err(context_diags);
    }
    let mut diags = DiagnosticList::new();
    for node in &mut expanded.graph {
        expand_node(node, &table, &mut budget, &mut diags);
        if budget.has_ended() {
            // The expansion that did not fit is refused, and nothing after it is read. Every
            // reader below returns at once on an ended budget, so this only spares the walk.
            break;
        }
    }
    if diags.is_empty() {
        Ok(expanded)
    } else {
        Err(diags)
    }
}

/// Build the prefix/term table from the document `@context`: a single inline map, or a list of
/// inline maps merged in order (later bindings win). Remote references, `@base`, `@import`,
/// `@vocab`, malformed term values, and a term spelled as an IRI that does not bind its own
/// expansion are refused — see the module docs for why each is a refusal rather than a skip.
fn prefix_table(context: &Context, diags: &mut DiagnosticList) -> PrefixTable {
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
    rebound_term_refusals(&table, diags);
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
    diags: &mut DiagnosticList,
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
                    if binds_own_expansion(term, iri, table) {
                        table.insert(term.clone(), iri.clone());
                    } else {
                        // A context may define any number of such terms through one long
                        // prefix, so the refusal, which names the term's own expansion whole,
                        // is built only while the list has room for it (GitHub issue #709).
                        diags.raise(|| iri_shaped_term_refusal(term, iri, table));
                    }
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

/// Whether a term name has the form of an IRI or a compact IRI in JSON-LD 1.1's sense: it
/// contains a colon anywhere but as its first or last character, or a slash anywhere (JSON-LD 1.1
/// Processing Algorithms, Create Term Definition, step 14.2.4).
fn is_iri_shaped_term(term: &str) -> bool {
    term.contains('/')
        || term
            .char_indices()
            .any(|(at, c)| c == ':' && at > 0 && at + 1 < term.len())
}

/// The refusal for a term spelled as an IRI or compact IRI that binds anything but its own
/// expansion where it is defined (GitHub issue #591).
///
/// A whole-token term match wins over every other reading of a token ([`expand_token`]), so a
/// context entry `"S231:ExtensionBlock": "http://example.org#Foo"` (or one keyed by the full IRI)
/// would make every `S231:ExtensionBlock` in the document mean `Foo`: an identity, a `@type`, an
/// `isOfDataType` or a full-IRI property key redirected by the document itself. JSON-LD 1.1
/// forbids exactly this. Create Term Definition, step 14.2.4, reads: "If the term contains a
/// colon (`:`) anywhere but as the first or last character of term, or if it contains a slash
/// (`/`) anywhere: ... If the result of IRI expanding term using local context, and defined, is
/// not the same as the IRI mapping of definition, an invalid IRI mapping error has been detected
/// and processing is aborted."
///
/// The term's own expansion is [`own_expansion`] against the terms in force. Both `@context`
/// shapes reach [`merge_context_entries`] as sorted maps, and a prefix is a proper prefix of its
/// term's name, so the prefix a term reads through is merged before the term, as JSON-LD's
/// dependency resolution on the local context would have it. A term that binds its own expansion
/// changes nothing and is accepted (an absolute IRI bound to itself among them, which JSON-LD's
/// step 14 skips). A term with no expansion of its own (`a/b`, a relative reference) can bind
/// nothing. A name with `@` never reaches here: it is a keyword or has a keyword's form, and is
/// refused (`@base`, `@import`, `@vocab`) or never made a term, which is what JSON-LD's step 5
/// does with a keyword-form name. [`rebound_term_refusals`] then judges the accepted terms
/// against the whole context.
fn iri_shaped_term_refusal(term: &str, iri: &str, table: &PrefixTable) -> Diagnostic {
    let reading = match own_expansion(term, table) {
        Some(own) => format!("its own expansion is `{own}`"),
        None => "it has no expansion of its own".to_owned(),
    };
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "@context term `{term}` is spelled as an IRI but binds `{iri}`, and {reading}; \
             JSON-LD 1.1 refuses this as an invalid IRI mapping (Create Term Definition, step \
             14.2.4), because the document could then redirect that spelling wherever it \
             appears"
        ),
    )
    .with_subject(term.to_owned())
}

/// Whether `term` may bind `iri`: it is not spelled as an IRI ([`is_iri_shaped_term`]), or `iri`
/// is its own expansion against `table`. The expansion is compared in place, never joined: a
/// context may define a term per few bytes through a prefix bound to an IRI of any length.
fn binds_own_expansion(term: &str, iri: &str, table: &PrefixTable) -> bool {
    !is_iri_shaped_term(term) || own_expansion(term, table).is_some_and(|own| own.is(iri))
}

/// Refuse every term of the finished `table` spelled as an IRI whose own expansion under the whole
/// context is no longer its mapping (GitHub issue #591), in term order.
///
/// A later map of a list context may bind or rebind the prefix a term was read through, and
/// JSON-LD keeps the term's mapping when it does. The spelling would then read one way as a whole token and
/// another everywhere else: `[{"S231": <hostile>, "S231:ExtensionBlock": <hostile>ExtensionBlock},
/// {"S231": <S231 namespace>}]` passes the definition check and the namespace check, and would
/// read a root typed `S231:ExtensionBlock` as a class of the hostile namespace. JSON-LD accepts
/// such a context; this engine refuses it as `NonSubsetConstruct`, as it refuses a nested compact
/// IRI, because a whole-token term match must never change what a token means. Only a list context
/// can trip this, since one map binds each term once and merges a prefix before its terms.
fn rebound_term_refusals(table: &PrefixTable, diags: &mut DiagnosticList) {
    for (term, iri) in table {
        if binds_own_expansion(term, iri, table) {
            continue;
        }
        diags.raise(|| {
            let now = own_expansion(term, table)
                .map_or_else(|| "nothing".to_owned(), |now| format!("`{now}`"));
            Diagnostic::error(
                DiagCode::NonSubsetConstruct,
                format!(
                    "@context term `{term}` is spelled as an IRI and binds `{iri}`, its own \
                     expansion where it is defined, but a later context map binds its prefix, so \
                     the same spelling now expands to {now}; a term spelled as an IRI must bind \
                     its own expansion under the whole context, or the document could redirect \
                     that spelling wherever it appears"
                ),
            )
            .with_subject(term.clone())
        });
    }
}

/// A term's own expansion: its spelling read as [`expand_token`] reads a token, without the
/// whole-token match of the term itself (a compact IRI through its prefix, an absolute IRI as
/// written), or `None` when it has no absolute reading. Nothing is copied.
fn own_expansion<'a>(term: &'a str, table: &'a PrefixTable) -> Option<Iri<'a>> {
    match expand_curie(term, table) {
        Expansion::Expanded(own) => Some(own),
        Expansion::Verbatim => Some(Iri::whole(term)),
        Expansion::Relative => None,
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

/// Expand an identity token in place; a relative token is refused with `RelativeIri`.
///
/// `owner` is `None` when the token is the node's own `@id` (the diagnostic then carries the
/// token itself as subject) and `Some(node_id)` for a structural reference on that node. The
/// expansion is joined through `budget`: one that does not fit leaves the token as written and
/// is refused there, and once the budget has ended the token is not read at all.
fn expand_identity(
    token: &mut String,
    slot: &str,
    owner: Option<&str>,
    table: &PrefixTable,
    budget: &mut ExpansionBudget,
    diags: &mut DiagnosticList,
) {
    if token.is_empty() || budget.has_ended() {
        // An empty `@id` has its own pinned diagnostic (`MissingConnectorId`) downstream.
        return;
    }
    match expand_token(token, table) {
        Expansion::Expanded(iri) => {
            let site = Site {
                slot,
                reference: owner.is_some(),
                node: owner,
            };
            if let Some(iri) = budget.expand(iri, token, site, diags) {
                *token = iri;
            }
        }
        Expansion::Verbatim => {}
        // One node may carry any number of relative references, each refused about the same
        // `@id`, so the refusal is built only while the list has room for it (GitHub issue #695).
        Expansion::Relative => diags.raise(|| {
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
            Diagnostic::error(DiagCode::RelativeIri, message).with_subject(subject)
        }),
    }
}

/// Expand a typing token in place — best-effort, never refused for its form (the downstream
/// no-match diagnostics own junk typing tokens). `slot` and `owner` name the token for the one
/// refusal there is: an expansion that does not fit `budget`, which leaves the token as written.
fn expand_typing(
    token: &mut String,
    slot: &str,
    owner: &str,
    table: &PrefixTable,
    budget: &mut ExpansionBudget,
    diags: &mut DiagnosticList,
) {
    if let Expansion::Expanded(iri) = expand_token(token, table) {
        let site = Site {
            slot,
            reference: false,
            node: Some(owner),
        };
        if let Some(iri) = budget.expand(iri, token, site, diags) {
            *token = iri;
        }
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

/// Expand every in-scope slot of one node, until an expansion does not fit `budget`: nothing of
/// the node is read after that, so the refusal is the last diagnostic the node raises.
fn expand_node(
    node: &mut Node,
    table: &PrefixTable,
    budget: &mut ExpansionBudget,
    diags: &mut DiagnosticList,
) {
    expand_identity(&mut node.id, "@id", None, table, budget, diags);
    let owner = node.id.clone();
    let mut expand_refs = |slot: &str, refs: &mut OneOrMany<IriRef>| {
        each_mut(refs, |reference: &mut IriRef| {
            expand_identity(&mut reference.id, slot, Some(&owner), table, budget, diags);
        });
    };
    expand_refs("S231:hasInput", &mut node.has_input);
    expand_refs("S231:hasOutput", &mut node.has_output);
    expand_refs("S231:hasParameter", &mut node.has_parameter);
    expand_refs("S231:hasConstant", &mut node.has_constant);
    expand_refs("S231:containsBlock", &mut node.contains_block);
    expand_refs("S231:hasInstance", &mut node.has_instance);
    expand_refs("S231:isConnectedTo", &mut node.is_connected_to);
    let mut expand_type = |slot: &str, value: &mut String| {
        if budget.has_ended() {
            return;
        }
        if vocabulary_namespace::check_typing_controls(value, slot, &owner, diags) {
            return;
        }
        if vocabulary_namespace::check_typing_whitespace(value, slot, &owner, table, diags) {
            return;
        }
        expand_typing(value, slot, &owner, table, budget, diags);
        if budget.has_ended() {
            return;
        }
        // A token in the retired or an `https:` namespace never starts with the published one,
        // so at most one of these two refuses it.
        vocabulary_namespace::check_typing(value, slot, &owner, diags);
        vocabulary_terms::check_term(value, slot, &owner, diags);
    };
    if let Some(types) = &mut node.r#type {
        each_mut(types, |value: &mut String| expand_type("@type", value));
    }
    if let Some(datatype) = &mut node.is_of_data_type {
        expand_type("S231:isOfDataType", &mut datatype.id);
    }
    read_unit_iris(node, &owner, table, budget, diags);
}

/// Read each `unit` and `displayUnit` IRI node of `node` through the closed unit table (issue
/// #631; design note `docs/design/unit-spellings.md`, decision 2).
///
/// A term's shape decides how it is read, not its key: a string or typed literal is a unit string
/// and is not touched here. An IRI node is expanded against the document `@context`
/// ([`expand_token`], best-effort like a typing token) and looked up with
/// [`unit_of_iri`]:
///
/// - **mapped**: the term becomes the bare unit string, the spelling the §7.10 unit gate, the
///   point list, the content id and the exporter all read;
/// - **unmapped**: the term stays an IRI node, its `@id` replaced by the expanded IRI where the
///   token expanded, so the refusal raised where the node is read
///   ([`term_shape::unmapped_unit`]) quotes the IRI the document means and not only its compact
///   spelling.
///
/// A term the shape rule already refuses ([`term_shape::problem`]) is left as written: a blank
/// `@id` or one carrying a format or control character is never expanded, even where a `@context`
/// term of that exact spelling exists, so the document cannot use a binding to launder a term no
/// reader can see. An `@id` that holds whitespace anywhere is left as written too: no IRI holds
/// any, so it is no identifier to look up, whatever term the `@context` declares under that
/// spelling, and it is refused as unmapped where it is read. `quantity` is never read here
/// (decision 1): its IRI node stays a verbatim lexical term.
///
/// Work is one expansion and one table lookup per term; nothing is dereferenced, and the one
/// refusal raised here is an expansion that does not fit `budget`, named by its property and
/// `owner`, the node's expanded `@id`. Nothing is read once the budget has ended.
fn read_unit_iris(
    node: &mut Node,
    owner: &str,
    table: &PrefixTable,
    budget: &mut ExpansionBudget,
    diags: &mut DiagnosticList,
) {
    let slots = [
        ("S231:unit", &mut node.unit),
        ("S231:displayUnit", &mut node.display_unit),
    ];
    for (property, slot) in slots {
        if budget.has_ended() {
            return;
        }
        let Some(term) = slot.as_mut() else {
            continue;
        };
        if term_shape::problem(term).is_some() {
            continue;
        }
        let TermAttr::Iri { id, .. } = term else {
            continue;
        };
        if id.contains(char::is_whitespace) {
            continue;
        }
        if let Expansion::Expanded(iri) = expand_token(id, table) {
            let site = Site {
                slot: property,
                reference: false,
                node: Some(owner),
            };
            match budget.expand(iri, id, site, diags) {
                Some(iri) => *id = iri,
                None => return,
            }
        }
        if let Some(unit) = unit_of_iri(id).map(str::to_owned) {
            *term = TermAttr::Bare(unit);
        }
    }
}

#[cfg(test)]
#[path = "expand_nested_tests.rs"]
mod nested_tests;

#[cfg(test)]
#[path = "expand_tests.rs"]
mod tests;
