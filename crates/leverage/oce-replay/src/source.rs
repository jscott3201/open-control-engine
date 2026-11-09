//! The frame source shape (decisions 4 and 13): rows by slot, `None` for a missing sample.

use oce_api::Value;

use crate::{MapId, SourceError, SourceKind, TimeOrigin};

/// One lent row: its time in model seconds and one cell per input slot, `None` for a missing
/// sample.
pub type Row<'a> = (f64, &'a [Option<Value>]);

/// Yields one row by slot; `None` is a missing sample. Not `Iterator`: it lends its buffer, so a
/// row is valid until the next call.
///
/// A row's cells are in the slot order of the [`crate::InputMap`] the source was bound to (one per
/// input), and its time is model seconds. Replay itself refuses a missing (NaN) or infinite time,
/// a time regression and, under `DuplicateTime::Refuse`, an equal time, so a source only converts.
/// The three adapters in this crate implement it; a custom source (a binding's iterator slow path)
/// may too.
pub trait FrameSource {
    /// The identity of the map this source was bound to.
    fn map_id(&self) -> &MapId;

    /// Whether the cells are samples or signals. Samples by default.
    fn kind(&self) -> SourceKind {
        SourceKind::Samples
    }

    /// The source's wall-clock origin, if it converted wall-clock instants. `None` by default.
    fn origin(&self) -> Option<&TimeOrigin> {
        None
    }

    /// Columns the source carries that neither the map nor the time column uses. Empty by
    /// default.
    fn ignored_columns(&self) -> &[String] {
        &[]
    }

    /// The next row, or `None` at the end.
    ///
    /// # Errors
    /// A [`SourceError`] for a row the source cannot produce; replay ends the run with it.
    fn next_row(&mut self) -> Result<Option<Row<'_>>, SourceError>;
}

/// A borrowed source is a source, so a run can drive one it does not own.
impl<S: FrameSource + ?Sized> FrameSource for &mut S {
    fn map_id(&self) -> &MapId {
        (**self).map_id()
    }
    fn kind(&self) -> SourceKind {
        (**self).kind()
    }
    fn origin(&self) -> Option<&TimeOrigin> {
        (**self).origin()
    }
    fn ignored_columns(&self) -> &[String] {
        (**self).ignored_columns()
    }
    fn next_row(&mut self) -> Result<Option<Row<'_>>, SourceError> {
        (**self).next_row()
    }
}

/// A boxed source is a source, so a binding can hold `Replay<Box<dyn FrameSource>>`.
impl<S: FrameSource + ?Sized> FrameSource for Box<S> {
    fn map_id(&self) -> &MapId {
        (**self).map_id()
    }
    fn kind(&self) -> SourceKind {
        (**self).kind()
    }
    fn origin(&self) -> Option<&TimeOrigin> {
        (**self).origin()
    }
    fn ignored_columns(&self) -> &[String] {
        (**self).ignored_columns()
    }
    fn next_row(&mut self) -> Result<Option<Row<'_>>, SourceError> {
        (**self).next_row()
    }
}
