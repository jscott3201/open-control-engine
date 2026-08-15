//! Descriptor-relative bounded reads for Line evidence artifacts.

use std::io::Read as _;
use std::path::{Component, Path};

const MAX_BYTES: usize = 1024 * 1024;

#[cfg(unix)]
pub(super) fn read(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    use rustix::fs::{Mode, OFlags, open, openat};

    let mut directory = open(
        root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| error.to_string())?;
    let components = Path::new(relative).components().collect::<Vec<_>>();
    for component in &components[..components.len().saturating_sub(1)] {
        let Component::Normal(segment) = component else {
            return Err("invalid descriptor-relative path".into());
        };
        directory = openat(
            &directory,
            *segment,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| error.to_string())?;
    }
    let Some(Component::Normal(file_name)) = components.last() else {
        return Err("invalid descriptor-relative final component".into());
    };
    let file = openat(
        &directory,
        *file_name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| error.to_string())?;
    let mut file = std::fs::File::from(file);
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err("artifact descriptor is not a bounded regular file".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("artifact descriptor exceeded its bound while reading".into());
    }
    Ok(bytes)
}

#[cfg(not(unix))]
pub(super) fn read(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    use std::os::windows::fs::MetadataExt as _;

    let components = Path::new(relative).components().collect::<Vec<_>>();
    let mut path = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err("invalid repository-relative artifact path".into());
        };
        path.push(segment);
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("artifact path contains a Windows reparse point".into());
        }
        #[cfg(not(windows))]
        if metadata.file_type().is_symlink() {
            return Err("artifact path contains a symlink component".into());
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            return Err("artifact ancestor is not a directory".into());
        }
    }
    let before = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !before.is_file() || before.len() > MAX_BYTES as u64 {
        return Err("artifact path is not a bounded regular file".into());
    }
    let mut file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
    let opened = file.metadata().map_err(|error| error.to_string())?;
    if !opened.is_file() || opened.len() > MAX_BYTES as u64 {
        return Err("opened artifact is not a bounded regular file".into());
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    file.by_ref()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("artifact exceeded its bound while reading".into());
    }
    Ok(bytes)
}
