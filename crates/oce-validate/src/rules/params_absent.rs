//! The elements of a parameter-declared shape that a block does not bind, counted and named
//! without walking the shape (issue #726).
//!
//! A flattened table or vector has a shape that one parameter can set: a `TimeTable` takes its
//! shape from the largest `table_<row>_<col>` it carries, an `ExtractSignal` its length from
//! `nout`. Up to 2^20 elements ([`oce_blocks::MAX_RESOLVED_PORT_WIDTH`]) cost the document a few
//! bytes. A rule that judged each absent element raised up to 2^20 findings per block, and walked
//! the shape to find them: 26 KB of CXF held 9.5 GB for 198 s before it was refused. So a rule
//! raises one finding for the absent elements of a shape, with their count and the first
//! [`NAMED_ABSENT`] of them, and finds them from the elements the block binds: [`absent_in`] works
//! in the number of bound elements, whatever the shape.

/// The most absent elements one finding names; the rest are counted.
pub(super) const NAMED_ABSENT: usize = 4;

/// The positions of an inclusive range that a block does not bind.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Absent {
    /// How many positions of the range are absent.
    pub(super) count: usize,
    /// The first [`NAMED_ABSENT`] absent positions, ascending.
    pub(super) first: Vec<usize>,
}

impl Absent {
    /// Count `other`'s positions with these, and name the first [`NAMED_ABSENT`] of both. `other`
    /// must be of a range wholly after this one's, so the named positions stay ascending.
    pub(super) fn then(mut self, other: Absent) -> Absent {
        self.count += other.count;
        let room = NAMED_ABSENT.saturating_sub(self.first.len());
        self.first.extend(other.first.into_iter().take(room));
        self
    }

    /// `names` of the first absent positions, `, ` between them, then ` and N more` when the
    /// count names fewer than it counts.
    pub(super) fn named(&self, name: impl Fn(usize) -> String) -> String {
        let names = self
            .first
            .iter()
            .map(|&position| name(position))
            .collect::<Vec<_>>()
            .join(", ");
        match self.count.saturating_sub(self.first.len()) {
            0 => names,
            more => format!("{names} and {more} more"),
        }
    }
}

/// The positions of `low..=high` that `present` lacks: their count and the first
/// [`NAMED_ABSENT`] of them, ascending.
///
/// `present` yields bound positions in strictly ascending order (a `BTreeMap`'s keys); a position
/// outside the range, or one not above the one before it, is skipped. Work is one step per
/// position `present` yields plus at most [`NAMED_ABSENT`], never the range's length. An empty
/// range (`low > high`) has nothing absent. Total and panic-free for every `usize` input.
pub(super) fn absent_in(
    present: impl IntoIterator<Item = usize>,
    low: usize,
    high: usize,
) -> Absent {
    if low > high {
        return Absent::default();
    }
    let mut first = Vec::new();
    let mut bound = 0_usize;
    // The lowest position not yet known to be bound or absent; `None` once past `usize::MAX`.
    let mut next = Some(low);
    for position in present {
        let Some(from) = next else {
            break;
        };
        if position < from || position > high {
            continue;
        }
        if position > from {
            name_absent(&mut first, from, position - 1);
        }
        bound += 1;
        next = position.checked_add(1);
    }
    if let Some(from) = next {
        name_absent(&mut first, from, high);
    }
    // `bound` counts distinct positions of the range, so it is at most the range's length,
    // `high - low + 1`, which only the whole of `0..=usize::MAX` cannot state.
    let count = match bound.checked_sub(1) {
        Some(bound_less_one) => (high - low) - bound_less_one,
        None => (high - low).saturating_add(1),
    };
    Absent { count, first }
}

/// Push the positions `from..=to` onto `first` while it names fewer than [`NAMED_ABSENT`].
fn name_absent(first: &mut Vec<usize>, from: usize, to: usize) {
    let mut position = from;
    while first.len() < NAMED_ABSENT && position <= to {
        first.push(position);
        let Some(after) = position.checked_add(1) else {
            break;
        };
        position = after;
    }
}

#[cfg(test)]
#[path = "params_absent_tests.rs"]
mod tests;
