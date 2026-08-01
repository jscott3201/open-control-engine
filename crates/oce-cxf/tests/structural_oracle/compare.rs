//! The structural comparison itself: canonicalize our fixture against the flattened
//! oracle, resolve conditionals on both sides against the fixture's own parameter
//! values, bucket every disagreement, and produce an honest per-fixture verdict.
//!
//! Verdict tiers, from the reference implementation: EXACT (instances and undirected
//! edges match the conditionally-resolved oracle), EXACT-XFOLD (exact outside an
//! excluded constant-fold subtree), RESIDUAL (real defects), UNKNOWNS
//! (something could not be verified). Excluded fixtures are never counted as passes.
//!
//! Connector-condition unknowns follow the material/immaterial rule (brief Amendment
//! 2026-07-28-E): material — the unknown could have decided an otherwise-alive edge —
//! counts in the unknowns column and blocks verification; immaterial — every touching
//! ref edge is dead via its other endpoint — is reported as a named note.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::conditions::{EvalError, MoIndex, ParamValue, evaluate};
use super::flatten::{Ctx, Flat};

/// Composite classes whose fixture-side treatment is a verified parameter-specialized
/// reduction: their subtrees are excluded from comparison on both sides.
pub const FOLD_CLASSES: &[&str] =
    &["Buildings.Controls.OBC.ASHRAE.G36.Generic.AirEconomizerHighLimits"];

/// Everything `analyse` measures for one fixture.
#[derive(Debug, Default)]
pub struct Analysis {
    pub required: usize,
    pub ourinst: usize,
    pub absent_verified: usize,
    pub folds: Vec<String>,
    pub fold_inst: usize,
    /// Our-side nodes under a fold prefix with no oracle counterpart (reported, not compared).
    pub fold_ours: usize,
    pub missing: Vec<(String, String)>,
    pub dangling: Vec<String>,
    pub extra: Vec<String>,
    pub clsmis: Vec<(String, String, String)>,
    pub refedges: usize,
    pub matched: usize,
    pub flipped: usize,
    pub refonly: Vec<(String, String)>,
    pub ouronly: Vec<(String, String)>,
    pub unknown_inst: Vec<String>,
    pub unknown_edges: Vec<(String, String)>,
    pub unknown_conn: Vec<String>,
    pub unknown_conn_note: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Required,
    AbsentVerified,
    Unknown,
    ExcludedFold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConnState {
    Alive,
    Dead,
    /// Unevaluable declaration condition — treated alive, recorded for accounting.
    Unknown(String),
}

/// Memoized conditional-resolution engine for one fixture against one flattened class.
struct Eval<'a> {
    flat: &'a Flat,
    mo: &'a mut MoIndex,
    env: &'a BTreeMap<String, ParamValue>,
    class: &'a str,
    folds: &'a [String],
    status: BTreeMap<String, Status>,
    conn: BTreeMap<String, ConnState>,
}

