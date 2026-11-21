//! Near misses of a vocabulary namespace: an IRI that is not in the namespace as written, and
//! would be under a spelling of it a document writes by mistake (issue #675; owner ruling of
//! 2026-11-20).
//!
//! The readers match a vocabulary by string: a key, a term or a typing token is in a namespace
//! when it starts with the namespace IRI, byte for byte. So an IRI one edit from the namespace is
//! in "another vocabulary" to every check, and whatever it carries is left unread without a word.
//! [`namespace_near_miss`] is the one decision of which spellings are that near: given an IRI and
//! a namespace, it says whether the IRI is a near miss of the namespace and, if so, where the
//! namespace as written ends and the local name begins. The caller holds the local name to its own
//! closed list and refuses with its own code; this module refuses nothing and knows no
//! vocabulary.
//!
//! An IRI is a near miss of `scheme://host/path` followed by one separator (`#` or `/`) when it
//! differs from the namespace only in these ways, alone or together:
//!
//! - **the scheme's letter case** (`HTTP://`; a scheme is case-insensitive, RFC 3986 §3.1);
//! - **`http` for `https` or the reverse**: another IRI, one character away;
//! - **the host's letter case** (`http://QUDT.org/`; RFC 3986 §3.2.2);
//! - **a default port**: an empty one, `:80` under `http` or `:443` under `https`, which RFC 3986
//!   §3.2.3 reads as no port;
//! - **the separator**: none, the other one, or more than one, in any mix (`…/qudthasUnit`,
//!   `…/qudt/hasUnit` for a `#` namespace, `…/qudt/#hasUnit`).
//!
//! Nothing else is one. The path is compared byte for byte and in its case (RFC 3986 leaves a path
//! case-sensitive), and another port is another authority. So is user information: everything
//! between `://` and the port is compared with the namespace's host, so `user@qudt.org` is not
//! `qudt.org`. A percent-encoded octet where the namespace has the character and a trailing dot on
//! the host are read as written, and so are no near miss. An IRI that is in the namespace exactly
//! is not a near miss either: the caller reads it.
//!
//! Work is a handful of comparisons over the IRI, linear in its length. Nothing is allocated,
//! nothing is dereferenced, and no input panics.

/// Where a near miss of a namespace splits: the namespace as the IRI writes it, and what follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamespaceNearMiss<'a> {
    /// The namespace as the IRI writes it: scheme, authority, path and whatever separators follow
    /// (`https://qudt.org/schema/qudt#`, `http://qudt.org:80/schema/qudt/#`,
    /// `http://qudt.org/schema/qudt` where there is none).
    pub written: &'a str,
    /// The rest of the IRI: the local name it would have in the namespace. It never starts with
    /// `#` or `/`, and it is empty when the IRI is a spelling of the namespace alone.
    pub local: &'a str,
}

/// Whether `iri` is a near miss of `namespace`, and where it splits if it is.
///
/// `namespace` is a namespace IRI the caller reads a vocabulary in, a constant of the form
/// `scheme://host/path` followed by one `#` or `/`, with no port (`http://qudt.org/schema/qudt#`,
/// `http://data.ashrae.org/S231#`). `iri` is an absolute IRI, already expanded against its
/// document's `@context`; nothing is expanded or trimmed here.
///
/// Returns `Some` when `iri` is the namespace under one or more of the near-miss spellings the
/// module documentation lists (scheme case, `http` against `https`, host case, a default port, the
/// separator), followed by any local name, the empty one included. Returns `None` when `iri` is in
/// `namespace` exactly as written (the scheme, host and path byte for byte, no port, and the one
/// separator), when it is anywhere else, and when `namespace` has no `://`, carries a port, or
/// does not end in `#` or `/`.
///
/// A vocabulary read in two namespaces that differ only in the separator
/// ([`crate::QUDT_SCHEMA_NAMESPACES`]) has each one a near miss of the other, so such a caller
/// asks whether the IRI is in either exactly before it asks this.
///
/// The two slices returned are `iri` cut in two, so `written` then `local` is `iri`. Work is
/// linear in the length of `iri`; it allocates nothing and never panics.
#[must_use]
pub fn namespace_near_miss<'a>(iri: &'a str, namespace: &str) -> Option<NamespaceNearMiss<'a>> {
    let read = Authority::of(namespace)?;
    // The separator is one ASCII byte, so the cut before it is on a character boundary.
    let separator = read.rest.chars().next_back().filter(|c| is_separator(*c))?;
    let stem = &read.rest[..read.rest.len() - separator.len_utf8()];
    if read.port.is_some() {
        return None;
    }
    let found = Authority::of(iri)?;
    if !same_scheme(found.scheme, read.scheme)
        || !found.host.eq_ignore_ascii_case(read.host)
        || !is_default_port(found.scheme, found.port)
    {
        return None;
    }
    let after_stem = found.rest.strip_prefix(stem)?;
    let local = after_stem.trim_start_matches(is_separator);
    let separators = &after_stem[..after_stem.len() - local.len()];
    let exact = found.scheme == read.scheme
        && found.host == read.host
        && found.port.is_none()
        && separators.chars().eq([separator]);
    if exact {
        return None;
    }
    Some(NamespaceNearMiss {
        written: &iri[..iri.len() - local.len()],
        local,
    })
}

/// An IRI of the form `scheme://authority…`, cut at its authority.
struct Authority<'a> {
    /// Everything before the first `://`.
    scheme: &'a str,
    /// The authority up to its first `:`, user information included where there is any.
    host: &'a str,
    /// What follows the host's `:`, when the authority has one.
    port: Option<&'a str>,
    /// Everything after the authority: the path, a query and a fragment.
    rest: &'a str,
}

impl<'a> Authority<'a> {
    /// `iri` cut at its authority, or `None` when it has no `://`. The authority ends at the
    /// first `/`, `?` or `#` (RFC 3986 §3.2). Nothing is validated: a part that is no scheme or no
    /// host simply equals no namespace's.
    fn of(iri: &'a str) -> Option<Self> {
        let (scheme, after) = iri.split_once("://")?;
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        let (authority, rest) = after.split_at(end);
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        Some(Self {
            scheme,
            host,
            port,
            rest,
        })
    }
}

/// Whether `c` ends a namespace IRI: `#` or `/`.
fn is_separator(c: char) -> bool {
    matches!(c, '#' | '/')
}

/// Whether `scheme` is `http` or `https`, in any letter case.
fn is_web_scheme(scheme: &str) -> bool {
    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
}

/// Whether a document could have meant `read` by `found`: the same scheme in any letter case, or
/// `http` for `https` or the reverse.
fn same_scheme(found: &str, read: &str) -> bool {
    found.eq_ignore_ascii_case(read) || (is_web_scheme(found) && is_web_scheme(read))
}

/// Whether `port` is no port under `scheme`: absent, empty, `80` under `http` or `443` under
/// `https` (RFC 3986 §3.2.3).
fn is_default_port(scheme: &str, port: Option<&str>) -> bool {
    match port {
        None | Some("") => true,
        Some("80") => scheme.eq_ignore_ascii_case("http"),
        Some("443") => scheme.eq_ignore_ascii_case("https"),
        Some(_) => false,
    }
}

#[cfg(test)]
#[path = "namespace_near_miss_tests.rs"]
mod tests;
