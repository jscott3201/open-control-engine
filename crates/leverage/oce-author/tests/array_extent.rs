//! An array element past its block's resolved length, through the authoring validator (GitHub
//! #626). Stage 1 inherits the load gate's refusal through `oce_api::class_ports`, which runs the
//! load gate's own parameter rules, so the finding carries the engine's code,
//! `parameter-out-of-range`, placed on the element: a list's extra value, or the element
//! assignment. Each of the eight vector templates, in every form: a list longer than the length,
//! a lone element past it, index 0, the length left out, the length bound to a sequence
//! parameter, and an in-length control that validates clean through both stages.

use oce_author::validate_text;

/// One vector template: its block line's class and fixed parameters, the element base, the
/// length's parameter and value, its signal kind and its ports.
struct Template {
    class: String,
    fixed: &'static str,
    base: &'static str,
    len: &'static str,
    n: usize,
    signal: &'static str,
    outputs: &'static [&'static str],
    /// An element value inside the length, as the text spells it.
    value: &'static str,
}

fn templates() -> Vec<Template> {
    let mut all = Vec::new();
    for (family, signal, value) in [("Reals", "Real", "1.5"), ("Integers", "Integer", "7")] {
        all.push(Template {
            class: format!("CDL.{family}.MultiSum"),
            fixed: "",
            base: "k",
            len: "nin",
            n: 2,
            signal,
            outputs: &["y"],
            value,
        });
    }
    for signal in ["Real", "Integer", "Boolean"] {
        all.push(Template {
            class: format!("CDL.Routing.{signal}ExtractSignal"),
            fixed: "nin = 2, ",
            base: "extract",
            len: "nout",
            n: 1,
            signal,
            outputs: &["y1"],
            value: "2",
        });
        // `nout = 2`: the members inside the length default to `true`, which is the count.
        all.push(Template {
            class: format!("CDL.Routing.{signal}VectorFilter"),
            fixed: "nout = 2, ",
            base: "msk",
            len: "nin",
            n: 2,
            signal,
            outputs: &["y1", "y2"],
            value: "true",
        });
    }
    all
}

/// The block statement stands on this line of every document.
const BLOCK_LINE: u32 = 8;

impl Template {
    /// The document with `body` inside the block's parentheses after the fixed parameters. The
    /// sequence parameter `n` holds the length.
    fn text(&self, body: &str) -> (String, String) {
        let block = format!("block b = {}({}{body})", self.class, self.fixed);
        let mut text = format!(
            "sequence s\nderived_from none\nparam n : Integer = {}\nin u1 : {sig}\nin u2 : {sig}\n",
            self.n,
            sig = self.signal
        );
        for output in self.outputs {
            text.push_str(&format!("out {output} : {}\n", self.signal));
        }
        while text.lines().count() < usize::try_from(BLOCK_LINE).expect("fits") - 1 {
            text.push_str("# padding\n");
        }
        text.push_str(&block);
        text.push_str("\nu1 -> b.u1\nu2 -> b.u2\n");
        for output in self.outputs {
            text.push_str(&format!("b.{output} -> {output}\n"));
        }
        (text, block)
    }

    fn list(&self, length: usize) -> String {
        format!("[{}]", vec![self.value; length].join(", "))
    }
}

type Placed = Vec<(String, String, u32, u32)>;

fn placed(text: &str) -> Placed {
    validate_text(text)
        .expect("parses")
        .iter()
        .map(|f| {
            let at = f.span.expect("placed");
            (f.code.to_owned(), f.subject.clone(), at.line, at.column)
        })
        .collect()
}

fn finding(code: &str, subject: &str, column: usize) -> (String, String, u32, u32) {
    (
        code.to_owned(),
        subject.to_owned(),
        BLOCK_LINE,
        u32::try_from(column).expect("fits"),
    )
}

/// The 1-based column of the last occurrence of `needle` in `line`.
fn column_of_last(line: &str, needle: &str) -> usize {
    line.rfind(needle).expect("present") + 1
}

#[test]
fn an_in_length_list_validates_clean_through_both_stages() {
    for t in &templates() {
        let (text, _) = t.text(&format!(
            "{} = {}, {} = {}",
            t.len,
            t.n,
            t.base,
            t.list(t.n)
        ));
        assert_eq!(placed(&text), Placed::new(), "{}", t.class);
    }
}

#[test]
fn a_list_longer_than_the_length_is_refused_at_its_extra_value() {
    for t in &templates() {
        let (text, block) = t.text(&format!(
            "{} = {}, {} = {}",
            t.len,
            t.n,
            t.base,
            t.list(t.n + 1)
        ));
        // The extra value is the list's last, just before `])`.
        let column = column_of_last(&block, &format!("{}])", t.value));
        assert_eq!(
            placed(&text),
            vec![finding(
                "parameter-out-of-range",
                &format!("b.{}_{}", t.base, t.n + 1),
                column
            )],
            "{}",
            t.class
        );
    }
}

#[test]
fn a_lone_element_past_the_length_is_refused_at_its_name() {
    for t in &templates() {
        let element = format!("{}_{}", t.base, t.n + 1);
        let (text, block) = t.text(&format!("{} = {}, {element} = {}", t.len, t.n, t.value));
        assert_eq!(
            placed(&text),
            vec![finding(
                "parameter-out-of-range",
                &format!("b.{element}"),
                column_of_last(&block, &element)
            )],
            "{}",
            t.class
        );
    }
}

#[test]
fn a_length_bound_to_a_sequence_parameter_is_the_parameters_value() {
    for t in &templates() {
        let (text, block) = t.text(&format!("{} = n, {} = {}", t.len, t.base, t.list(t.n + 1)));
        assert_eq!(
            placed(&text),
            vec![finding(
                "parameter-out-of-range",
                &format!("b.{}_{}", t.base, t.n + 1),
                column_of_last(&block, &format!("{}])", t.value))
            )],
            "{}",
            t.class
        );
    }
}

#[test]
fn index_zero_is_no_element_of_the_class_at_stage_one() {
    // Stage 1's own name check reads `k_0` as no parameter of the class, before the load gate's
    // rule, which refuses a hand-written `k_0` as past the length (oce-api `array_extent`).
    for t in &templates() {
        let element = format!("{}_0", t.base);
        let (text, block) = t.text(&format!("{} = {}, {element} = {}", t.len, t.n, t.value));
        assert_eq!(
            placed(&text),
            vec![finding(
                "unknown-parameter",
                &format!("b.{element}"),
                column_of_last(&block, &element)
            )],
            "{}",
            t.class
        );
    }
}

#[test]
fn an_omitted_length_takes_the_class_default() {
    // `MultiSum.nin` defaults to 0, so every gain is past it; `ExtractSignal.nout` to 1; and
    // `VectorFilter.nin` has no default, so its absence is the finding and no member is judged.
    for t in &templates() {
        let (text, block) = t.text(&format!("{} = {}", t.base, t.list(2)));
        let first = block.find('[').expect("a list") + 2;
        let second = column_of_last(&block, &format!("{}])", t.value));
        let expected = match t.base {
            "k" => vec![
                finding("parameter-out-of-range", "b.k_1", first),
                finding("parameter-out-of-range", "b.k_2", second),
            ],
            "extract" => vec![finding("parameter-out-of-range", "b.extract_2", second)],
            _ => vec![finding(
                "missing-required-parameter",
                "b",
                column_of_last(&block, "b = "),
            )],
        };
        assert_eq!(placed(&text), expected, "{}", t.class);
    }
}
