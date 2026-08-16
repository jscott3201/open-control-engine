//! Descriptor-relative bounded reads for Reliefs evidence artifacts.

use std::io::Read as _;
use std::path::{Component, Path};

const MAX_BYTES: usize = 1024 * 1024;

#[cfg(unix)]
pub(super) fn read(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::os::unix::fs::MetadataExt as _;

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
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > MAX_BYTES as u64 {
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

#[cfg(windows)]
pub(super) fn read(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_ATTRIBUTE_REPARSE_POINT: u64 = 0x400;
    const FILE_ATTRIBUTE_DIRECTORY: u64 = 0x10;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    #[derive(Clone, Copy)]
    struct HandleInfo {
        attributes: u64,
        links: u64,
        volume: u64,
        index: u64,
        size: u64,
    }
    fn handle_info(file: &std::fs::File) -> Result<HandleInfo, String> {
        let information =
            winapi_util::file::information(file).map_err(|error| error.to_string())?;
        Ok(HandleInfo {
            attributes: information.file_attributes(),
            links: information.number_of_links(),
            volume: information.volume_serial_number(),
            index: information.file_index(),
            size: information.file_size(),
        })
    }
    fn open_without_following(path: &Path, directory: bool) -> Result<std::fs::File, String> {
        let mut options = std::fs::OpenOptions::new();
        options.access_mode(0).custom_flags(
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
        );
        options.open(path).map_err(|error| error.to_string())
    }
    fn reject_reparse(info: HandleInfo) -> Result<(), String> {
        (info.attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0)
            .then_some(())
            .ok_or_else(|| "artifact path contains a Windows reparse point".into())
    }

    let components = Path::new(relative).components().collect::<Vec<_>>();
    let mut path = root.to_path_buf();
    let mut ancestors = Vec::new();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err("invalid repository-relative artifact path".into());
        };
        path.push(segment);
        if index + 1 < components.len() {
            let handle = open_without_following(&path, true)?;
            let info = handle_info(&handle)?;
            reject_reparse(info)?;
            if info.attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
                return Err("artifact ancestor is not a directory".into());
            }
            ancestors.push((path.clone(), info));
        }
    }
    let before_handle = open_without_following(&path, false)?;
    let before = handle_info(&before_handle)?;
    reject_reparse(before)?;
    if before.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
        || before.links != 1
        || before.size > MAX_BYTES as u64
    {
        return Err("artifact path is not a bounded regular file".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let opened = handle_info(&file)?;
    reject_reparse(opened)?;
    if opened.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
        || opened.links != 1
        || opened.size > MAX_BYTES as u64
        || (opened.volume, opened.index) != (before.volume, before.index)
    {
        return Err("opened artifact is not a bounded regular file".into());
    }
    for (ancestor, expected) in ancestors {
        let handle = open_without_following(&ancestor, true)?;
        let observed = handle_info(&handle)?;
        reject_reparse(observed)?;
        if observed.attributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || (observed.volume, observed.index) != (expected.volume, expected.index)
        {
            return Err("artifact ancestor changed while opening the file".into());
        }
    }
    let mut bytes = Vec::with_capacity(opened.size as usize);
    file.by_ref()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("artifact exceeded its bound while reading".into());
    }
    Ok(bytes)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn read(_root: &Path, _relative: &str) -> Result<Vec<u8>, String> {
    Err("bounded Reliefs artifact reads are unsupported on this platform".into())
}
