//! The load gate refuses a vector element past its resolved length (GitHub #626), through
//! `Engine::load_cxf` on hand-written CXF documents. For each of the eight vector templates (`k`
//! on both `MultiSum` classes, `extract` on the three `ExtractSignal` classes, `msk` on the three
//! `VectorFilter` classes): a list longer than the length, a lone element past it, index 0, the
//! length left out (the class default applies, or the `Required` rule refuses its absence), the
//! length bound to a sequence parameter, and an in-length control that loads and runs with every
//! value it was given. Until this, each of the refusals below loaded with no warning and the
//! value was dropped: `MultiSum(nin = 2)` with gains `[1, 2, 3]` ran as `[1, 2]`.

use oce_api::{Engine, Value};

const BASE: &str = "http://example.org#doc";

/// A literal a parameter node or an array element holds.
#[derive(Clone, Copy)]
enum Lit {
    Real(f64),
    Integer(i64),
    Boolean(bool),
}

impl Lit {
    fn kind(self) -> &'static str {
        match self {
            Self::Real(_) => "Real",
            Self::Integer(_) => "Integer",
            Self::Boolean(_) => "Boolean",
        }
    }

    fn json(self) -> String {
        match self {
            Self::Real(v) => format!(
                "{{ \"@value\": \"{v:?}\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }}"
            ),
            Self::Integer(v) => v.to_string(),
            Self::Boolean(v) => v.to_string(),
        }
    }
}

