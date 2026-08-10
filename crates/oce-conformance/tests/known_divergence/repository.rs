//! Repository evidence-file checks layered over the pure register reader.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::reader::{Evidence, Register, ValidationCode, ValidationError};

pub(crate) fn validate_repository(
    register: &Register,
    repository_root: &Path,
) -> Result<(), ValidationError> {
    let canonical_root = fs::canonicalize(repository_root).map_err(|error| {
        ValidationError::new(
            ValidationCode::RepositoryRoot,
            None,
            format!("{}: {error}", repository_root.display()),
        )
    })?;
    if !canonical_root.is_dir() {
        return Err(ValidationError::new(
            ValidationCode::RepositoryRoot,
            None,
            format!("{} is not a directory", repository_root.display()),
        ));
    }
    for (index, entry) in register.entries.iter().enumerate() {
        canonical_regular_file(
            repository_root,
            &canonical_root,
            &entry.comparison_reference,
            index,
            ValidationCode::ComparisonReferenceMissing,
            ValidationCode::ComparisonReferenceNotFile,
            ValidationCode::ComparisonReferenceOutsideRepository,
        )?;
        for evidence in &entry.evidence {
            let path = canonical_regular_file(
                repository_root,
                &canonical_root,
                &evidence.path,
                index,
                ValidationCode::EvidenceMissing,
                ValidationCode::EvidenceNotFile,
                ValidationCode::EvidenceOutsideRepository,
            )?;
            let mut file = File::open(&path).map_err(|error| {
                ValidationError::new(
                    ValidationCode::EvidenceMissing,
                    Some(index),
                    format!("{}: {error}", evidence.path),
                )
            })?;
            let mut hasher = Sha256::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let read = file.read(&mut buffer).map_err(|error| {
                    ValidationError::new(
                        ValidationCode::EvidenceMissing,
                        Some(index),
                        format!("{}: {error}", evidence.path),
                    )
                })?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            validate_digest(evidence, &hasher.finalize(), index)?;
        }
    }
    Ok(())
}

fn canonical_regular_file(
    repository_root: &Path,
    canonical_root: &Path,
    relative: &str,
    index: usize,
    missing: ValidationCode,
    not_file: ValidationCode,
    outside: ValidationCode,
) -> Result<PathBuf, ValidationError> {
    let candidate = repository_root.join(relative);
    let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
        ValidationError::new(missing, Some(index), format!("{relative}: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(ValidationError::new(
            not_file,
            Some(index),
            format!("{relative} is not a regular file"),
        ));
    }
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        ValidationError::new(missing, Some(index), format!("{relative}: {error}"))
    })?;
    if !canonical.starts_with(canonical_root) {
        return Err(ValidationError::new(
            outside,
            Some(index),
            format!("{relative} resolves outside the repository root"),
        ));
    }
    Ok(canonical)
}

pub(crate) fn validate_digest(
    evidence: &Evidence,
    digest: &[u8],
    index: usize,
) -> Result<(), ValidationError> {
    let digest = hex(digest);
    if digest != evidence.sha256 {
        return Err(ValidationError::new(
            ValidationCode::EvidenceDigestMismatch,
            Some(index),
            format!(
                "{} digest mismatch: recorded {}, recomputed {digest}",
                evidence.path, evidence.sha256
            ),
        ));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}
