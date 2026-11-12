//! The SHA-256 digest behind a canonical content id and the id's text form, `oce:canon1:` and 64
//! lowercase hexadecimal digits (`docs/design/content-id.md`, "Hash and id format").

use std::fmt;
use std::str::FromStr;

use sha2::{Digest, Sha256};

/// The text every canonical content id starts with. `canon1` names the `OCECANON` encoding
/// version 1 and SHA-256 together. The tag stays `oce:canon1:` before 1.0.0: a change to either
/// re-stamps ids in place, and the engine version tells the layouts apart.
pub(crate) const PREFIX: &str = "oce:canon1:";

/// SHA-256 of `bytes`, in digest output order. A pure function: the `sha2` backend choice
/// (portable code or a CPU SHA extension) cannot change the output.
pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// One canonical content id: the SHA-256 of the `OCECANON` v1 encoding of a loaded flat graph,
/// written `oce:canon1:<64 lowercase hex digits>` (`docs/design/content-id.md`).
///
/// Minted only by [`crate::Engine::content_id`]. Re-serializing a document, permuting its arrays
/// or moving it to another instance namespace leaves the id unchanged; any change that reaches
/// execution or the boundary I/O definitions moves it. It is not [`crate::LoadReport::model_id`]
/// (the authored root `@id`), not an export-bytes tag, not a build identity and not a signature.
///
/// [`ContentId::parse`] is a checked parser of the exact text form, for hosts that stored an id
/// and compare it later; it never accepts upper case, whitespace, another prefix or a digest of
/// another length. There is no conversion from a string, a [`crate::CatalogContentId`] or a
/// [`crate::DomainKey`]:
///
/// ```compile_fail,E0308
/// use oce_api::{CatalogContentId, ContentId};
/// fn confuse_catalog_with_content(catalog: CatalogContentId) -> ContentId {
///     catalog
/// }
/// ```
///
/// ```compile_fail,E0308
/// let model_id = oce_api::DomainKey::new("authored-model");
/// let _: oce_api::ContentId = model_id;
/// ```
///
/// ```compile_fail,E0277
/// let _: oce_api::ContentId = String::from("oce:canon1:unverified").into();
/// ```
///
/// The text form is pinned:
///
/// ```
/// let text = "oce:canon1:fbbea1ef32cb24797011163d09ead7ee5c17757a4aad551b6989c745740a07a5";
/// let id = oce_api::ContentId::parse(text)?;
/// assert_eq!(id.to_string(), text);
/// assert_eq!(id.digest()[0], 0xfb);
/// assert!(oce_api::ContentId::parse(&text.to_uppercase()).is_err());
/// # Ok::<(), oce_api::ContentIdParseError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentId {
    text: String,
    digest: [u8; 32],
}

impl ContentId {
    /// The id of a 32-byte SHA-256 digest.
    pub(crate) fn from_digest(digest: [u8; 32]) -> Self {
        let mut text = String::with_capacity(PREFIX.len() + 64);
        text.push_str(PREFIX);
        for byte in digest {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self { text, digest }
    }

    /// Parse the exact text form `oce:canon1:` followed by 64 lowercase hexadecimal digits.
    ///
    /// # Errors
    /// [`ContentIdParseError`] for any other text: a different prefix (including `cxf:fnv1a128:`
    /// and `catalog:1:fnv1a128:`), a digest that is not exactly 64 digits, or a character outside
    /// `0-9a-f`. Never panics.
    pub fn parse(text: &str) -> Result<Self, ContentIdParseError> {
        let hex = text.strip_prefix(PREFIX).ok_or(ContentIdParseError)?;
        if hex.len() != 64 {
            return Err(ContentIdParseError);
        }
        let mut digest = [0u8; 32];
        for (slot, pair) in digest.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            *slot = (nibble(pair[0])? << 4) | nibble(pair[1])?;
        }
        Ok(Self::from_digest(digest))
    }

    /// Borrow the text form, `oce:canon1:<64 lowercase hex digits>`. No allocation or panic.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The 32 SHA-256 digest bytes in output order: the bytes a compiled-schedule blob carries
    /// in its header. No allocation or panic.
    #[must_use]
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl FromStr for ContentId {
    type Err = ContentIdParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// The text given to [`ContentId::parse`] is not `oce:canon1:` and 64 lowercase hex digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a canonical content id: expected `oce:canon1:` and 64 lowercase hex digits")]
pub struct ContentIdParseError;

const HEX: &[u8; 16] = b"0123456789abcdef";

fn nibble(byte: u8) -> Result<u8, ContentIdParseError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(ContentIdParseError),
    }
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;
