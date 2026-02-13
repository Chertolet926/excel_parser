# ZipFs Documentation

A high-performance, in-memory virtual file system for ZIP archives.

## Quick Start

```rust
use std::fs::File;
use excel_parser::{ZipFs, FilterSet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("archive.xlsx")?;
    let limit: u64 = 100 * 1024 * 1024; // 100 MiB

    // Load with filter
    let filter = FilterSet::new()
        .add_exact("xl/workbook.xml")?
        .add_glob("xl/worksheets/*.xml")?;

    let fs = ZipFs::new(file, Some(filter), Some(limit))?;

    // List files
    for sheet in fs.list_files("xl/worksheets") {
        println!("  - {}", sheet);
    }

    // Get file content
    if let Some(content) = fs.get_file("xl/workbook.xml") {
        println!("Size: {} bytes", content.len());
    }

    Ok(())
}
```

## Public API

### ZipFs

```rust
pub fn new<R: Read + Seek>(
    reader: R,
    filter: Option<FilterSet>,
    max_archive_size: Option<u64>,
) -> Result<Self, ZipFsError>
```

Creates a ZipFs from a ZIP archive.

| Parameter | Type | Description |
|-----------|------|-------------|
| `reader` | `R: Read + Seek` | ZIP data source |
| `filter` | `Option<FilterSet>` | File filter or `None` |
| `max_archive_size` | `Option<u64>` | Max archive size in bytes |

**Errors:**
- `ArchiveTooLarge` – archive exceeds size limit
- `Zip` – malformed ZIP structure
- `Io` – I/O error

---

### list_files()

```rust
pub fn list_files(&self, dir_path: &str) -> Vec<&str>
```

Lists files in a directory.

| Parameter | Type | Description |
|-----------|------|-------------|
| `dir_path` | `&str` | Directory path |

**Returns:** Vector of file paths as string slices.

---

### get_file()

```rust
pub fn get_file(&self, path: &str) -> Option<&[u8]>
```

Gets file content by path.

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | `&str` | File path |

**Returns:** `Some(&[u8])` or `None`.

---

## FilterSet

### new()

```rust
pub fn new() -> Self
```

Creates an empty filter set.

---

### add_exact()

```rust
pub fn add_exact(mut self, path: &str) -> Result<Self, ZipFsError>
```

Adds an exact path to match.

**Example:**
```rust
FilterSet::new().add_exact("xl/workbook.xml")?
```

---

### add_glob()

```rust
pub fn add_glob(mut self, pattern: &str) -> Result<Self, ZipFsError>
```

Adds a glob pattern (`*` and `?` supported).

**Example:**
```rust
FilterSet::new().add_glob("xl/worksheets/*.xml")?
```

---

### matches_str()

```rust
pub fn matches_str(&self, path: &str) -> bool
```

Tests if a path matches the filter.

**Example:**
```rust
assert!(filter.matches_str("xl/worksheets/sheet1.xml"));
```

---

## Error Types

| Error | Description |
|-------|-------------|
| `ArchiveTooLarge(u64, u64)` | (actual, limit) |
| `InvalidPattern(String)` | Empty or contains ".." |
| `Zip` | Malformed archive |
| `Io` | I/O error |
