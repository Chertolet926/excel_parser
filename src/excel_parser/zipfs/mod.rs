mod filters;
mod path_utils;

pub use path_utils::{normalize_path, parent_dir, normalize_dir, is_safe_path};
use std::{io::{Read, Seek, SeekFrom}, borrow::Cow, sync::Arc};
use zip::{result::ZipError, ZipArchive, read::ZipFile};
pub use filters::FilterSet;
use rustc_hash::FxHashMap;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Custom error type (thiserror)
// ---------------------------------------------------------------------------

/// Error type for ZIP filesystem operations.
///
/// Wraps errors from the underlying ZIP crate, I/O errors, and custom
/// validation errors (size limit exceeded, invalid path patterns).
#[derive(Error, Debug)]
pub enum ZipFsError {
    /// An error originating from the `zip` crate.
    #[error("ZIP error: {0}")]
    Zip(#[from] ZipError),

    /// The archive size exceeds the configured maximum allowed size.
    #[error("Archive size {0} exceeds limit {1}")]
    ArchiveTooLarge(u64, u64),

    /// A path or glob pattern was invalid (empty, contains "..", etc.).
    #[error("Invalid glob pattern: {0}")]
    InvalidPattern(String),

    /// An I/O error while reading the archive.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// ZipFs – in-memory virtual file system from a ZIP archive
// ---------------------------------------------------------------------------

/// In-memory virtual file system loaded from a ZIP archive.
///
/// # Features
/// - Glob and exact-path filtering via [`FilterSet`].
/// - Directory index stores **only immediate children** (no recursion).
/// - Optional archive size limit (protection against OOM).
///
/// # Example
/// ```
/// # use zip_fs::{ZipFs, FilterSet, ZipFsError};
/// # fn main() -> Result<(), ZipFsError> {
/// let data = std::fs::File::open("archive.zip")?;
/// let filter = FilterSet::new()
///     .add_exact("doc.txt")?
///     .add_glob("images/*.png")?;
/// let fs = ZipFs::new(data, Some(filter), Some(100_000_000))?;
///
/// if let Some(content) = fs.get_file("doc.txt") {
///     println!("doc.txt size: {} bytes", content.len());
/// }
/// for file in fs.list_files("images") {
///     println!(" - {}", file);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ZipFs {
    /// File storage: normalized path → raw content.
    files: FxHashMap<Arc<str>, Vec<u8>>,
    /// Directory index: normalized directory path → list of full file paths in it.
    dir_index: FxHashMap<Arc<str>, Vec<Arc<str>>>,
    /// Cache for parent directory strings to avoid repeated allocations.
    parent_cache: FxHashMap<String, Arc<str>>
}

impl ZipFs {
    /// Loads only files that match the provided [`FilterSet`].
    ///
    /// # Arguments
    /// * `reader` – source of ZIP data (must implement `Read + Seek`).
    /// * `filter` – optional [`FilterSet`] with exact paths and/or glob patterns.
    /// * `max_archive_size` – optional maximum allowed archive size in bytes.
    ///   If the archive is larger, an `ArchiveTooLarge` error is returned.
    ///
    /// # Errors
    /// * `ZipFsError::ArchiveTooLarge` – archive exceeds the size limit.
    /// * `ZipFsError::Zip` – malformed ZIP structure.
    /// * `ZipFsError::Io` – I/O error.
    pub fn new<R: Read + Seek>(
        reader: R,
        filter: Option<FilterSet>,
        max_archive_size: Option<u64>,
    ) -> Result<Self, ZipFsError> {
        let reader = Self::check_archive_size(reader, max_archive_size)?;

        let archive = ZipArchive::new(reader)?;
        let mut fs = ZipFs {
            files: FxHashMap::with_capacity_and_hasher(archive.len(), Default::default()),
            dir_index: FxHashMap::with_capacity_and_hasher(archive.len() / 5, Default::default()),
            parent_cache: FxHashMap::with_capacity_and_hasher(64, Default::default())
        };

        fs.load_entries(archive, filter.as_ref());
        Ok(fs)
    }

    // -------------------------------------------------------------------------
    // Public API
    // -------------------------------------------------------------------------

    /// Returns the **full paths** of files that are **immediate children** of `dir_path`.
    ///
    /// Subdirectories are **not** traversed. To list files in a subdirectory,
    /// call this method with that subdirectory's path.
    ///
    /// # Arguments
    /// * `dir_path` – a directory path (e.g., `"images"` or `"docs/2025"`).
    ///
    /// # Returns
    /// A vector of full file paths (as string slices) that reside directly under
    /// the given directory. If the directory does not exist or contains no files,
    /// an empty vector is returned.
    pub fn list_files(&self, dir_path: &str) -> Vec<&str> {
        let normalized = normalize_dir(dir_path);
        self.dir_index
            .get(&*normalized)
            .map(|v| v.iter().map(AsRef::as_ref).collect())
            .unwrap_or_default()
    }

