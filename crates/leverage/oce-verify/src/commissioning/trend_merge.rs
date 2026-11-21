//! The input sources merged on the union of their times (decision 15; issue #660).
//!
//! **The rule.** The frames are the instants of the union of the sources' times, in increasing
//! order. At an instant `t`, a source with `k` rows at `t` fills the first `k` frames at `t`,
//! its `i`-th row the `i`-th frame, and there are as many frames at `t` as the source with the
//! most rows there has. A source with fewer rows at `t`, or none, is missing at the frames it
//! does not fill: NaN in a Real column, a cleared validity flag in an Integer or Boolean one.
//! Replay then holds it, under its source's `hold.maxAge`, from its last observed sample, which
//! at a second frame at `t` is its own row at `t` (the issue's recommended reading, pinned by
//! test). So two files with one row each at `t` give one frame, and a file with two rows at `t`
//! beside a file with one give two.
//!
//! **Independent of order.** The frames and the cells depend on each source's times and values
//! only: the union, the count per instant (a maximum) and the row a source puts in a frame do
//! not depend on the order sources are declared or files handed in, and the columns are sorted
//! by name afterwards.
//!
//! **Bounds and cost.** A first pass counts the frames, refusing past `max_rows` as soon as the
//! count passes it, and the bytes the merged columns would hold are checked against
//! `max_source_bytes` before anything is allocated. Both passes visit each row of each source
//! once through a heap of one cursor per source: `O(R log S)` for `R` rows over `S` sources,
//! and no memory beyond the heap and the result. The frames are at most the rows of all
//! sources together, never a product of sources.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use oce_replay::{Column, ReplayLimits};

use super::source_error::{SourceError, SourceErrorKind as Kind};
use super::trend_read::Values;

/// One input source to merge: its times and its converted columns.
pub(crate) struct Merging<'a> {
    pub(crate) times: &'a [f64],
    pub(crate) columns: &'a [Values],
}

/// A time ordered totally: every time here is finite, and `-0.0` was read as `0.0`.
#[derive(Clone, Copy, PartialEq)]
struct Instant(f64);

impl Eq for Instant {}

impl PartialOrd for Instant {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Instant {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Visit the instants of the union in increasing order: for each, its time, the index of its
/// first frame, its frame count and each source's run there as `(source, first row, rows)`.
fn walk(
    sources: &[Merging<'_>],
    mut visit: impl FnMut(f64, u64, u64, &[(usize, usize, usize)]) -> Result<(), SourceError>,
) -> Result<u64, SourceError> {
    let mut heap: BinaryHeap<Reverse<(Instant, usize)>> = sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.times.first().map(|&t| Reverse((Instant(t), index))))
        .collect();
    let mut next = vec![0_usize; sources.len()];
    let mut runs: Vec<(usize, usize, usize)> = Vec::with_capacity(sources.len());
    let mut frames: u64 = 0;
    while let Some(Reverse((Instant(t), _))) = heap.peek().copied() {
        runs.clear();
        while let Some(&Reverse((Instant(at), index))) = heap.peek() {
            if at != t {
                break;
            }
            heap.pop();
            let times = sources[index].times;
            let start = next[index];
            let end = start + times[start..].iter().take_while(|&&x| x == t).count();
            next[index] = end;
            runs.push((index, start, end - start));
            if let Some(&after) = times.get(end) {
                heap.push(Reverse((Instant(after), index)));
            }
        }
        let count = runs.iter().map(|run| run.2).max().unwrap_or(0) as u64;
        visit(t, frames, count, &runs)?;
        frames += count;
    }
    Ok(frames)
}

/// The merged frame times and, per source, per column, the merged column.
///
/// # Errors
/// `too-many-rows` past `max_rows` frames; `too-many-columns` past `max_columns` columns, the
/// time column included; `too-many-bytes` when the merged columns would hold more than
/// `max_source_bytes`.
pub(crate) fn merge(
    sources: &[Merging<'_>],
    limits: &ReplayLimits,
) -> Result<(Vec<f64>, Vec<Vec<Column>>), SourceError> {
    let width: usize = sources.iter().map(|source| source.columns.len()).sum();
    if width as u64 + 1 > u64::from(limits.max_columns) {
        return Err(SourceError::new(
            Kind::TooManyColumns,
            format!(
                "the input sources map {width} columns; with the time at most {} are merged",
                limits.max_columns
            ),
        ));
    }
    let max_rows = limits.max_rows;
    let frames = walk(sources, |_, first, count, _| {
        if first + count > max_rows {
            Err(SourceError::new(
                Kind::TooManyRows,
                format!("the input sources merge into more than {max_rows} frames"),
            ))
        } else {
            Ok(())
        }
    })?;
    let per_frame: u64 = 8 + sources
        .iter()
        .flat_map(|source| source.columns)
        .map(|values| match values {
            Values::Real(_) => 8,
            Values::Integer(_) => 9,
            Values::Boolean(_) => 2,
        })
        .sum::<u64>();
    if u128::from(frames) * u128::from(per_frame) > u128::from(limits.max_source_bytes) {
        return Err(SourceError::new(
            Kind::TooManyBytes,
            format!(
                "{frames} merged frames of {per_frame} bytes each are more than {} bytes",
                limits.max_source_bytes
            ),
        ));
    }
    // Within `max_rows`, which is far below `usize::MAX` on every target with the memory.
    let length = usize::try_from(frames).unwrap_or(usize::MAX);
    let mut times = vec![0.0; length];
    let mut merged: Vec<Vec<Column>> = sources
        .iter()
        .map(|source| {
            source
                .columns
                .iter()
                .map(|values| match values {
                    Values::Real(_) => Column::Real(vec![f64::NAN; length]),
                    Values::Integer(_) => Column::Integer {
                        values: vec![0; length],
                        valid: Some(vec![false; length]),
                    },
                    Values::Boolean(_) => Column::Boolean {
                        values: vec![false; length],
                        valid: Some(vec![false; length]),
                    },
                })
                .collect()
        })
        .collect();
    walk(sources, |t, first, count, runs| {
        let first = first as usize;
        times[first..first + count as usize].fill(t);
        for &(index, start, rows) in runs {
            for (from, into) in sources[index].columns.iter().zip(&mut merged[index]) {
                place(from, start, rows, into, first);
            }
        }
        Ok(())
    })?;
    Ok((times, merged))
}

/// Copy rows `start..start + rows` of `from` into the frames from `first` of `into`.
fn place(from: &Values, start: usize, rows: usize, into: &mut Column, first: usize) {
    for offset in 0..rows {
        let frame = first + offset;
        match (from, &mut *into) {
            (Values::Real(values), Column::Real(column)) => column[frame] = values[start + offset],
            (
                Values::Integer(values),
                Column::Integer {
                    values: column,
                    valid: Some(valid),
                },
            ) => {
                if let Some(value) = values[start + offset] {
                    column[frame] = i64::from(value);
                    valid[frame] = true;
                }
            }
            (
                Values::Boolean(values),
                Column::Boolean {
                    values: column,
                    valid: Some(valid),
                },
            ) => {
                if let Some(value) = values[start + offset] {
                    column[frame] = value;
                    valid[frame] = true;
                }
            }
            // Each merged column is built above from the kind of its source column.
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "trend_merge_tests.rs"]
mod tests;
