//! Oracle-side hierarchy flattening: parse each vendored CXF document, resolve class
//! names the way Modelica lexical scoping does, and recursively inline composite
//! instances by prefix substitution.
//!
//! Semantics of record: the reference `hflat.py` (brief `structural-oracle-gate`),
//! measured against the corpus — our exporter keeps composite boundary connectors as
//! real nodes, so inlining is pure prefix substitution; the child document's root
//! connectors are the authoritative boundary-port inventory (the parent's
//! `hasInstance` lists only *referenced* ports); and no array-valued composite
//! instance exists in the corpus (asserted here, loudly, in case one ever appears).
//!
//! Class canonicalization never suffix-matches: modelica-json copies `.mo` type
//! spellings verbatim into `@type` (`ex:CDL.Logical.Not`), so resolution walks the
//! declaring class's package path and consults exactly two sources — the vendored
//! oracle-doc set and the vendored `.mo` set.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::Value;

use super::{doc_path, load_graph, mo_path};

/// One parsed oracle document, ids stripped of the `ex:<class>.` prefix.
#[derive(Debug, Default, Clone)]
pub struct ParsedDoc {
    /// Instance path → (declared class as written, `S231:isArray`).
    /// Resolve the class with [`Ctx::resolve`].
    pub inst: BTreeMap<String, (String, bool)>,
    /// Root connector name → connector type term.
    pub conn: BTreeMap<String, String>,
    /// Container path → declared port set (from `S231:hasInstance`).
    pub inv: BTreeMap<String, BTreeSet<String>>,
    /// Directed edges as written (`S231:isConnectedTo`).
    pub edges: BTreeSet<(String, String)>,
}

/// The fully flattened view of one upstream class.
#[derive(Debug, Default, Clone)]
pub struct Flat {
    /// Elementary instance path → resolved class.
    pub inst: BTreeMap<String, String>,
    /// Expanded composite instance path → resolved class.
    pub comps: BTreeMap<String, String>,
    /// Container path → port inventory.
    pub inv: BTreeMap<String, BTreeSet<String>>,
    /// Directed edges over flattened paths.
    pub edges: BTreeSet<(String, String)>,
    /// Root connectors of the class itself.
    pub topports: BTreeSet<String>,
    /// Path → class whose `.mo` declares the path's last segment.
    pub declared_by: BTreeMap<String, String>,
}

/// Memoized corpus context. All lookups are pure functions of the vendored tree.
#[derive(Default)]
pub struct Ctx {
    docs: HashMap<String, ParsedDoc>,
    flats: HashMap<String, Flat>,
    resolved: HashMap<(String, String), String>,
}

impl Ctx {
    pub fn parse_doc(&mut self, class: &str) -> &ParsedDoc {
        if !self.docs.contains_key(class) {
            let parsed = parse_doc_uncached(class);
            self.docs.insert(class.to_owned(), parsed);
        }
        &self.docs[class]
    }

    /// Expandable = its oracle doc exists and declares instances.
    pub fn is_composite(&mut self, class: &str) -> bool {
        doc_path(class).exists() && !self.parse_doc(class).inst.is_empty()
    }

    /// Modelica-scoped resolution of a class name written as `.mo` shorthand: walk up
    /// the declaring class's package path; first prefix under which the name exists as
    /// an oracle doc or a vendored `.mo` wins. Never a suffix match.
    pub fn resolve(&mut self, icls: &str, ctx_class: &str) -> String {
        if icls.starts_with("Buildings.") {
            return icls.to_owned();
        }
        let key = (icls.to_owned(), ctx_class.to_owned());
        if let Some(hit) = self.resolved.get(&key) {
            return hit.clone();
        }
        let parts: Vec<&str> = ctx_class.split('.').collect();
        let mut out = icls.to_owned();
        for k in (0..parts.len()).rev() {
            let mut cand = parts[..k].join(".");
            if !cand.is_empty() {
                cand.push('.');
            }
            cand.push_str(icls);
            if doc_path(&cand).exists() || mo_path(&cand).exists() {
                out = cand;
                break;
            }
        }
        self.resolved.insert(key, out.clone());
        out
    }

