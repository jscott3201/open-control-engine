//! The bounds a commissioning spec and a point mapping are read under (commissioning note,
//! "Test-spec file", as landed).
//!
//! The two readers of this crate have one profile. Nesting ([`crate::MAX_DEPTH`]), list length
//! ([`crate::MAX_LIST_LEN`]) and string length ([`crate::MAX_STRING_BYTES`]) are the strict
//! cursor's, fixed, and shared with the vector reader. The rest are [`SpecLimits`]: each field is
//! a default and a documented maximum at once. A caller may lower any of them, to zero included;
//! a value above the maximum is read as the maximum, so no caller can raise one. Every bound is
//! inclusive: a document *at* a bound loads, one unit past it is refused with its own code,
//! never truncated.

/// The bounds on one commissioning spec or point mapping that the strict profile does not fix.
///
/// `Default` is the documented maxima ([`SpecLimits::MAX`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecLimits {
    /// Bytes in the document, comments included. Maximum 4 MiB; past it the refusal is
    /// `input-too-large`, decided from the length before a byte is examined. It is also the
    /// bound on a load's memory: a load holds at most 32 bytes per byte of the document (the
    /// worst case is a long list of one-digit numbers inside a field the verifier ignores and
    /// keeps), so about 134 MB at the maximum, and less in proportion when this is lowered.
    pub max_bytes: u64,
    /// Entries in one reference's `parameters` map. Maximum 256; past it the refusal is
    /// `too-many-parameters`. It bounds the passes that apply the edits (commissioning note,
    /// decision 29).
    pub max_parameters: u32,
    /// Keys in one `outputs`, `indicators` or `columns` map. Maximum 4,096, the most outputs a
    /// run compares and the most columns a replay source has; past it the refusal is
    /// `too-many-keys`.
    pub max_keys: u32,
    /// Work units one selection of a reference's `outputs` or `indicators` keys against a
    /// sequence's outputs may spend, when a run binds the reference: one unit for every key and
    /// output compared, and one per step of the glob matcher, so the count depends on the keys
    /// and the outputs alone, never on a clock. Maximum 2^28 = 268,435,456; past it the refusal
    /// is `work-limit-exceeded`, and a selection whose keys times outputs alone pass it is
    /// refused before anything is allocated. A selection holds one bit per key and output, so
    /// this bound is also its memory: at most an eighth of it in bytes.
    ///
    /// Two costs of a selection are outside this count, because they grow with the sequence's
    /// outputs alone, which come from the engine's own frame and not from the spec: the index
    /// an exact key is resolved through (one sort of the output paths, `n log n` comparisons of
    /// path bytes), and the paths an ambiguous exact name collects for its refusal. For scale,
    /// a million outputs sharing one last segment, and one exact key naming it, are refused as
    /// `ambiguous-output` after 7.4 s in a debug build.
    pub max_key_match_work: u64,
}

impl SpecLimits {
    /// The documented maxima, which are also the defaults.
    pub const MAX: Self = Self {
        max_bytes: 4 * 1024 * 1024,
        max_parameters: 256,
        max_keys: 4_096,
        max_key_match_work: 1 << 28,
    };

    /// These limits with every field above its documented maximum lowered to that maximum: the
    /// limits a load actually applies.
    #[must_use]
    pub fn effective(&self) -> Self {
        let max = Self::MAX;
        Self {
            max_bytes: self.max_bytes.min(max.max_bytes),
            max_parameters: self.max_parameters.min(max.max_parameters),
            max_keys: self.max_keys.min(max.max_keys),
            max_key_match_work: self.max_key_match_work.min(max.max_key_match_work),
        }
    }
}

impl Default for SpecLimits {
    fn default() -> Self {
        Self::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_documented_maxima() {
        let limits = SpecLimits::default();
        assert_eq!(limits, SpecLimits::MAX);
        assert_eq!(limits.max_bytes, 4_194_304);
        assert_eq!(limits.max_parameters, 256);
        assert_eq!(limits.max_keys, 4_096);
        assert_eq!(limits.max_key_match_work, 268_435_456);
        // The byte bound is the vector reader's: the two readers of this crate have one profile.
        assert_eq!(limits.max_bytes, crate::VerifyLimits::MAX.max_bytes);
        assert_eq!(
            limits.max_parameters,
            crate::VerifyLimits::MAX.max_parameters
        );
    }

    #[test]
    fn a_limit_can_be_lowered_and_never_raised() {
        let lowered = SpecLimits {
            max_bytes: 10,
            max_parameters: 0,
            max_keys: 3,
            max_key_match_work: 1,
        };
        assert_eq!(lowered.effective(), lowered);
        let raised = SpecLimits {
            max_bytes: u64::MAX,
            max_parameters: u32::MAX,
            max_keys: u32::MAX,
            max_key_match_work: u64::MAX,
        };
        assert_eq!(raised.effective(), SpecLimits::MAX);
        // One past each maximum is still the maximum, field by field.
        let max = SpecLimits::MAX;
        for one_past in [
            SpecLimits {
                max_bytes: max.max_bytes + 1,
                ..max
            },
            SpecLimits {
                max_parameters: max.max_parameters + 1,
                ..max
            },
            SpecLimits {
                max_keys: max.max_keys + 1,
                ..max
            },
            SpecLimits {
                max_key_match_work: max.max_key_match_work + 1,
                ..max
            },
        ] {
            assert_ne!(one_past, max);
            assert_eq!(one_past.effective(), max);
        }
        // Lowering one field leaves the others alone.
        let one_lowered = SpecLimits {
            max_keys: 7,
            ..raised
        };
        assert_eq!(one_lowered.effective(), SpecLimits { max_keys: 7, ..max });
    }
}
