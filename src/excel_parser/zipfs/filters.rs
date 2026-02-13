use std::sync::Arc;
use rustc_hash::FxHashSet;
use super::path_utils::validate_path;
use super::ZipFsError;

/// A set of filters that can match paths either exactly or by glob pattern.
///
/// This structure is useful for selecting a subset of entries from a ZIP archive,
/// for example when extracting or listing only specific files and directories.
/// Filters are added in a builder‑style fashion; each addition validates and
/// normalizes the input path or pattern.
///
/// # Example
/// ```
/// # use zip_fs::filter::FilterSet;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let filter = FilterSet::new()
///     .add_exact("xl/workbook.xml")?
///     .add_glob("xl/worksheets/*.xml")?;
///
/// assert!( filter.matches_str("xl/workbook.xml"));
/// assert!( filter.matches_str("xl/worksheets/sheet1.xml"));
/// assert!(!filter.matches_str("xl/styles.xml"));
/// # Ok(())
/// # }
/// ```
///
/// If no filters are added, the set is considered empty and `matches_str` will
/// always return `false` (i.e., nothing matches). To match everything, simply
/// don't use a filter or treat an empty filter as “allow all” at the caller level.
#[derive(Debug, Default)]
pub struct FilterSet {
    /// Exact paths that must be matched. Stored as reference‑counted strings
    /// to reduce cloning overhead when checking many paths.
    exact: FxHashSet<Arc<str>>,
    /// Glob patterns, in the order they were added. They are evaluated in sequence
    /// using `fast_glob::glob_match`.
    globs: Vec<String>
}

impl FilterSet {
    /// Creates an empty filter set.
    ///
    /// Equivalent to `FilterSet::default()`.
    pub fn new() -> Self { Self::default() }

    /// Adds an exact path to the filter set.
    ///
    /// The path is first validated and normalized by [`validate_path`], which
    /// ensures it is not empty, does not contain directory‑traversal components,
    /// and has a consistent format (e.g., leading slashes removed). If validation
    /// fails, a `ZipFsError::InvalidPattern` is returned.
    ///
    /// # Arguments
    /// * `path` – The exact path to match (e.g., `"xl/workbook.xml"`).
    ///
    /// # Errors
    /// Returns `ZipFsError::InvalidPattern` if the path is empty, contains `".."`,
    /// or is otherwise invalid according to [`validate_path`].
    pub fn add_exact(mut self, path: &str) -> Result<Self, ZipFsError> {
        let normalized = validate_path(path)?;
        self.exact.insert(Arc::from(normalized));
        Ok(self)
    }

    /// Adds a glob pattern to the filter set.
    ///
    /// The pattern is validated and normalized in the same way as exact paths
    /// (see [`add_exact`](Self::add_exact)). After validation, it is stored
    /// for later matching. Matching is performed with the
    /// [`fast_glob::glob_match`] function, which supports the usual `*` and `?`
    /// wildcards.
    ///
    /// # Arguments
    /// * `pattern` – A glob pattern (e.g., `"xl/worksheets/*.xml"`).
    ///
    /// # Errors
    /// Returns `ZipFsError::InvalidPattern` if the pattern is empty, contains `".."`,
    /// or is otherwise invalid.
    pub fn add_glob(mut self, pattern: &str) -> Result<Self, ZipFsError> {
        let normalized = validate_path(pattern)?;
        self.globs.push(normalized);
        Ok(self)
    }

    /// Checks whether the given path matches any of the filters in the set.
    ///
    /// The check is performed in two steps:
    /// 1. Exact match against the set of exact paths (O(1) average).
    /// 2. If no exact match is found, each glob pattern is tested in order.
    ///
    /// # Arguments
    /// * `path` – The path to test (should already be normalized, e.g., by
    ///   [`validate_path`]).
    ///
    /// # Returns
    /// `true` if the path matches at least one filter, `false` otherwise.
    #[inline]
    pub fn matches_str(&self, path: &str) -> bool {
        if self.exact.contains(path) { return true; }
        self.globs.iter().any(|g| fast_glob::glob_match(g, path))
    }

    /// Returns `true` if no filters have been added to the set.
    ///
    /// An empty filter set matches **no** paths. If you need a set that matches
    /// everything, either avoid using a filter or treat the absence of filters
    /// as a special case in your logic.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.globs.is_empty()
    }
}