    /// Recursively inline composite instances of `class` by prefix substitution.
    pub fn flatten(&mut self, class: &str) -> Flat {
        if let Some(hit) = self.flats.get(class) {
            return hit.clone();
        }
        let doc = self.parse_doc(class).clone();
        let mut flat = Flat {
            inv: doc.inv.clone(),
            edges: doc.edges.clone(),
            topports: doc.conn.keys().cloned().collect(),
            ..Flat::default()
        };
        for (name, (raw_cls, is_array)) in &doc.inst {
            let icls = self.resolve(raw_cls, class);
            flat.declared_by.insert(name.clone(), class.to_owned());
            if self.is_composite(&icls) {
                // no array-valued composite instance exists in this corpus; if one ever
                // appears, prefix-substitution inlining needs a design change, not a guess
                assert!(
                    !is_array,
                    "array-valued composite instance {class}.{name} ({icls}): unsupported"
                );
                flat.comps.insert(name.clone(), icls.clone());
                let sub = self.flatten(&icls);
                let pre = format!("{name}.");
                for (k, v) in &sub.inst {
                    flat.inst.insert(format!("{pre}{k}"), v.clone());
                }
                for (k, v) in &sub.comps {
                    flat.comps.insert(format!("{pre}{k}"), v.clone());
                }
                for (k, v) in &sub.inv {
                    flat.inv.insert(format!("{pre}{k}"), v.clone());
                }
                for (k, v) in &sub.declared_by {
                    flat.declared_by.insert(format!("{pre}{k}"), v.clone());
                }
                // child root connectors are the authoritative boundary inventory
                let child_conn: BTreeSet<String> =
                    self.parse_doc(&icls).conn.keys().cloned().collect();
                flat.inv.entry(name.clone()).or_default().extend(child_conn);
                for (a, b) in &sub.edges {
                    flat.edges
                        .insert((format!("{pre}{a}"), format!("{pre}{b}")));
                }
            } else {
                flat.inst.insert(name.clone(), icls);
            }
        }
        self.flats.insert(class.to_owned(), flat.clone());
        flat
    }
}

fn strip<'a>(id: &'a str, prefix: &str) -> &'a str {
    id.strip_prefix(prefix).unwrap_or(id)
}

fn parse_doc_uncached(class: &str) -> ParsedDoc {
    let graph = load_graph(&doc_path(class));
    let prefix = format!("ex:{class}.");
    let mut doc = ParsedDoc::default();
    for node in &graph {
        let id = node.get("@id").and_then(Value::as_str).unwrap_or_default();
        let loc = strip(id, &prefix);
        let ty = node.get("@type").and_then(Value::as_str);
        if let Some(t) = ty {
            if let Some(cls) = t.strip_prefix("ex:") {
                if t != "S231:Block" {
                    let is_array = node.get("S231:isArray").is_some_and(|v| v != &Value::Null);
                    doc.inst.insert(loc.to_owned(), (cls.to_owned(), is_array));
                }
            } else if (t.contains("Input") || t.contains("Output"))
                && !loc.contains('.')
                && id.starts_with(&prefix)
            {
                doc.conn.insert(loc.to_owned(), t.to_owned());
            }
        }
        if let Some(members) = node.get("S231:hasInstance").and_then(Value::as_array) {
            for m in members {
                if let Some(mid) = m.get("@id").and_then(Value::as_str) {
                    let seg = strip(mid, &prefix);
                    if let Some((_, port)) = seg.split_once('.') {
                        doc.inv
                            .entry(loc.to_owned())
                            .or_default()
                            .insert(port.to_owned());
                    }
                }
            }
        }
        if let Some(conn) = node.get("S231:isConnectedTo") {
            let targets: Vec<&Value> = match conn {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            };
            for t in targets {
                if let Some(tid) = t.get("@id").and_then(Value::as_str) {
                    doc.edges
                        .insert((loc.to_owned(), strip(tid, &prefix).to_owned()));
                }
            }
        }
    }
    doc
}
