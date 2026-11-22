//! The groundings of one document's values that every owner listing their node shares (issue
//! #749).
//!
//! A parameter node that many owners list was grounded again by each: a leaf in resolver Step 7,
//! a composite in lowering, a guard scope in the specialization pass, and each composite parsed it
//! again for its declarations' dependency order. Each grounding lexed the value's text and built
//! its result anew, so a long string value cost `owners × text` of lexing and allocation while
//! each further owner cost the document a reference: 1,024 leaves listing one 60 KB value, a
//! 702 KB document, requested 1.31 GB. [`DocumentGroundings`] grounds such a value once and hands
//! the result to every later owner.
//!
//! # What is shared, and why it is exact
//!
//! A value of a node two owners or more list, written as an expression or a typed literal
//! (a literal grounds at no cost and is never kept), is kept after its first grounding when that
//! grounding
//!
//! - looked up no name: the evaluator consults its scope only to look a name up, to resolve an
//!   enumeration (the same catalog tables under every scope of the resolver) and to ask whether
//!   non-finite values are refused, so a grounding that looked no name up is the one every
//!   owner's scope gives; and
//! - was not refused for the evaluation budget: such a refusal names the budget that ran out,
//!   the evaluation's own cap or the document's, and which one a later owner would meet depends
//!   on the fuel left to it.
//!
//! # The evaluation budget is charged as before
//!
//! The document's evaluation budget (`MAX_DOCUMENT_EVAL_FUEL`) is charged for every owner as if
//! the owner had grounded the value itself, so sharing changes no outcome. The first grounding's
//! cost is the fuel it spent ([`oce_expr::eval_fuel_remaining`] before and after); an evaluation
//! is deterministic, so evaluating again to the same end spends exactly that again. A later owner
//! takes the kept result only when the fuel left covers that cost, and is charged it
//! ([`oce_expr::spend_eval_fuel`]). When the fuel left does not cover it, evaluating again would
//! refuse partway, at a point only evaluating again finds, so that owner grounds the value itself,
//! exactly as before. (In that regime, once the budget is nearly spent, each owner lexes the value
//! again; issue #798 charges lexing to the budget.)
//!
//! # What the caches hold
//!
//! Nothing whose size the document has not paid for. A kept scalar result is built from its
//! value's own text: the evaluator makes a string only from a literal, which a kept result shares
//! (`ExprAst::Str` holds an `Arc<str>`), and an error quotes at most a token, a name or the
//! lexical form of its text. Kept identifier heads are names of the text. A kept array evaluation
//! is kept only when its element count is the count the document's element budget charged for
//! the owner that evaluated it (`ArrayElementBudget`); an evaluation of any other length, or a
//! refusal, is dropped once its owner has read it. Only nodes two owners list are kept for, so
//! every kept value is one each of those owners' tables would otherwise ground again. One pass
//! (specialization, composite lowering, leaf grounding) keeps one cache over the nodes it reads,
//! since each pass reads its own copy of the document. Looked up, never iterated.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use oce_expr::{EvalResult, ExprError, Scope};
use oce_model::{EnumClassId, Value};

use crate::dto::{CxfValue, Node};
use crate::ground::{GroundErr, ground_param_value};
use crate::parameter_reads::Grounded;

/// A result kept for later owners, with the fuel its grounding spent.
struct Kept<T> {
    result: T,
    fuel: u64,
}

/// The groundings of one pass over one document that every owner listing their node shares; see
/// the module documentation.
pub(crate) struct DocumentGroundings {
    /// The parameter nodes more than one owner lists, by address.
    shared: BTreeSet<*const Node>,
    /// The kept scalar groundings, keyed by the address of the value in the document.
    kept: BTreeMap<*const CxfValue, Kept<Result<Value, GroundErr>>>,
    /// The kept array evaluations, keyed the same way: each an array of the element count the
    /// element budget charged for the owner that evaluated it.
    arrays: BTreeMap<*const CxfValue, Kept<Result<EvalResult, ExprError>>>,
    /// The kept identifier heads of declaration expressions, keyed the same way.
    heads: BTreeMap<*const CxfValue, Vec<String>>,
}

