//! The one rule for a path a document names (vectors note decision 28; commissioning note,
//! "Test-spec file"): relative to the document, `/`-separated, and unable to leave its directory
//! or to name one file two ways.
//!
//! A vector set's mutant paths and every path of a commissioning spec are held to it. This crate
//! opens no file: the rule is what lets a caller join a path to the document's directory without
//! looking at it again. Each reader words its own refusal; the rule is decided here, once.

/// Why a path is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathDefect {
    /// The path holds a backslash: segments are separated by `/` only.
    Backslash,
    /// The path starts with `/`.
    Absolute,
    /// A segment is empty: the path is empty, ends in `/`, or holds `//`.
    EmptySegment,
    /// A segment is `.`.
    DotSegment,
    /// A segment is `..`.
    ParentSegment,
}

/// The first defect of `path`, or `None` for a path that stays inside its document's directory:
/// no backslash, not absolute, and no empty, `.` or `..` segment. A backslash is reported before
/// an absolute path, and that before a segment.
pub(crate) fn defect(path: &str) -> Option<PathDefect> {
    if path.contains('\\') {
        return Some(PathDefect::Backslash);
    }
    if path.starts_with('/') {
        return Some(PathDefect::Absolute);
    }
    path.split('/').find_map(|segment| match segment {
        "" => Some(PathDefect::EmptySegment),
        "." => Some(PathDefect::DotSegment),
        ".." => Some(PathDefect::ParentSegment),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_of_plain_segments_has_no_defect() {
        for path in [
            "a",
            "a/b.json",
            "data/Supply_Air_Temp.mos",
            "...",
            "a/..b/c..",
            ".hidden",
            "a b/c",
            "C:x",
        ] {
            assert_eq!(defect(path), None, "{path}");
        }
    }

    #[test]
    fn each_defect_is_named_and_the_first_one_wins() {
        for (path, expected) in [
            ("a\\b", PathDefect::Backslash),
            ("\\", PathDefect::Backslash),
            ("/a", PathDefect::Absolute),
            ("/", PathDefect::Absolute),
            ("", PathDefect::EmptySegment),
            ("a/", PathDefect::EmptySegment),
            ("a//b", PathDefect::EmptySegment),
            (".", PathDefect::DotSegment),
            ("a/./b", PathDefect::DotSegment),
            ("..", PathDefect::ParentSegment),
            ("a/../b", PathDefect::ParentSegment),
            ("a/b/..", PathDefect::ParentSegment),
            // A backslash wins over an absolute path, and that over a segment.
            ("/a\\..", PathDefect::Backslash),
            ("/../a", PathDefect::Absolute),
            // The first bad segment is the one reported.
            ("./..", PathDefect::DotSegment),
            ("../.", PathDefect::ParentSegment),
        ] {
            assert_eq!(defect(path), Some(expected), "{path:?}");
        }
    }
}