/// A parameter node of the block under test.
enum Param {
    /// A scalar: its name and literal.
    Scalar(String, Lit),
    /// The length bound to the sequence parameter `n` (an Integer the root declares).
    Reference(&'static str),
    /// A preserved vector: its base and elements.
    Vector(&'static str, Vec<Lit>),
}

fn scalar(name: &str, value: Lit) -> Param {
    Param::Scalar(name.to_owned(), value)
}

fn node(id: &str, body: &str) -> String {
    format!("{{ \"@id\": \"{id}\", {body} }}")
}

fn ids(names: &[String]) -> String {
    names
        .iter()
        .map(|id| format!("{{ \"@id\": \"{id}\" }}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One block of `class` (`CDL.…`) with `params`, `inputs` boundary inputs `u1…` driving its
/// inputs `u1…` and its outputs (`y`, or `y1…`) driving boundary outputs of the same names, all
/// of signal kind `kind`; `n` is the sequence parameter a [`Param::Reference`] names.
fn document(
    class: &str,
    params: &[Param],
    kind: &str,
    inputs: usize,
    outputs: &[&str],
    n: i64,
) -> String {
    let block = format!("{BASE}.blk");
    let mut nodes = Vec::new();
    let mut parameters = Vec::new();
    for param in params {
        let (name, body) = match param {
            Param::Scalar(name, value) => (
                name.clone(),
                format!(
                    "\"@type\": \"S231:Parameter\", \"S231:isOfDataType\": {{ \"@id\": \
                     \"S231:{}\" }}, \"S231:value\": {}",
                    value.kind(),
                    value.json()
                ),
            ),
            Param::Reference(name) => (
                (*name).to_owned(),
                "\"@type\": \"S231:Parameter\", \"S231:isOfDataType\": { \"@id\": \
                 \"S231:Integer\" }, \"S231:value\": \"n\""
                    .to_owned(),
            ),
            Param::Vector(base, elements) => (
                (*base).to_owned(),
                format!(
                    "\"@type\": \"S231:Parameter\", \"S231:isOfDataType\": {{ \"@id\": \
                     \"S231:{}\" }}, \"S231:isArray\": true, \"S231:numberDimensions\": 1, \
                     \"S231:sizeOfDimensions\": \"({})\", \"S231:value\": [{}]",
                    elements[0].kind(),
                    elements.len(),
                    elements
                        .iter()
                        .map(|e| e.json())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
        };
        let id = format!("{block}.{name}");
        nodes.push(node(&id, &body));
        parameters.push(id);
    }
    let (mut root_inputs, mut block_inputs, mut block_outputs, mut root_outputs) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for i in 1..=inputs {
        let (outer, inner) = (format!("{BASE}.u{i}"), format!("{block}.u{i}"));
        nodes.push(node(
            &outer,
            &format!(
                "\"@type\": \"S231:{kind}Input\", \"S231:isOfDataType\": {{ \"@id\": \
                 \"S231:{kind}\" }}, \"S231:isConnectedTo\": [{{ \"@id\": \"{inner}\" }}]"
            ),
        ));
        nodes.push(node(
            &inner,
            &format!(
                "\"@type\": \"S231:{kind}Input\", \"S231:isOfDataType\": {{ \"@id\": \
                 \"S231:{kind}\" }}"
            ),
        ));
        root_inputs.push(outer);
        block_inputs.push(inner);
    }
    for name in outputs {
        let (inner, outer) = (format!("{block}.{name}"), format!("{BASE}.{name}"));
        nodes.push(node(
            &inner,
            &format!(
                "\"@type\": \"S231:{kind}Output\", \"S231:isOfDataType\": {{ \"@id\": \
                 \"S231:{kind}\" }}, \"S231:isConnectedTo\": [{{ \"@id\": \"{outer}\" }}]"
            ),
        ));
        nodes.push(node(
            &outer,
            &format!(
                "\"@type\": \"S231:{kind}Output\", \"S231:isOfDataType\": {{ \"@id\": \
                 \"S231:{kind}\" }}"
            ),
        ));
        block_outputs.push(inner);
        root_outputs.push(outer);
    }
    let sequence_parameter = format!("{BASE}.n");
    nodes.push(node(
        &sequence_parameter,
        &format!(
            "\"@type\": \"S231:Parameter\", \"S231:isOfDataType\": {{ \"@id\": \
             \"S231:Integer\" }}, \"S231:value\": {n}"
        ),
    ));
    let root = node(
        BASE,
        &format!(
            "\"@type\": \"S231:Block\", \"S231:hasParameter\": [{}], \"S231:hasInput\": [{}], \
             \"S231:hasOutput\": [{}], \"S231:containsBlock\": [{{ \"@id\": \"{block}\" }}]",
            ids(&[sequence_parameter]),
            ids(&root_inputs),
            ids(&root_outputs)
        ),
    );
    let instance = node(
        &block,
        &format!(
            "\"@type\": \"http://example.org#Buildings.Controls.OBC.{class}\", \
             \"S231:hasParameter\": [{}], \"S231:hasInput\": [{}], \"S231:hasOutput\": [{}]",
            ids(&parameters),
            ids(&block_inputs),
            ids(&block_outputs)
        ),
    );
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\" }}, \"@graph\": [{root}, \
         {instance}, {}] }}",
        nodes.join(", ")
    )
}

/// The refusal of a document, as `(code, subject, message)`; panics if it loads.
fn refusal(document: &str) -> Vec<(String, String, String)> {
    match Engine::new().load_cxf(document.as_bytes()) {
        Ok(_) => panic!("loaded:\n{document}"),
        Err(error) => error
            .diagnostics()
            .iter()
            .map(|d| {
                (
                    d.code.as_str().to_owned(),
                    d.subject.as_deref().unwrap_or("").to_owned(),
                    d.message.clone(),
                )
            })
            .collect(),
    }
}

/// The refusal of an element past the length, at the block.
fn past(class: &str, name: &str, n: usize) -> (String, String, String) {
    (
        "parameter-out-of-range".to_owned(),
        format!("{BASE}.blk"),
        format!("parameter `{name}` on block `{class}` is outside resolved array length {n}"),
    )
}

/// One vector template as the document builder needs it.
struct Template {
    class: String,
    base: &'static str,
    /// The length parameter, and the other parameters every case carries.
    len: &'static str,
    other: Vec<(&'static str, i64)>,
    /// The length every case but the omitted one uses.
    n: usize,
    signal: &'static str,
    inputs: usize,
    outputs: Vec<&'static str>,
    /// An element value inside the length, and the one written past it.
    element: Lit,
}

fn templates() -> Vec<Template> {
    let mut all = Vec::new();
    for (family, signal, element) in [
        ("Reals", "Real", Lit::Real(3.0)),
        ("Integers", "Integer", Lit::Integer(3)),
    ] {
        all.push(Template {
            class: format!("CDL.{family}.MultiSum"),
            base: "k",
            len: "nin",
            other: Vec::new(),
            n: 2,
            signal,
            inputs: 2,
            outputs: vec!["y"],
            element,
        });
    }
    for signal in ["Real", "Integer", "Boolean"] {
        all.push(Template {
            class: format!("CDL.Routing.{signal}ExtractSignal"),
            base: "extract",
            len: "nout",
            other: vec![("nin", 2)],
            n: 1,
            signal,
            inputs: 2,
            outputs: vec!["y1"],
            element: Lit::Integer(1),
        });
        // `nout = 2`: the two members inside the length default to `true`, which is the count.
        all.push(Template {
            class: format!("CDL.Routing.{signal}VectorFilter"),
            base: "msk",
            len: "nin",
            other: vec![("nout", 2)],
            n: 2,
            signal,
            inputs: 2,
            outputs: vec!["y1", "y2"],
            element: Lit::Boolean(true),
        });
    }
    all
}

impl Template {
    /// The document with the length (`Some` as a literal, `None` as the reference to `n`) and
    /// `elements`.
    fn document(&self, len: Option<usize>, elements: Vec<Param>, n: usize) -> String {
        let mut params: Vec<Param> = self
            .other
            .iter()
            .map(|(name, v)| scalar(name, Lit::Integer(*v)))
            .collect();
        params.push(match len {
            Some(len) => scalar(self.len, Lit::Integer(i64::try_from(len).expect("fits"))),
            None => Param::Reference(self.len),
        });
        params.extend(elements);
        document(
            &self.class,
            &params,
            self.signal,
            self.inputs,
            &self.outputs,
            i64::try_from(n).expect("fits"),
        )
    }

    fn element(&self, index: usize) -> Param {
        scalar(&format!("{}_{index}", self.base), self.element)
    }

    /// A list one longer than the length, each element inside it the in-length value.
    fn longer_list(&self) -> Param {
        Param::Vector(self.base, vec![self.element; self.n + 1])
    }
}

#[test]
fn a_list_longer_than_the_length_is_refused_at_its_extra_element() {
    for t in &templates() {
        assert_eq!(
            refusal(&t.document(Some(t.n), vec![t.longer_list()], t.n)),
            vec![past(&t.class, &format!("{}_{}", t.base, t.n + 1), t.n)],
            "{}",
            t.class
        );
    }
}

#[test]
fn a_lone_element_past_the_length_is_refused() {
    for t in &templates() {
        assert_eq!(
            refusal(&t.document(Some(t.n), vec![t.element(t.n + 1)], t.n)),
            vec![past(&t.class, &format!("{}_{}", t.base, t.n + 1), t.n)],
            "{}",
            t.class
        );
    }
}

#[test]
fn index_zero_is_refused() {
    for t in &templates() {
        assert_eq!(
            refusal(&t.document(Some(t.n), vec![t.element(0)], t.n)),
            vec![past(&t.class, &format!("{}_0", t.base), t.n)],
            "{}",
            t.class
        );
    }
}

#[test]
fn a_length_bound_to_a_sequence_parameter_is_the_parameters_value() {
    for t in &templates() {
        assert_eq!(
            refusal(&t.document(None, vec![t.longer_list()], t.n)),
            vec![past(&t.class, &format!("{}_{}", t.base, t.n + 1), t.n)],
            "{}",
            t.class
        );
    }
}

#[test]
fn an_omitted_length_takes_the_class_default() {
    // `MultiSum.nin` defaults to 0: the block has no inputs, and every gain is past the length.
    for (family, signal, element) in [
        ("Reals", "Real", Lit::Real(1.0)),
        ("Integers", "Integer", Lit::Integer(1)),
    ] {
        let class = format!("CDL.{family}.MultiSum");
        let doc = document(
            &class,
            &[Param::Vector("k", vec![element, element])],
            signal,
            0,
            &["y"],
            2,
        );
        assert_eq!(
            refusal(&doc),
            vec![past(&class, "k_1", 0), past(&class, "k_2", 0)],
            "{class}"
        );
    }
    // `ExtractSignal.nout` defaults to 1.
    for signal in ["Real", "Integer", "Boolean"] {
        let class = format!("CDL.Routing.{signal}ExtractSignal");
        let doc = document(
            &class,
            &[
                scalar("nin", Lit::Integer(2)),
                Param::Vector("extract", vec![Lit::Integer(2), Lit::Integer(1)]),
            ],
            signal,
            2,
            &["y1"],
            2,
        );
        assert_eq!(refusal(&doc), vec![past(&class, "extract_2", 1)], "{class}");
    }
}

#[test]
fn a_required_length_left_out_is_refused_as_missing_and_judges_no_element() {
    // `VectorFilter.nin` has no default; its absence is the refusal, and no mask member is
    // judged against a length nobody wrote.
    for signal in ["Real", "Integer", "Boolean"] {
        let class = format!("CDL.Routing.{signal}VectorFilter");
        let doc = document(
            &class,
            &[
                scalar("nout", Lit::Integer(1)),
                scalar("msk_9", Lit::Boolean(true)),
            ],
            signal,
            0,
            &["y1"],
            2,
        );
        assert_eq!(
            refusal(&doc),
            vec![(
                "missing-required-parameter".to_owned(),
                format!("{BASE}.blk"),
                format!("block `{class}` is missing required parameter `nin`")
            )],
            "{class}"
        );
    }
}

/// Load `document`, run one frame with the inputs `u1`, `u2` at `values`, and return the
/// boundary outputs by local name.
fn run(document: &str, values: [Value; 2]) -> Vec<(String, Value)> {
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(document.as_bytes())
        .unwrap_or_else(|error| panic!("refused: {error}\n{document}"));
    assert_eq!(report.warnings.len(), 0, "{:?}", report.warnings);
    let entries = [
        (format!("{BASE}.u1"), values[0].clone()),
        (format!("{BASE}.u2"), values[1].clone()),
    ];
    let entries: Vec<(&str, Value)> = entries
        .iter()
        .map(|(p, v)| (p.as_str(), v.clone()))
        .collect();
    let prepared = engine.prepare_frame(0.0, &entries).expect("prepares");
    let done = engine.execute_frame(prepared).expect("runs");
    done.outputs()
        .iter()
        .map(|(path, value)| {
            (
                path.strip_prefix(&format!("{BASE}."))
                    .unwrap_or(path)
                    .to_owned(),
                value.clone(),
            )
        })
        .collect()
}

/// `found` is exactly one output named `name`, bit for bit `expected`.
fn one_output(found: &[(String, Value)], name: &str, expected: &Value, what: &str) {
    assert_eq!(found.len(), 1, "{what}: {found:?}");
    assert_eq!(found[0].0, name, "{what}");
    assert!(found[0].1.bit_eq(expected), "{what}: {found:?}");
}

#[test]
fn an_in_length_control_loads_and_uses_every_value_it_was_given() {
    // Gains 10 and 20 on inputs 1 and 2 give 50: both gains are read.
    for (family, signal, gains, inputs, sum) in [
        (
            "Reals",
            "Real",
            vec![Lit::Real(10.0), Lit::Real(20.0)],
            [Value::Real(1.0), Value::Real(2.0)],
            Value::Real(50.0),
        ),
        (
            "Integers",
            "Integer",
            vec![Lit::Integer(10), Lit::Integer(20)],
            [Value::Integer(1), Value::Integer(2)],
            Value::Integer(50),
        ),
    ] {
        let doc = document(
            &format!("CDL.{family}.MultiSum"),
            &[scalar("nin", Lit::Integer(2)), Param::Vector("k", gains)],
            signal,
            2,
            &["y"],
            2,
        );
        one_output(&run(&doc, inputs), "y", &sum, family);
    }
    // `extract = [2]` selects the second input; the mask `[false, true]` keeps only it.
    for (signal, inputs) in [
        ("Real", [Value::Real(1.0), Value::Real(2.0)]),
        ("Integer", [Value::Integer(1), Value::Integer(2)]),
        ("Boolean", [Value::Boolean(false), Value::Boolean(true)]),
    ] {
        let second = inputs[1].clone();
        let extract = document(
            &format!("CDL.Routing.{signal}ExtractSignal"),
            &[
                scalar("nin", Lit::Integer(2)),
                scalar("nout", Lit::Integer(1)),
                Param::Vector("extract", vec![Lit::Integer(2)]),
            ],
            signal,
            2,
            &["y1"],
            2,
        );
        one_output(&run(&extract, inputs.clone()), "y1", &second, signal);
        let filter = document(
            &format!("CDL.Routing.{signal}VectorFilter"),
            &[
                scalar("nin", Lit::Integer(2)),
                scalar("nout", Lit::Integer(1)),
                Param::Vector("msk", vec![Lit::Boolean(false), Lit::Boolean(true)]),
            ],
            signal,
            2,
            &["y1"],
            2,
        );
        one_output(&run(&filter, inputs), "y1", &second, signal);
    }
}