impl Eval<'_> {
    fn in_fold(&self, path: &str) -> bool {
        self.folds
            .iter()
            .any(|f| path == f || path.starts_with(&format!("{f}.")))
    }

    fn own_condition(&mut self, path: &str) -> (Option<String>, String) {
        let (decl, scope) = match path.rsplit_once('.') {
            None => (self.flat.declared_by.get(path).cloned(), String::new()),
            Some((parent, _)) => (
                self.flat
                    .comps
                    .get(parent)
                    .cloned()
                    .or_else(|| self.flat.declared_by.get(path).cloned()),
                format!("{parent}."),
            ),
        };
        let name = path.rsplit('.').next().unwrap_or(path);
        match decl {
            None => (None, scope),
            Some(d) => (self.mo.conditional_of(&d, name), scope),
        }
    }

    /// Required / absent-verified / unknown along the whole ancestor chain.
    fn path_status(&mut self, path: &str) -> Status {
        if let Some(hit) = self.status.get(path) {
            return *hit;
        }
        let segs: Vec<String> = path.split('.').map(str::to_owned).collect();
        let mut unknown = false;
        let mut out = Status::Required;
        for k in 1..=segs.len() {
            let prefix = segs[..k].join(".");
            let (cond, scope) = self.own_condition(&prefix);
            match cond.as_deref() {
                None => {
                    if k == segs.len() {
                        unknown = true; // cannot rule on the leaf itself
                    }
                }
                Some("") => {}
                Some(c) => match evaluate(c, self.env, &scope) {
                    Ok(false) => {
                        out = Status::AbsentVerified;
                        break;
                    }
                    Ok(true) => {}
                    Err(EvalError::Unevaluable(_)) => unknown = true,
                    Err(EvalError::Syntax(msg)) => panic!("condition on {prefix}: {msg}"),
                },
            }
        }
        if out == Status::Required && unknown {
            out = Status::Unknown;
        }
        self.status.insert(path.to_owned(), out);
        out
    }

    /// Aliveness of a connector endpoint under its own declaration condition.
    fn connector_state(&mut self, ep: &str) -> ConnState {
        if let Some(hit) = self.conn.get(ep) {
            return hit.clone();
        }
        let out = self.connector_state_uncached(ep);
        self.conn.insert(ep.to_owned(), out.clone());
        out
    }

    fn connector_state_uncached(&mut self, ep: &str) -> ConnState {
        let (cond, scope) = if let Some((cont, port)) = ep.rsplit_once('.') {
            let Some(ccls) = self.flat.comps.get(cont).cloned() else {
                return ConnState::Alive; // elementary block port: never conditional
            };
            if self.path_status(cont) == Status::AbsentVerified {
                return ConnState::Dead;
            }
            (self.mo.conditional_of(&ccls, port), format!("{cont}."))
        } else {
            (self.mo.conditional_of(self.class, ep), String::new())
        };
        match cond.as_deref() {
            None | Some("") => ConnState::Alive,
            Some(c) => match evaluate(c, self.env, &scope) {
                Ok(true) => ConnState::Alive,
                Ok(false) => ConnState::Dead,
                Err(EvalError::Unevaluable(_)) => ConnState::Unknown(c.to_owned()),
                Err(EvalError::Syntax(msg)) => panic!("connector condition on {ep}: {msg}"),
            },
        }
    }

    /// An endpoint is dead when its instance (or containing instance) is verified
    /// absent or folded, or its connector condition verifiably evaluates false.
    fn endpoint_alive(&mut self, ep: &str) -> bool {
        for q in Self::inst_prefixes(ep) {
            if self.in_fold(&q) {
                return false;
            }
            if self.flat.inst.contains_key(&q) && self.path_status(&q) == Status::AbsentVerified {
                return false;
            }
        }
        self.connector_state(ep) != ConnState::Dead
    }

    fn endpoint_unknown(&mut self, ep: &str) -> bool {
        Self::inst_prefixes(ep)
            .into_iter()
            .any(|q| self.flat.inst.contains_key(&q) && self.path_status(&q) == Status::Unknown)
    }

    fn inst_prefixes(ep: &str) -> Vec<String> {
        let mut v = vec![ep.to_owned()];
        if let Some((c, _)) = ep.rsplit_once('.') {
            v.push(c.to_owned());
        }
        v
    }
}

