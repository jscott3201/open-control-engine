//! Borrowed validation of the closed public-descriptor grammar; no private fingerprint authority.

use crate::{CompatibilityMismatch as M, ReplayError};

const LABELS: [&str; 10] = [
    "oce-compatibility:",
    "catalog-schema:",
    "catalog:",
    "io-schema:",
    "value-schema:",
    "parameter-schema:",
    "execution-profile:",
    "execution-profile-schema:",
    "oce-api-version:",
    "export:",
];

pub(crate) fn fields(text: &str) -> Result<[&str; 10], ReplayError> {
    if text.len() > 1024 || !text.ends_with('\n') {
        return Err(ReplayError::MalformedDescriptor);
    }
    let mut lines = text[..text.len() - 1].split('\n');
    let mut fields = [""; 10];
    for (index, label) in LABELS.iter().enumerate() {
        fields[index] = lines
            .next()
            .and_then(|line| line.strip_prefix(label))
            .ok_or(ReplayError::MalformedDescriptor)?;
    }
    if lines.next().is_some()
        || [0, 1, 3, 4, 5, 7].into_iter().any(|i| !revision(fields[i]))
        || !tag(fields[2], "catalog:1:fnv1a128:")
        || !fields[6].strip_prefix("HostTick-v").is_some_and(revision)
        || !version(fields[8])
        || !(fields[9] == "none" || tag(fields[9], "cxf:fnv1a128:"))
    {
        return Err(ReplayError::MalformedDescriptor);
    }
    Ok(fields)
}

pub(crate) fn compare(actual: &str, expected: &str) -> Result<(), ReplayError> {
    let a = fields(actual)?;
    let b = fields(expected)?;
    let causes = [
        M::DescriptorRevision,
        M::CatalogSchema,
        M::CatalogContent,
        M::IoSchema,
        M::ValueSchema,
        M::ParameterSchema,
        M::ExecutionProfile,
        M::ExecutionProfile,
        M::Build,
        if (a[9] == "none") != (b[9] == "none") {
            M::ExportPresence
        } else {
            M::ExportContent
        },
    ];
    for (index, cause) in causes.into_iter().enumerate() {
        if a[index] != b[index] || (index == 0 && a[index] != "1") {
            return Err(ReplayError::DescriptorMismatch(cause));
        }
    }
    Ok(())
}

fn revision(s: &str) -> bool {
    decimal(s) && !s.starts_with('0') && s.parse::<u32>().is_ok()
}

fn decimal(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s.len() == 1 || !s.starts_with('0'))
}

fn tag(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix).is_some_and(|s| {
        s.len() == 32
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn identifiers(s: &str, prerelease: bool) -> bool {
    s.split('.').all(|p| {
        !p.is_empty()
            && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !(prerelease && p.bytes().all(|b| b.is_ascii_digit()) && !decimal(p))
    })
}

fn version(s: &str) -> bool {
    let (s, build) = s.split_once('+').map_or((s, None), |(s, b)| (s, Some(b)));
    let (core, pre) = s.split_once('-').map_or((s, None), |(s, p)| (s, Some(p)));
    let mut numbers = core.split('.');
    (0..3).all(|_| {
        numbers
            .next()
            .is_some_and(|n| decimal(n) && n.parse::<u64>().is_ok())
    }) && numbers.next().is_none()
        && pre.is_none_or(|p| identifiers(p, true))
        && build.is_none_or(|p| identifiers(p, false))
}
