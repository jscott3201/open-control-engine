//! The codes `oce verify` passes through from `oce-verify`, each with a one-line meaning, in the
//! order of `oce_verify::VECTOR_ERROR_CODES` and `oce_verify::VERIFY_ERROR_CODES` (pinned by the
//! unit tests below). Every one is exit `2`: a failed expectation is never a code, it is a failed
//! scenario inside the receipt (vectors note, "Error codes").

/// The vector file's load codes and their meanings, in `oce_verify::VECTOR_ERROR_CODES` order.
pub(crate) const VECTOR_MEANINGS: &[(&str, &str)] = &[
    ("not-utf8", "the vector file is not UTF-8"),
    (
        "byte-order-mark",
        "the vector file starts with a byte-order mark",
    ),
    ("json-syntax", "the vector file is not one JSON document"),
    (
        "input-too-large",
        "the vector file is larger than 4 MiB; refused before it is read",
    ),
    (
        "nesting-too-deep",
        "objects and arrays nest deeper than 8 levels",
    ),
    (
        "list-too-long",
        "a list has more than 65,536 entries, a step list included",
    ),
    (
        "string-too-long",
        "a string is longer than 4,096 bytes as written",
    ),
    ("duplicate-key", "an object writes one key twice"),
    (
        "unknown-key",
        "an object carries a key the format does not define (within, when: not yet)",
    ),
    ("missing-key", "an object lacks a required key"),
    (
        "wrong-type",
        "a value has the wrong JSON type for its position",
    ),
    (
        "non-finite-number",
        "NaN or Infinity as a number, or a real that overflows binary64",
    ),
    (
        "integer-out-of-range",
        "an integer outside 64 bits, or a time outside the integers a binary64 holds",
    ),
    (
        "control-character",
        "a string carries a control character, raw or escaped",
    ),
    ("unknown-schema", "schema is not oce/vectors/v1"),
    (
        "invalid-clock",
        "step_s is not finite and positive, or horizon_s is not finite or is negative",
    ),
    (
        "horizon-off-cadence",
        "no whole number of steps reaches horizon_s",
    ),
    ("too-many-ticks", "the clock has more than 16,777,216 ticks"),
    ("too-many-scenarios", "more than 4,096 scenarios"),
    (
        "too-many-expectations",
        "a scenario has more than 4,096 expectations",
    ),
    ("too-many-mutants", "more than 256 mutants"),
    (
        "too-many-parameters",
        "a params map has more than 256 entries",
    ),
    ("duplicate-scenario", "two scenarios share a name"),
    (
        "empty-name",
        "an empty scenario, input, parameter, output or diagnostic block name",
    ),
    ("step-list-empty", "a step list has no step"),
    (
        "step-list-not-from-zero",
        "a step list does not start at t = 0",
    ),
    (
        "step-times-not-increasing",
        "a step's t is not after the step before it",
    ),
    ("step-after-horizon", "a step's t is after horizon_s"),
    ("invalid-window", "a window's from_s is after its to_s"),
    (
        "window-outside-clock",
        "a window starts before 0 or ends after horizon_s",
    ),
    ("window-uncovered", "a window holds no tick of the clock"),
    (
        "ambiguous-expectation",
        "an expectation is not exactly one of the window and instant forms",
    ),
    (
        "instant-off-cadence",
        "a diagnostic at_s is not a tick of the clock",
    ),
    (
        "mutant-path-invalid",
        "a mutant path is absolute, has an empty, . or .. segment, or holds a backslash",
    ),
    ("duplicate-mutant", "two mutants share a path"),
];

/// The bind and run codes and their meanings, in `oce_verify::VERIFY_ERROR_CODES` order.
pub(crate) const VERIFY_MEANINGS: &[(&str, &str)] = &[
    (
        "subject-load-failed",
        "the engine refused the subject, or a text or wire subject has an error finding",
    ),
    ("unknown-input", "an input name matches no boundary input"),
    (
        "ambiguous-input",
        "an input suffix matches several boundary inputs (candidates)",
    ),
    (
        "duplicate-input",
        "two input names of one scenario resolve to one boundary input",
    ),
    (
        "input-not-supplied",
        "a scenario supplies no value for a boundary input",
    ),
    (
        "unknown-output",
        "an output name matches no boundary output",
    ),
    (
        "ambiguous-output",
        "an output suffix matches several boundary outputs (candidates)",
    ),
    ("param-path-unknown", "a parameter key matches no parameter"),
    (
        "param-path-ambiguous",
        "a parameter suffix matches several parameters (candidates)",
    ),
    (
        "param-path-duplicate",
        "two keys of one params map name one parameter",
    ),
    (
        "param-refused",
        "the engine refuses an edit in every pass (cause: param-range, param-type, param-structural)",
    ),
    (
        "value-type-mismatch",
        "a value its port's or parameter's type does not take",
    ),
    (
        "value-not-representable",
        "a number no binary64 holds exactly where a Real is read",
    ),
    (
        "enum-port-unsupported",
        "the subject has an Enum boundary input or output",
    ),
    (
        "unsupported-port-type",
        "the subject has a String boundary input or output",
    ),
    (
        "too-many-frames",
        "the run would execute more than 16,777,216 frames, or one scenario more than replay allows",
    ),
    (
        "too-many-instants",
        "the run would compare more than 268,435,456 instants",
    ),
    (
        "too-many-load-bytes",
        "the run would load more than 1 GiB of subject and mutant bytes",
    ),
    (
        "too-many-frame-bytes",
        "the run's frames weighted by document bytes would pass 2^40",
    ),
    ("mutant-missing", "a listed mutant was given no bytes"),
    (
        "mutant-unlisted",
        "mutant bytes were given for a path the set does not list",
    ),
    (
        "duplicate-mutant",
        "one mutant given twice, or two listings that resolve to one file",
    ),
    (
        "too-many-mutant-bytes",
        "the mutants are larger than 64 MiB together; refused before any is read",
    ),
    ("mutant-load-failed", "the engine refused a mutant"),
    (
        "mutant-boundary-differs",
        "a mutant's input or output paths or types are not the subject's (path)",
    ),
    ("mutant-identical", "a mutant has the subject's content id"),
    (
        "mutant-param-refused",
        "a mutant refuses an edit the subject admits (cause)",
    ),
    (
        "run-refused",
        "the engine refused a frame for a reason about the run, not the frame",
    ),
    (
        "replay-failed",
        "the replay stopped with an error (cause: trace-too-large, engine-setup, ...)",
    ),
    (
        "window-uncovered",
        "a window held no executed tick after a run with no refused frame",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(table: &[(&'static str, &str)]) -> Vec<&'static str> {
        table.iter().map(|(code, _)| *code).collect()
    }

    #[test]
    fn the_tables_are_oce_verifys_code_lists_in_order() {
        assert_eq!(codes(VECTOR_MEANINGS), oce_verify::VECTOR_ERROR_CODES);
        assert_eq!(codes(VERIFY_MEANINGS), oce_verify::VERIFY_ERROR_CODES);
    }

    #[test]
    fn every_meaning_is_one_nonempty_line() {
        for (code, meaning) in VECTOR_MEANINGS.iter().chain(VERIFY_MEANINGS) {
            assert!(!meaning.is_empty() && !meaning.contains('\n'), "{code}");
        }
    }
}