fn und(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

fn scope_of(loc: &str) -> String {
    match loc.rsplit_once('.') {
        Some((parent, _)) => format!("{parent}."),
        None => String::new(),
    }
}

/// Canonicalize one edge endpoint against the flattened oracle. Public so the port-name
/// hazard is directly pinned by unit tests: a numeric-suffixed port collapses to its
/// stem ONLY when the oracle's own inventory declares the stem — `logSwi.u2` is a real
/// port and an undeclared `u3` must stay visible as-is, never silently alias `u`.
pub fn canon_endpoint(flat: &Flat, canon_path: &dyn Fn(&str) -> String, ep: &str) -> String {
    if !ep.contains('.') {
        if flat.topports.contains(ep) {
            return ep.to_owned();
        }
        return match array_stem(ep).filter(|s| flat.topports.contains(s)) {
            Some(s) => s,
            None => ep.to_owned(),
        };
    }
    let (cont, port) = ep.rsplit_once('.').expect("dotted endpoint");
    let ccont = canon_path(cont);
    match flat.inv.get(&ccont) {
        Some(ports) if !ports.contains(port) => {
            // vector-port element: strip trailing digits only when the oracle's own
            // inventory declares the stem (never strip blindly)
            let stem = port.trim_end_matches(|c: char| c.is_ascii_digit());
            if !stem.is_empty() && stem != port && ports.contains(stem) {
                format!("{ccont}.{stem}")
            } else {
                format!("{ccont}.{port}")
            }
        }
        _ => format!("{ccont}.{port}"),
    }
}

/// `abs1_3` → `abs1`; `yRelFan_3` → `yRelFan`; `abs1` → None (no `_N` suffix).
fn array_stem(seg: &str) -> Option<String> {
    let trimmed = seg.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.len() == seg.len() {
        return None;
    }
    trimmed
        .strip_suffix('_')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn param_value(v: Option<&Value>) -> Option<ParamValue> {
    match v? {
        Value::Bool(b) => Some(ParamValue::Bool(*b)),
        Value::String(s) => Some(ParamValue::Enum(s.clone())),
        Value::Number(n) => n.as_f64().map(ParamValue::Num),
        Value::Object(o) => o
            .get("@value")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<f64>().ok())
            .map(ParamValue::Num),
        _ => None,
    }
}

pub fn analyse(
    ctx: &mut Ctx,
    mo: &mut MoIndex,
    class: &str,
    graph: &[Value],
    root: &str,
) -> Analysis {
    let flat = ctx.flatten(class);
    let prefix = format!("{root}.");
    let strip = |id: &str| -> String { id.strip_prefix(&prefix).unwrap_or(id).to_owned() };

    // ---- our side: instances, parameter env, declared conditionals ----
    let mut ourinst: BTreeMap<String, String> = BTreeMap::new();
    let mut env: BTreeMap<String, ParamValue> = BTreeMap::new();
    let mut declared_cond: BTreeMap<String, String> = BTreeMap::new();
    for node in graph {
        let id = node.get("@id").and_then(Value::as_str).unwrap_or_default();
        let loc = strip(id);
        if node
            .get("S231:isConditionalComponent")
            .is_some_and(|v| v.as_bool() == Some(true))
        {
            let cond = node
                .get("S231:conditionalExpression")
                .and_then(Value::as_str)
                .unwrap_or_default();
            declared_cond.insert(loc.clone(), cond.to_owned());
        }
        match node.get("@type").and_then(Value::as_str) {
            Some(t) if t.contains("CDL.") => {
                ourinst.insert(loc, t.replace("http://example.org#", ""));
            }
            Some("S231:Parameter") => {
                if let Some(v) = param_value(node.get("S231:value")) {
                    env.insert(loc, v);
                }
            }
            _ => {}
        }
    }

    // resolve our declared conditionals the way the importer does: a false condition
    // removes the component (block or connector) from the effective document
    let mut our_inactive: BTreeSet<String> = BTreeSet::new();
    for (loc, cond) in &declared_cond {
        if cond.is_empty() {
            continue;
        }
        match evaluate(cond, &env, &scope_of(loc)) {
            Ok(false) => {
                our_inactive.insert(loc.clone());
            }
            Ok(true) | Err(EvalError::Unevaluable(_)) => {} // visible, like the importer's bias
            Err(EvalError::Syntax(msg)) => panic!("our-side conditionalExpression on {loc}: {msg}"),
        }
    }
    ourinst.retain(|k, _| {
        !our_inactive.contains(k) && !our_inactive.iter().any(|p| k.starts_with(&format!("{p}.")))
    });

    // ---- canonicalization guided by the flattened oracle ----
    let mut allpref: BTreeSet<String> = BTreeSet::new();
    for p in flat.inst.keys().chain(flat.comps.keys()) {
        let segs: Vec<&str> = p.split('.').collect();
        for i in 1..=segs.len() {
            allpref.insert(segs[..i].join("."));
        }
    }
    let canon_path = |path: &str| -> String {
        let mut out: Vec<String> = Vec::new();
        for seg in path.split('.') {
            let joined = |s: &str| {
                if out.is_empty() {
                    s.to_owned()
                } else {
                    format!("{}.{}", out.join("."), s)
                }
            };
            if allpref.contains(&joined(seg)) {
                out.push(seg.to_owned());
            } else if let Some(stem) = array_stem(seg).filter(|s| allpref.contains(&joined(s))) {
                out.push(stem);
            } else {
                out.push(seg.to_owned());
            }
        }
        out.join(".")
    };
    let canon = |ep: &str| -> String { canon_endpoint(&flat, &canon_path, ep) };

    // ---- conditional resolution of the flattened oracle ----
    let folds: Vec<String> = flat
        .comps
        .iter()
        .filter(|(_, c)| FOLD_CLASSES.contains(&c.as_str()))
        .map(|(p, _)| p.clone())
        .collect();
    let mut ev = Eval {
        flat: &flat,
        mo,
        env: &env,
        class,
        folds: &folds,
        status: BTreeMap::new(),
        conn: BTreeMap::new(),
    };

    let mut status: BTreeMap<String, Status> = BTreeMap::new();
    for path in flat.inst.keys() {
        let st = if ev.in_fold(path) {
            Status::ExcludedFold
        } else {
            ev.path_status(path)
        };
        status.insert(path.clone(), st);
    }

    // ---- instance layer ----
    let mut bycanon: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for op in ourinst.keys() {
        bycanon.entry(canon_path(op)).or_default().push(op.clone());
    }
    let required: Vec<String> = status
        .iter()
        .filter(|(_, s)| **s == Status::Required)
        .map(|(p, _)| p.clone())
        .collect();
    let missing: Vec<(String, String)> = required
        .iter()
        .filter(|p| !bycanon.contains_key(*p))
        .map(|p| {
            let ann = match ev.own_condition(p).0.as_deref() {
                None => "decl-not-found".to_owned(),
                Some("") => "UNCONDITIONAL".to_owned(),
                Some(c) => format!("if {c}"),
            };
            (p.clone(), ann)
        })
        .collect();
    let unknown_inst: Vec<String> = status
        .iter()
        .filter(|(_, s)| **s == Status::Unknown)
        .map(|(p, _)| p.clone())
        .collect();
    let dangling: Vec<String> = status
        .iter()
        .filter(|(p, s)| **s == Status::AbsentVerified && bycanon.contains_key(*p))
        .map(|(p, _)| p.clone())
        .collect();
    let extra: Vec<String> = bycanon
        .keys()
        .filter(|p| !flat.inst.contains_key(*p) && !ev.in_fold(p))
        .cloned()
        .collect();
    let fold_ours = bycanon
        .keys()
        .filter(|p| !flat.inst.contains_key(*p) && ev.in_fold(p))
        .count();
    let clsmis: Vec<(String, String, String)> = required
        .iter()
        .filter_map(|p| {
            bycanon.get(p).and_then(|ops| {
                ops.iter()
                    .find(|op| ourinst[*op] != flat.inst[p])
                    .map(|op| (p.clone(), flat.inst[p].clone(), ourinst[op].clone()))
            })
        })
        .collect();

    // ---- edge layer ----
    let mut our_edges: BTreeSet<(String, String)> = BTreeSet::new();
    for node in graph {
        let id = node.get("@id").and_then(Value::as_str).unwrap_or_default();
        if let Some(conn) = node.get("S231:isConnectedTo") {
            let targets: Vec<&Value> = match conn {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            };
            for t in targets {
                if let Some(tid) = t.get("@id").and_then(Value::as_str) {
                    our_edges.insert((canon(&strip(id)), canon(&strip(tid))));
                }
            }
        }
    }
    let fold_touch = |ev: &Eval, x: &str| {
        ev.in_fold(x)
            || x.rsplit_once('.')
                .map(|(c, _)| ev.in_fold(c))
                .unwrap_or(false)
    };
    let o_dir: BTreeSet<(String, String)> = our_edges
        .into_iter()
        .filter(|(a, b)| !fold_touch(&ev, a) && !fold_touch(&ev, b))
        .collect();

    let mut r_dir: BTreeSet<(String, String)> = BTreeSet::new();
    let mut r_unknown_dir: BTreeSet<(String, String)> = BTreeSet::new();
    for (a, b) in &flat.edges {
        let alive = (ev.endpoint_alive(a), ev.endpoint_alive(b));
        if alive.0 && alive.1 {
            r_dir.insert((a.clone(), b.clone()));
        } else {
            // an edge is "unknown" (rather than dead) when some endpoint's condition
            // could not be evaluated AND no endpoint is verifiably dead
            let unk = (ev.endpoint_unknown(a), ev.endpoint_unknown(b));
            let any_unknown = unk.0 || unk.1;
            let a_not_dead = alive.0 || unk.0;
            let b_not_dead = alive.1 || unk.1;
            if any_unknown && a_not_dead && b_not_dead {
                r_unknown_dir.insert((a.clone(), b.clone()));
            }
        }
    }

    let ru: BTreeSet<(String, String)> = r_dir.iter().map(|(a, b)| und(a, b)).collect();
    let ou: BTreeSet<(String, String)> = o_dir.iter().map(|(a, b)| und(a, b)).collect();
    let ruu: BTreeSet<(String, String)> = r_unknown_dir.iter().map(|(a, b)| und(a, b)).collect();
    let flipped = r_dir
        .iter()
        .filter(|(a, b)| {
            !o_dir.contains(&(a.clone(), b.clone())) && o_dir.contains(&(b.clone(), a.clone()))
        })
        .count();
    let refonly: Vec<(String, String)> = ru
        .difference(&ou)
        .filter(|e| !ruu.contains(*e))
        .cloned()
        .collect();
    let unknown_edges: Vec<(String, String)> = ruu.difference(&ou).cloned().collect();
    let ouronly: Vec<(String, String)> = ou
        .iter()
        .filter(|e| !ru.contains(*e) && !ruu.contains(*e))
        .cloned()
        .collect();

    // ---- material vs immaterial connector unknowns (Amendment 2026-07-28-E) ----
    let unknown_eps: Vec<String> = ev
        .conn
        .iter()
        .filter(|(_, s)| matches!(s, ConnState::Unknown(_)))
        .map(|(ep, _)| ep.clone())
        .collect();
    let mut unknown_conn: Vec<String> = Vec::new();
    let mut unknown_conn_note: Vec<String> = Vec::new();
    for ep in unknown_eps {
        let touches_ru = ru.iter().any(|(a, b)| *a == ep || *b == ep);
        let all_dead = flat
            .edges
            .iter()
            .filter(|(a, b)| *a == ep || *b == ep)
            .all(|(a, b)| {
                [a, b]
                    .into_iter()
                    .filter(|x| **x != ep)
                    .all(|x| !ev.endpoint_alive(x) && !ev.endpoint_unknown(x))
            });
        if touches_ru && !all_dead {
            unknown_conn.push(ep);
        } else {
            unknown_conn_note.push(ep);
        }
    }

    Analysis {
        required: required.len(),
        ourinst: ourinst.len(),
        absent_verified: status
            .values()
            .filter(|s| **s == Status::AbsentVerified)
            .count(),
        fold_inst: status
            .values()
            .filter(|s| **s == Status::ExcludedFold)
            .count(),
        fold_ours,
        folds,
        missing,
        dangling,
        extra,
        clsmis,
        refedges: ru.len(),
        matched: ru.intersection(&ou).count(),
        flipped,
        refonly,
        ouronly,
        unknown_inst,
        unknown_edges,
        unknown_conn,
        unknown_conn_note,
    }
}

impl Analysis {
    pub fn defects(&self) -> usize {
        self.missing.len()
            + self.dangling.len()
            + self.extra.len()
            + self.clsmis.len()
            + self.refonly.len()
            + self.ouronly.len()
    }

    pub fn unknowns(&self) -> usize {
        self.unknown_inst.len() + self.unknown_edges.len() + self.unknown_conn.len()
    }

    /// Honest per-fixture verdict. Nothing excluded or representational is a pass.
    pub fn verdict(&self) -> &'static str {
        if self.defects() > 0 {
            "RESIDUAL"
        } else if self.unknowns() > 0 {
            "UNKNOWNS"
        } else if !self.folds.is_empty() {
            "EXACT-XFOLD"
        } else {
            "EXACT"
        }
    }
}