impl DocumentGroundings {
    /// Nothing kept yet, for owners `owners` whose parameter nodes `by_id` finds: a node any two
    /// of them list (`hasParameter` and `hasConstant`, an owner counted once however often it
    /// lists the node) is shared. Linear in the listings.
    pub(crate) fn of<'n>(
        owners: impl IntoIterator<Item = &'n Node>,
        by_id: &HashMap<&str, &Node>,
    ) -> Self {
        let mut owners_of: HashMap<&str, (usize, &str)> = HashMap::new();
        let mut shared = BTreeSet::new();
        for owner in owners {
            for listed in owner.has_parameter.iter().chain(owner.has_constant.iter()) {
                let iri = listed.id.as_str();
                let entry = owners_of.entry(iri).or_insert((0, owner.id.as_str()));
                if entry.0 == 0 || !std::ptr::eq(entry.1, owner.id.as_str()) {
                    entry.0 += 1;
                    entry.1 = owner.id.as_str();
                }
                if entry.0 == 2
                    && let Some(node) = by_id.get(iri)
                {
                    shared.insert(std::ptr::from_ref(*node));
                }
            }
        }
        Self {
            shared,
            kept: BTreeMap::new(),
            arrays: BTreeMap::new(),
            heads: BTreeMap::new(),
        }
    }

    /// Whether the document keeps `value`, a value of `node`, for later owners.
    pub(crate) fn shares(&self, node: &Node, value: &CxfValue) -> bool {
        matches!(value, CxfValue::Expr(_) | CxfValue::Typed { .. })
            && self.shared.contains(&std::ptr::from_ref(node))
    }

    /// The result kept for `value`, without a charge: for a reading the caller already paid for.
    pub(crate) fn kept(&self, value: &CxfValue) -> Option<&Result<Value, GroundErr>> {
        self.kept
            .get(&std::ptr::from_ref(value))
            .map(|kept| &kept.result)
    }

    /// One owner's reading of `value`, a value of the parameter node `node`, against `scope`:
    /// the kept result, charged the fuel its grounding spent, when the document keeps one and the
    /// fuel left covers it; otherwise the owner's own grounding, kept for later owners when it
    /// may be (see the module documentation). `Kept` when the result is the document's.
    pub(crate) fn ground(
        &mut self,
        node: &Node,
        value: &CxfValue,
        scope: &dyn Scope,
    ) -> Grounded<'_> {
        if !self.shares(node, value) {
            return Grounded::Fresh(evaluated(value, scope));
        }
        let key = std::ptr::from_ref(value);
        let reuse = self.kept.get(&key).map(|kept| spent_again(kept.fuel));
        match reuse {
            Some(true) => match self.kept.get(&key) {
                Some(kept) => Grounded::Kept(&kept.result),
                None => Grounded::Fresh(evaluated(value, scope)),
            },
            Some(false) => Grounded::Fresh(evaluated(value, scope)),
            None => {
                let before = oce_expr::eval_fuel_remaining();
                let (read_a_name, result) = ground_reading_names(value, scope);
                let fuel = fuel_spent(before);
                if read_a_name || refused_for_fuel(&result) {
                    return Grounded::Fresh(result);
                }
                Grounded::Kept(&self.kept.entry(key).or_insert(Kept { result, fuel }).result)
            }
        }
    }

    /// The identifier heads of the expression `text`, the value `value` of the declaration node
    /// `node` ([`crate::resolve::identifier_heads`], what a composite's or a guard scope's
    /// dependency order reads): parsed once for the document when the node is shared, since they
    /// depend on the text alone, and for each read otherwise. Parsing is not metered.
    pub(crate) fn identifier_heads(
        &mut self,
        node: &Node,
        value: &CxfValue,
        text: &str,
    ) -> Grounded<'_, Vec<String>> {
        let parse = || {
            #[cfg(test)]
            HEADS_PARSED.with(|parsed| parsed.set(parsed.get() + 1));
            crate::resolve::identifier_heads(text)
        };
        if !self.shares(node, value) {
            return Grounded::Fresh(parse());
        }
        Grounded::Kept(
            self.heads
                .entry(std::ptr::from_ref(value))
                .or_insert_with(parse),
        )
    }

    /// One owner's evaluation of the array expression `text`, the value `value` of the array
    /// parameter node `node`, against `scope`, for an owner whose declared shape holds `expected`
    /// elements (already charged to the element budget): the kept evaluation, charged the fuel it
    /// spent, when the document keeps one and the fuel left covers it; otherwise the owner's own.
    /// An evaluation is kept only when it is an array of exactly `expected` elements, read no
    /// name and was not refused; the caller checks it against its own shape either way.
    pub(crate) fn evaluate_array(
        &mut self,
        node: &Node,
        value: &CxfValue,
        text: &str,
        expected: usize,
        scope: &dyn Scope,
    ) -> Grounded<'_, Result<EvalResult, ExprError>> {
        let evaluate = |scope: &dyn Scope| {
            #[cfg(test)]
            count_grounding();
            oce_expr::eval_str_finite(text, scope)
        };
        if !self.shares(node, value) {
            return Grounded::Fresh(evaluate(scope));
        }
        let key = std::ptr::from_ref(value);
        let reuse = self.arrays.get(&key).map(|kept| spent_again(kept.fuel));
        match reuse {
            Some(true) => match self.arrays.get(&key) {
                Some(kept) => Grounded::Kept(&kept.result),
                None => Grounded::Fresh(evaluate(scope)),
            },
            Some(false) => Grounded::Fresh(evaluate(scope)),
            None => {
                let before = oce_expr::eval_fuel_remaining();
                let reading = NameReads {
                    inner: scope,
                    read: Cell::new(false),
                };
                let result = evaluate(&reading);
                let fuel = fuel_spent(before);
                match result {
                    Ok(EvalResult::Array(array))
                        if !reading.read.get() && array.len() == expected =>
                    {
                        let kept = self.arrays.entry(key).or_insert(Kept {
                            result: Ok(EvalResult::Array(array)),
                            fuel,
                        });
                        Grounded::Kept(&kept.result)
                    }
                    other => Grounded::Fresh(other),
                }
            }
        }
    }
}

