//! Descriptor-relative bounded reads for repository evidence.

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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt as _, symlink};

    #[test]
    fn descriptor_walk_rejects_ancestor_symlink_and_final_fifo() {
        let root = std::env::temp_dir().join(format!("oce-safe-read-{}", std::process::id()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        std::fs::create_dir(root.join("real")).unwrap();
        std::fs::write(root.join("real/file"), b"ok").unwrap();
        symlink(root.join("real"), root.join("link")).unwrap();
        assert!(read(&root, "link/file").is_err());
        let fifo = root.join("real/fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(read(&root, "real/fifo").unwrap_err().contains("regular"));
        std::fs::remove_dir_all(root).unwrap();
    }
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
