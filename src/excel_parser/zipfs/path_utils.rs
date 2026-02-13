use super::ZipFsError;
use std::borrow::Cow;

/// Checks whether a path is safe to use in a ZIP archive.
///
/// A path is considered safe if it is not empty and does not contain the
/// directory traversal component `".."`.
///
/// # Arguments
///
/// * `path` - The path string to check.
///
/// # Returns
///
/// `true` if the path is safe, `false` otherwise.
#[inline]
pub fn is_safe_path(path: &str) -> bool {
    !path.is_empty() && !path.contains("..")
}

/// Validates and normalizes a path for use within a ZIP filesystem.
///
/// This function first normalizes the path by removing leading slashes and
/// converting backslashes to forward slashes. It then checks that the
/// resulting path is not empty and does not contain any `".."` components,
/// which could lead to directory traversal attacks.
///
/// # Arguments
///
/// * `path` - The raw path string to validate.
///
/// # Returns
///
/// * `Ok(String)` – The normalized path, owned as a `String`.
/// * `Err(ZipFsError)` – If the path is empty or contains `".."`, an
///   appropriate error is returned.
#[inline]
pub fn validate_path(path: &str) -> Result<String, ZipFsError> {
    let normalized = normalize_path(path);
    
    if normalized.is_empty() {
        return Err(ZipFsError::InvalidPattern("empty path".into()));
    }

    if normalized.contains("..") {
        return Err(ZipFsError::InvalidPattern(
            "path traversal not allowed".into(),
        ));
    }
    
    Ok(normalized.into_owned())
}

/// Returns the parent directory of a given path.
///
/// This function extracts the portion of the path before the last `/`
/// separator. If there is no separator, it returns an empty string.
/// Trailing slashes are not specially handled; the last component after
/// the final slash is considered the file or directory name.
///
/// # Arguments
///
/// * `path` - A path string, expected to use `/` as the separator.
///
/// # Returns
///
/// The parent directory path, or an empty string if there is no parent.
#[inline]
pub fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |pos| &path[..pos])
}

/// Normalizes a filesystem path for consistent internal representation.
///
/// This function:
/// - Removes any leading `/` characters.
/// - Converts all `\` (backslash) characters into `/` (forward slash).
///
/// The result is returned as a `Cow<str>` to avoid unnecessary allocations
/// when no changes are needed.
///
/// # Arguments
///
/// * `path` - The raw path string to normalize.
///
/// # Returns
///
/// A normalized path, possibly borrowed or owned.
#[inline]
pub fn normalize_path(path: &str) -> Cow<'_, str> {
    if path.starts_with('/') || path.contains('\\') {
        let trimmed = path.trim_start_matches('/');
        if trimmed.contains('\\') {
            trimmed.replace('\\', "/").into()
        } else {
            trimmed.into()
        }
    } else {
        path.into()
    }
}

/// Normalizes a directory path by removing leading and trailing slashes.
///
/// This function strips any `/` characters from the start and end of the
/// input string, producing a clean directory name suitable for use as a
/// key or identifier.
///
/// # Arguments
///
/// * `dir` - The raw directory path string.
///
/// # Returns
///
/// A normalized directory path, possibly borrowed or owned.
#[inline]
pub fn normalize_dir(dir: &str) -> Cow<'_, str> {
    let trimmed = dir.trim_start_matches('/').trim_end_matches('/');
    if trimmed.len() == dir.len() {
        dir.into()
    } else {
        trimmed.to_string().into()
    }
}