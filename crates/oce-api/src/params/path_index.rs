//! The dotted parameter paths of [`ParamTable`](super::ParamTable), held in parts and found by
//! their hash (issue #807).
//!
//! A parameter's path is its block's instance IRI, `.` and its name, or `b<id>`, `.` and its name
//! for a hand-built block with no IRI. The table used to spell every path and keep it twice, as
//! the row's path and as the index's key: three times the IRI's length a parameter once the
//! string had grown, so a block under a long IRI held its IRI once per parameter in each copy.
//! Each row now holds its block's IRI shared (`Arc<str>`) and its name, and the index holds the
//! hash of each spelled path beside the row's position, sorted. A lookup hashes the path it is
//! given, takes the rows with that hash, and compares each one's parts with the path, so a path
//! is spelled only when [`ParamTable::iter`](super::ParamTable::iter) hands it out.
//!
//! The answers are the ones a map from the spelled path to the first row would give: the rows
//! with one hash are kept in row order, and the first whose parts spell the path is the one
//! found, so two rows whose hashes collide are told apart by their parts. The hash is the
//! crate's [`StableHash`] fed the path's bytes one at a time (FNV-1a over 128 bits, defined byte
//! by byte), so a row hashed as its base, `.` and name and a lookup hashed as one string are the
//! same fold over the same bytes whatever the toolchain, and the index is the same for one model
//! on every load and every build. It decides only which rows are compared, never which one is
//! found, and nothing iterates it or prints it: `iter` walks the rows in their order, and
//! `ParamTable`'s `Debug` shows the rows alone.
//!
//! The hash is unkeyed, so a document can craft paths that collide. A lookup then compares its
//! path with every colliding row: linear in those rows, which the document bounds by the paths
//! it spells. A load does no lookup; only a host that calls `get_param` or `set_param` once for
//! each of the colliding paths sees a quadratic total. Ordering the rows of one hash by their
//! path bytes, so a lookup searches them instead of scanning, is issue #813.

use std::sync::Arc;

use oce_model::BlockId;

use crate::stable_hash::StableHash;

/// A hash of a path's bytes, fed in pieces whose concatenation is the path. Each implementation
/// is a fold over the bytes alone, so how the path is cut into pieces never changes the result.
pub(super) trait PathHash: Default {
    /// Fold `bytes` in, one byte at a time.
    fn write(&mut self, bytes: &[u8]);
    /// The hash of the bytes folded so far.
    fn finish(self) -> u128;
}

/// The index's hash: [`StableHash`], whose `write_bytes` folds one byte at a time.
pub(super) struct StablePathHash(StableHash);

impl Default for StablePathHash {
    fn default() -> Self {
        Self(StableHash::new())
    }
}

impl PathHash for StablePathHash {
    fn write(&mut self, bytes: &[u8]) {
        self.0.write_bytes(bytes);
    }

    fn finish(self) -> u128 {
        self.0.finish()
    }
}

/// What a parameter's path starts with: its block's instance IRI, shared with the model, or the
/// synthetic `b<id>` of a hand-built block that has none.
#[derive(Clone, Debug)]
pub(super) enum PathBase {
    /// The block's instance IRI.
    Iri(Arc<str>),
    /// No IRI: the path starts `b<id>`, the block's id in decimal.
    Unnamed(BlockId),
}

impl PathBase {
    /// The base of a parameter of a block whose instance IRI is `iri`.
    pub(super) fn of(iri: Option<&Arc<str>>, block: BlockId) -> Self {
        iri.map_or(Self::Unnamed(block), |iri| Self::Iri(Arc::clone(iri)))
    }

    /// Feed the path's bytes, base, `.` and `name`, to `hasher`, as one string would be fed.
    fn hash_path(&self, name: &str, hasher: &mut impl PathHash) {
        match self {
            Self::Iri(iri) => hasher.write(iri.as_bytes()),
            Self::Unnamed(block) => {
                hasher.write(b"b");
                hasher.write(block.0.to_string().as_bytes());
            }
        }
        hasher.write(b".");
        hasher.write(name.as_bytes());
    }

    /// Whether the path, base, `.` and `name`, is `path`.
    fn spells(&self, name: &str, path: &str) -> bool {
        let Some(rest) = (match self {
            Self::Iri(iri) => path.strip_prefix(&**iri),
            Self::Unnamed(block) => path
                .strip_prefix('b')
                .and_then(|rest| rest.strip_prefix(block.0.to_string().as_str())),
        }) else {
            return false;
        };
        rest.strip_prefix('.') == Some(name)
    }

    /// The path, base, `.` and `name`, spelled.
    pub(super) fn path(&self, name: &str) -> String {
        match self {
            Self::Iri(iri) => format!("{iri}.{name}"),
            Self::Unnamed(block) => format!("b{}.{name}", block.0),
        }
    }
}

/// The rows' positions sorted by the hash of their spelled paths, row order within one hash.
#[derive(Clone, Default)]
pub(super) struct PathIndex {
    by_hash: Vec<(u128, usize)>,
}

impl PathIndex {
    /// Index the rows `parts` yields, in row order, as `(base, name)`.
    pub(super) fn of<'a>(parts: impl Iterator<Item = (&'a PathBase, &'a str)>) -> Self {
        Self::hashed_by::<StablePathHash>(parts)
    }

    /// The first row whose path is `path`, `part(row)` giving each row's `(base, name)`.
    pub(super) fn find<'a>(
        &self,
        path: &str,
        part: impl Fn(usize) -> (&'a PathBase, &'a str),
    ) -> Option<usize> {
        self.find_hashed_by::<StablePathHash>(path, part)
    }

    /// [`Self::of`] with the hasher `H`, which tests replace with one whose hashes collide.
    fn hashed_by<'a, H: PathHash>(parts: impl Iterator<Item = (&'a PathBase, &'a str)>) -> Self {
        let mut by_hash: Vec<(u128, usize)> = parts
            .enumerate()
            .map(|(row, (base, name))| {
                let mut hasher = H::default();
                base.hash_path(name, &mut hasher);
                (hasher.finish(), row)
            })
            .collect();
        by_hash.sort_unstable();
        Self { by_hash }
    }

    /// [`Self::find`] with the hasher `H` the index was built with.
    fn find_hashed_by<'a, H: PathHash>(
        &self,
        path: &str,
        part: impl Fn(usize) -> (&'a PathBase, &'a str),
    ) -> Option<usize> {
        let mut hasher = H::default();
        hasher.write(path.as_bytes());
        let hash = hasher.finish();
        let start = self.by_hash.partition_point(|&(h, _)| h < hash);
        self.by_hash[start..]
            .iter()
            .take_while(|&&(h, _)| h == hash)
            .map(|&(_, row)| row)
            .find(|&row| {
                let (base, name) = part(row);
                base.spells(name, path)
            })
    }
}

#[cfg(test)]
#[path = "path_index_tests.rs"]
mod tests;