/// What one [`DocumentGroundings`] holds, for the tests that bound it.
#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Retained {
    /// Kept scalar results and errors.
    pub(crate) scalars: usize,
    /// The bytes of the kept string results.
    pub(crate) string_bytes: usize,
    /// Kept array evaluations.
    pub(crate) arrays: usize,
    /// The elements of the kept array evaluations.
    pub(crate) array_elements: usize,
    /// The bytes of the kept identifier heads.
    pub(crate) head_bytes: usize,
}

#[cfg(test)]
impl DocumentGroundings {
    /// What the caches hold now.
    pub(crate) fn retained(&self) -> Retained {
        let string_bytes = self
            .kept
            .values()
            .map(|kept| match &kept.result {
                Ok(Value::String(text)) => text.len(),
                _ => 0,
            })
            .sum();
        let array_elements = self
            .arrays
            .values()
            .map(|kept| match &kept.result {
                Ok(EvalResult::Array(array)) => array.len(),
                _ => 0,
            })
            .sum();
        Retained {
            scalars: self.kept.len(),
            string_bytes,
            arrays: self.arrays.len(),
            array_elements,
            head_bytes: self.heads.values().flatten().map(String::len).sum(),
        }
    }
}

/// Whether the fuel left covers `fuel` units, in which case they are spent: what a reading that
/// takes a kept result is charged, the units grounding it again would spend. Fewer left spends
/// nothing. Outside every metered frame nothing is charged.
fn spent_again(fuel: u64) -> bool {
    oce_expr::spend_eval_fuel(fuel).is_ok()
}

/// The fuel spent since the remainder was `before`.
fn fuel_spent(before: Option<u64>) -> u64 {
    match (before, oce_expr::eval_fuel_remaining()) {
        (Some(before), Some(after)) => before.saturating_sub(after),
        _ => 0,
    }
}

/// Whether `result` is a refusal for the evaluation budget.
fn refused_for_fuel(result: &Result<Value, GroundErr>) -> bool {
    matches!(
        result,
        Err(GroundErr::Expr(ExprError::ExpressionTooExpensive { .. }))
    )
}

/// Ground `value` against `scope`, counted in test builds (`reads_counted_by`).
pub(crate) fn evaluated(value: &CxfValue, scope: &dyn Scope) -> Result<Value, GroundErr> {
    #[cfg(test)]
    count_grounding();
    ground_param_value(value, scope)
}

/// Ground `value` against `scope` and report whether the grounding looked a name up.
pub(crate) fn ground_reading_names(
    value: &CxfValue,
    scope: &dyn Scope,
) -> (bool, Result<Value, GroundErr>) {
    let reading = NameReads {
        inner: scope,
        read: Cell::new(false),
    };
    let result = evaluated(value, &reading);
    (reading.read.get(), result)
}

/// A scope that answers as `inner` does and records whether anything looked a name up.
struct NameReads<'s> {
    inner: &'s dyn Scope,
    read: Cell<bool>,
}

impl Scope for NameReads<'_> {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        self.read.set(true);
        self.inner.lookup(name)
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        self.inner.enum_class(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        self.inner.enum_ordinal(class, literal)
    }

    fn requires_finite(&self) -> bool {
        self.inner.requires_finite()
    }
}

#[cfg(test)]
use crate::parameter_reads::count_grounding;

#[cfg(test)]
thread_local! {
    /// The declaration expressions [`DocumentGroundings::identifier_heads`] parsed on this
    /// thread, counted in test builds only.
    static HEADS_PARSED: Cell<u64> = const { Cell::new(0) };
}

/// Run `run` and return how many declaration expressions it parsed for their identifier heads
/// through [`DocumentGroundings`] on this thread, with its result.
#[cfg(test)]
pub(crate) fn heads_parsed_by<R>(run: impl FnOnce() -> R) -> (u64, R) {
    HEADS_PARSED.with(|parsed| parsed.set(0));
    let result = run();
    (HEADS_PARSED.with(Cell::get), result)
}

#[cfg(test)]
#[path = "document_groundings_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "document_groundings_parity_tests.rs"]
mod parity_tests;
