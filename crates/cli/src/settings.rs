use std::collections::BTreeMap;
use std::env;
use std::fs::{self, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde_json::value::RawValue;
use tempfile::NamedTempFile;

// Keep unedited values verbatim, including numbers larger than Rust's native JSON numbers.
type Document = BTreeMap<String, Box<RawValue>>;
pub type Changes = BTreeMap<&'static str, Option<serde_json::Value>>;

pub fn settings_path() -> io::Result<PathBuf> {
    let home = || env::home_dir().ok_or_else(|| io::Error::other("Home directory is unavailable"));
    let dir = match env::var_os("PI_CODING_AGENT_DIR").filter(|dir| !dir.is_empty()) {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            match dir.strip_prefix("~") {
                Ok(rest) => home()?.join(rest),
                Err(_) => dir,
            }
        }
        None => home()?.join(".pi/agent"),
    };
    Ok(std::path::absolute(dir)?.join("settings.json"))
}

fn read_object(raw: &str) -> io::Result<Document> {
    if !raw.trim_start().starts_with('{') {
        return Err(io::Error::other("Expected a JSON object"));
    }
    serde_json::from_str(raw).map_err(io::Error::other)
}

fn apply_changes(original: &Document, changes: Changes) -> io::Result<Document> {
    let mut document = original.clone();
    let mut nested = BTreeMap::<&str, Document>::new();
    for (path, value) in changes {
        let (object, key) = if let Some((root, leaf)) = path.split_once('.') {
            if value.is_none() && !original.contains_key(root) && !nested.contains_key(root) {
                continue;
            }
            let parent = match nested.entry(root) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let parent = original
                        .get(root)
                        .map_or_else(|| Ok(Document::new()), |raw| read_object(raw.get()))?;
                    entry.insert(parent)
                }
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            };
            (parent, leaf)
        } else {
            (&mut document, path)
        };
        if let Some(value) = value {
            let raw = serde_json::value::to_raw_value(&value).map_err(io::Error::other)?;
            object.insert(key.into(), raw);
        } else {
            object.remove(key);
        }
    }
    for (root, parent) in nested {
        let raw = serde_json::value::to_raw_value(&parent).map_err(io::Error::other)?;
        document.insert(root.into(), raw);
    }
    Ok(document)
}

struct FileVersion {
    bytes: Vec<u8>,
    meta: Option<Metadata>,
}

impl FileVersion {
    fn matches(&self, other: &Self) -> bool {
        let (Some(old), Some(new)) = (&self.meta, &other.meta) else {
            return false;
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let id = |m: &Metadata| (m.dev(), m.ino(), m.ctime(), m.ctime_nsec());
            if id(old) != id(new) {
                return false;
            }
        }
        self.bytes == other.bytes && old.modified().ok() == new.modified().ok()
    }
}

fn read_file(path: &Path) -> io::Result<Option<FileVersion>> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_symlink() => {
            return Err(io::Error::other(
                "settings.json is a symbolic link; it will not be opened",
            ));
        }
        Ok(meta) if !meta.is_file() => {
            return Err(io::Error::other("settings.json is not a regular file"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("settings.json is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Some(FileVersion {
        bytes,
        meta: Some(file.metadata()?),
    }))
}

pub struct Settings {
    source: Option<FileVersion>,
    document: Document,
}

pub enum SaveError {
    Changed,
    Io(io::Error),
}

impl From<io::Error> for SaveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl Settings {
    pub fn load(path: &Path) -> io::Result<Self> {
        let source = read_file(path)?;
        let document = match &source {
            Some(source) => {
                let json = source
                    .bytes
                    .strip_prefix(b"\xef\xbb\xbf")
                    .unwrap_or(&source.bytes);
                if json.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
                    return Err(io::Error::other("Expected a JSON object"));
                }
                serde_json::from_slice(json)
                    .map_err(|error| io::Error::other(format!("Invalid JSON: {error}")))?
            }
            None => Document::new(),
        };
        Ok(Self { source, document })
    }

    pub fn missing(&self) -> bool {
        self.source.is_none()
    }

    pub fn value(&self, path: &str) -> io::Result<Option<serde_json::Value>> {
        let decode = |raw: &RawValue| serde_json::from_str(raw.get()).map_err(io::Error::other);
        if let Some((root, leaf)) = path.split_once('.') {
            let Some(raw) = self.document.get(root) else {
                return Ok(None);
            };
            return read_object(raw.get())?
                .get(leaf)
                .map(Box::as_ref)
                .map(decode)
                .transpose();
        }
        self.document
            .get(path)
            .map(Box::as_ref)
            .map(decode)
            .transpose()
    }

    pub fn save(&mut self, path: &Path, changes: Changes) -> Result<(), SaveError> {
        if changes.is_empty() {
            return Ok(());
        }
        self.check_version(path)?;
        let document = apply_changes(&self.document, changes)?;
        let mut bytes = serde_json::to_vec_pretty(&document).map_err(io::Error::other)?;
        bytes.push(b'\n');
        let parent = path.parent().expect("settings path has a directory");
        if self.missing() {
            fs::create_dir_all(parent)?;
        }
        let mut temp = NamedTempFile::new_in(parent)?;
        temp.write_all(&bytes)?;
        if let Some(meta) = self.source.as_ref().and_then(|source| source.meta.as_ref()) {
            temp.as_file().set_permissions(meta.permissions())?;
        }
        temp.as_file().sync_all()?;
        self.check_version(path)?;
        let file = if self.missing() {
            temp.persist_noclobber(path).map_err(|err| {
                if err.error.kind() == io::ErrorKind::AlreadyExists {
                    SaveError::Changed
                } else {
                    SaveError::Io(err.error)
                }
            })?
        } else {
            temp.persist(path).map_err(|err| err.error)?
        };
        // The rename already succeeded. A failed metadata read must not report it as a failed write.
        self.source = Some(FileVersion {
            bytes,
            meta: file.metadata().ok(),
        });
        self.document = document;
        Ok(())
    }

    fn check_version(&self, path: &Path) -> Result<(), SaveError> {
        match (&self.source, read_file(path)?) {
            (None, None) => Ok(()),
            (Some(original), Some(current)) if original.matches(&current) => Ok(()),
            _ => Err(SaveError::Changed),
        }
    }
}
