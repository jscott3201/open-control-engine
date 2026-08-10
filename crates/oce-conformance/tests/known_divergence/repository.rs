//! Repository evidence-file checks layered over the pure register reader.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use super::reader::{Evidence, Register, ValidationCode, ValidationError};

pub(crate) fn validate_repository(
    register: &Register,
    repository_root: &Path,
) -> Result<(), ValidationError> {
    for (index, entry) in register.entries.iter().enumerate() {
        let comparison_path = repository_root.join(&entry.comparison_reference);
        let comparison_metadata = fs::symlink_metadata(&comparison_path).map_err(|error| {
            ValidationError::new(
                ValidationCode::ComparisonReferenceMissing,
                Some(index),
                format!("{}: {error}", entry.comparison_reference),
            )
        })?;
        if !comparison_metadata.is_file() {
            return Err(ValidationError::new(
                ValidationCode::ComparisonReferenceNotFile,
                Some(index),
                format!("{} is not a regular file", entry.comparison_reference),
            ));
        }
        for evidence in &entry.evidence {
            let path = repository_root.join(&evidence.path);
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                ValidationError::new(
                    ValidationCode::EvidenceMissing,
                    Some(index),
                    format!("{}: {error}", evidence.path),
                )
            })?;
            if !metadata.is_file() {
                return Err(ValidationError::new(
                    ValidationCode::EvidenceNotFile,
                    Some(index),
                    format!("{} is not a regular file", evidence.path),
                ));
            }
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
