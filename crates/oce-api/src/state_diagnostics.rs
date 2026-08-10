//! Bounded diagnostic rendering for state compatibility refusals.

use std::fmt::{self, Write};

use crate::state::{BlockKey, EngineStateError};

const FIELD_LIMIT: usize = 256;

pub(crate) fn incompatible(
    subject: &str,
    snapshot: &impl fmt::Debug,
    target: &impl fmt::Debug,
) -> EngineStateError {
    EngineStateError::IncompatibleExecution {
        subject: bounded_text(subject),
        snapshot: bounded_debug(snapshot),
        target: bounded_debug(target),
    }
}

pub(crate) fn incompatible_text(subject: &str, snapshot: &str, target: &str) -> EngineStateError {
    EngineStateError::IncompatibleExecution {
        subject: bounded_text(subject),
        snapshot: bounded_text(snapshot),
        target: bounded_text(target),
    }
}

pub(crate) fn bounded_text(value: &str) -> String {
    let mut output = BoundedText::new();
    let _ = output.write_str(value);
    output.finish()
}

pub(crate) fn bounded_format(arguments: fmt::Arguments<'_>) -> String {
    let mut output = BoundedText::new();
    let _ = output.write_fmt(arguments);
    output.finish()
}

pub(crate) fn bounded_block_subject(key: &BlockKey) -> String {
    match key {
        BlockKey::Authored(iri) => bounded_text(iri),
        BlockKey::PassThrough {
            input_path,
            output_path,
        } => bounded_format(format_args!("{input_path} -> {output_path}")),
        BlockKey::Dense(id) => format!("block#{id}"),
    }
}

fn bounded_debug(value: &impl fmt::Debug) -> String {
    let mut output = BoundedText::new();
    let _ = write!(&mut output, "{value:?}");
    output.finish()
}

struct BoundedText {
    text: String,
    truncated: bool,
}

impl BoundedText {
    fn new() -> Self {
        Self {
            text: String::with_capacity(FIELD_LIMIT + 3),
            truncated: false,
        }
    }

    fn finish(mut self) -> String {
        if self.truncated {
            self.text.push_str("...");
        }
        self.text
    }
}

impl Write for BoundedText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let remaining = FIELD_LIMIT.saturating_sub(self.text.len());
        if remaining == 0 {
            self.truncated = true;
            return Err(fmt::Error);
        }
        let mut end = remaining.min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&value[..end]);
        if end < value.len() {
            self.truncated = true;
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    struct CountedDebug<'a>(&'a Cell<usize>);

    impl fmt::Debug for CountedDebug<'_> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            for _ in 0..1_000_000 {
                self.0.set(self.0.get() + 1);
                formatter.write_str("x")?;
            }
            Ok(())
        }
    }

    #[test]
    fn bounded_debug_stops_formatting_after_the_limit() {
        let writes = Cell::new(0);
        let output = bounded_debug(&CountedDebug(&writes));
        assert_eq!(output.len(), FIELD_LIMIT + 3);
        assert!(writes.get() <= FIELD_LIMIT + 1, "{}", writes.get());
    }
}
