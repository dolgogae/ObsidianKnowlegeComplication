//! Source handles. Linux/macOS resolve each component without following links;
//! path-based enumeration never grants authority to reopen a discovered file.

use std::fs::{self, File};
use std::path::Path;

use crate::error::{OkcError, Result};

pub(crate) struct SourceDirectory {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    handle: File,
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    root: std::path::PathBuf,
}

impl SourceDirectory {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let before = fs::symlink_metadata(path).map_err(|error| OkcError::io(path, error))?;
        if before.file_type().is_symlink() || !before.is_dir() {
            return Err(unsafe_source(
                path,
                "source root must be a non-symlink directory",
            ));
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let canonical = fs::canonicalize(path).map_err(|error| OkcError::io(path, error))?;
            let handle = open_absolute_directory(&canonical)?;
            let after = handle
                .metadata()
                .map_err(|error| OkcError::io(path, error))?;
            ensure_same_file(path, &before, &after)?;
            Ok(Self { handle })
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        Ok(Self {
            root: path.to_path_buf(),
        })
    }

    pub(crate) fn open_file(&self, relative: &Path) -> Result<File> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use rustix::fs::{Mode, OFlags, openat};
            use std::path::Component;

            let mut components = relative.components().peekable();
            let mut parent = self
                .handle
                .try_clone()
                .map_err(|error| OkcError::io(relative, error))?;
            while let Some(component) = components.next() {
                let Component::Normal(name) = component else {
                    return Err(unsafe_source(
                        relative,
                        "source member must be strictly relative",
                    ));
                };
                let is_directory = components.peek().is_some();
                let mut flags =
                    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
                if is_directory {
                    flags |= OFlags::DIRECTORY;
                }
                let file = File::from(
                    openat(&parent, name, flags, Mode::empty())
                        .map_err(|error| OkcError::io(relative, error.into()))?,
                );
                if !is_directory {
                    require_regular_file(relative, &file)?;
                    return Ok(file);
                }
                parent = file;
            }
            Err(unsafe_source(relative, "source member cannot be empty"))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        open_regular_source(&self.root.join(relative))
    }
}

#[cfg(any(
    feature = "archives",
    not(any(target_os = "linux", target_os = "macos"))
))]
pub(crate) fn open_regular_source(path: &Path) -> Result<File> {
    let before = fs::symlink_metadata(path).map_err(|error| OkcError::io(path, error))?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(unsafe_source(
            path,
            "source must be a non-symlink regular file",
        ));
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let file = {
        let name = path
            .file_name()
            .ok_or_else(|| unsafe_source(path, "source has no filename"))?;
        let parent = path
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let canonical_parent =
            fs::canonicalize(parent).map_err(|error| OkcError::io(parent, error))?;
        SourceDirectory::open(&canonical_parent)?.open_file(Path::new(name))?
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let file = File::open(path).map_err(|error| OkcError::io(path, error))?;
    require_regular_file(path, &file)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    ensure_same_file(
        path,
        &before,
        &file.metadata().map_err(|error| OkcError::io(path, error))?,
    )?;
    Ok(file)
}

fn require_regular_file(path: &Path, file: &File) -> Result<()> {
    if !file
        .metadata()
        .map_err(|error| OkcError::io(path, error))?
        .is_file()
    {
        return Err(unsafe_source(path, "opened source is not a regular file"));
    }
    Ok(())
}

fn unsafe_source(path: &Path, reason: &str) -> OkcError {
    OkcError::UnsafePath {
        path: path.display().to_string(),
        reason: reason.into(),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn ensure_same_file(path: &Path, before: &fs::Metadata, after: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(OkcError::IdentityMismatch(format!(
            "source was replaced before opening `{}`",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_absolute_directory(path: &Path) -> Result<File> {
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::path::Component;

    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    let mut directory = File::from(
        open("/", flags, Mode::empty()).map_err(|error| OkcError::io(path, error.into()))?,
    );
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = File::from(
                    openat(&directory, name, flags, Mode::empty())
                        .map_err(|error| OkcError::io(path, error.into()))?,
                );
            }
            _ => return Err(unsafe_source(path, "source root is not canonical")),
        }
    }
    Ok(directory)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use std::io::Read as _;
    use std::os::unix::fs::symlink;

    #[test]
    fn pinned_source_root_does_not_follow_a_replacement_alias() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("source");
        let outside = temporary.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(root.join("note.md"), "original").unwrap();
        fs::write(outside.join("note.md"), "secret").unwrap();
        let pinned = SourceDirectory::open(&root).unwrap();
        fs::rename(&root, temporary.path().join("moved")).unwrap();
        symlink(&outside, &root).unwrap();
        let mut text = String::new();
        pinned
            .open_file(Path::new("note.md"))
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "original");
        assert!(SourceDirectory::open(&root).is_err());
    }

    #[test]
    fn pinned_source_rejects_replaced_leaf_and_ancestor_symlinks() {
        for relative in ["note.md", "folder"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("source");
            fs::create_dir_all(root.join("folder")).unwrap();
            fs::write(root.join("note.md"), "original").unwrap();
            fs::write(root.join("folder/note.md"), "original").unwrap();
            let pinned = SourceDirectory::open(&root).unwrap();
            let moved = temporary.path().join("outside");
            fs::rename(root.join(relative), &moved).unwrap();
            symlink(&moved, root.join(relative)).unwrap();
            let member = if relative == "folder" {
                "folder/note.md"
            } else {
                "note.md"
            };
            assert!(pinned.open_file(Path::new(member)).is_err());
        }
    }
}