    /// Returns the raw content of a file, if loaded.
    ///
    /// # Arguments
    /// * `path` – the normalized path of the file (e.g., `"doc.txt"`).
    ///
    /// # Returns
    /// `Some(&[u8])` containing the file's data, or `None` if the file was not
    /// found (either because it wasn't in the archive or it was filtered out).
    pub fn get_file(&self, path: &str) -> Option<&[u8]> {
        let normalized = normalize_path(path);
        self.files.get(&*normalized).map(|v| v.as_slice())
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    /// Indexes a file under its **immediate** parent directory.
    ///
    /// Updates `dir_index` so that the file's path is recorded under the
    /// normalized parent directory key. The root directory is represented by
    /// an empty string.
    ///
    /// # Arguments
    /// * `file_path` – the full normalized path of the file (as an `Arc<str>`).
    #[inline]
    fn index_file(&mut self, file_path: Arc<str>) {
        let parent = parent_dir(file_path.as_ref());
        let parent_key = if parent.is_empty() {
            self.parent_cache
                .entry(String::new())
                .or_insert_with(|| Arc::from(""))
                .clone()
        } else {
            self.parent_cache
                .entry(parent.to_string())
                .or_insert_with(|| Arc::from(parent))
                .clone()
        };
        
        self.dir_index.entry(parent_key)
            .or_default()
            .push(file_path);
    }

    /// Tries to read the entire file content into memory.
    ///
    /// Performs basic capacity checks to avoid allocation failures for very
    /// large files. Returns `None` if the file size exceeds `usize::MAX` or if
    /// memory reservation fails.
    ///
    /// # Arguments
    /// * `file` – the ZIP file entry to read.
    /// * `name` – the normalized path as an `Arc<str>` (to be reused in the result).
    ///
    /// # Returns
    /// An optional tuple `(Arc<str>, Vec<u8>)` containing the path and the file
    /// content, or `None` if reading failed.
    fn try_read_file_content<R: Read>(mut file: ZipFile<R>, name: Arc<str>) -> Option<(Arc<str>, Vec<u8>)> {
        let size = file.size();
        if size > usize::MAX as u64 { return None; }
        
        let mut content = Vec::new();
        if content.try_reserve_exact(size as usize).is_err() { return None; }
        
        file.read_to_end(&mut content).ok()?;
        Some((name, content))
    }

    /// Iterates over all ZIP entries, applies filters, and loads matching files.
    ///
    /// This method populates `files` and `dir_index` with entries that are not
    /// directories, have safe paths, and (if a filter is provided) match the filter.
    /// Corrupted entries are silently skipped.
    ///
    /// # Arguments
    /// * `archive` – the opened ZIP archive.
    /// * `filter` – optional reference to a [`FilterSet`].
    fn load_entries<R: Read + Seek>(
        &mut self,
        mut archive: ZipArchive<R>,
        filter: Option<&FilterSet>,
    ) {
        // Pre-allocate storage for files with known capacity.
        self.files.reserve(archive.len());

        for i in 0..archive.len() {
            // Silently skip corrupted entries.
            let file = match archive.by_index(i) {
                Ok(f) => f,
                Err(_) => continue,
            };

            // Normalize the entry name without allocating if already clean.
            let name_cow = normalize_path(file.name());
            let name_str: &str = name_cow.as_ref();

            // Skip directories (ZIP entries ending with '/') and unsafe paths.
            if name_str.ends_with('/') || !is_safe_path(name_str) { continue; }

            // Apply filter if present – allocation‑free matching on normalized path.
            if let Some(filter) = filter {
                if !filter.matches_str(name_str) { continue; }
            }

            // Convert to Arc<str> without extra copy if the name is already owned.
            let name_arc = match name_cow {
                Cow::Borrowed(s) => Arc::from(s),
                Cow::Owned(s) => Arc::from(s),
            };

            // Try to read the file content.
            if let Some((name, content)) = Self::try_read_file_content(file, name_arc) {
                self.files.insert(name.clone(), content);
                self.index_file(name);
            }
        }
    }

    /// Checks whether the archive size exceeds the optional limit.
    ///
    /// If a limit is provided, the reader is seeked to the end to obtain the
    /// total size. After the check, the reader is rewound to the beginning
    /// so that it can be used to construct the ZIP archive.
    ///
    /// # Arguments
    /// * `reader` – the data source.
    /// * `max_archive_size` – optional maximum size in bytes.
    ///
    /// # Errors
    /// Returns `ZipFsError::ArchiveTooLarge` if the size exceeds the limit,
    /// or `ZipFsError::Io` if seeking fails.
    fn check_archive_size<R: Read + Seek>(
        mut reader: R,
        max_archive_size: Option<u64>,
    ) -> Result<R, ZipFsError> {
        if let Some(limit) = max_archive_size {
            let size = reader.seek(SeekFrom::End(0))?;
            if size > limit { return Err(ZipFsError::ArchiveTooLarge(size, limit)); }
        }
        
        reader.seek(SeekFrom::Start(0))?;
        Ok(reader)
    }
}
