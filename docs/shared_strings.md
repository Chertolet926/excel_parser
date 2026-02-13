# SharedStrings Documentation

A parsed table of shared strings extracted from Excel workbooks (`xl/sharedStrings.xml`).

## Overview

Excel stores repeated string values in a central location and references them by index from cell values. This struct parses that XML and provides efficient access to individual strings along with fuzzy search capability.

### Memory Optimization

Strings are stored as `Box<str>` to reduce memory overhead. This immutable, heap-allocated representation avoids the extra capacity tracking of `String` and allows cheap cloning.

### Thread Safety

The struct is `Send + Sync` because it contains only owned data and immutable references. Multiple threads can safely access a shared instance.

## Quick Start

```rust
use std::fs::File;
use excel_parser::SharedStrings;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load shared strings from Excel file
    let data = std::fs::read("xl/sharedStrings.xml")?;
    let shared = SharedStrings::load(&data)?;

    // Access strings by index
    println!("Total shared strings: {}", shared.len());
    if let Some(first) = shared.get(0) {
        println!("First string: {}", first);
    }

    // Fuzzy search
    let results = shared.fuzzy_find("math", 30);
    for (idx, score) in results.iter().take(5) {
        if let Some(s) = shared.get(*idx) {
            println!("[{}] {} (score: {})", idx, s, score);
        }
    }

    Ok(())
}
```

---

## Public API

### SharedStrings

#### load()

```rust
pub fn load(xml: &[u8]) -> Result<Self, quick_xml::Error>
```

Parses the shared strings XML content and builds the string table.

**Arguments:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `xml` | `&[u8]` | Raw bytes of `xl/sharedStrings.xml` |

**Returns:** A `SharedStrings` instance or an error.

**Errors:** Returns `quick_xml::Error` for malformed XML.

**XML Structure:**
```xml
<sst>
  <si><t>First string</t></si>
  <si><t>Second </t><t>string</t></si>
  ...
</sst>
```

---

#### get()

```rust
pub fn get(&self, index: usize) -> Option<&str>
```

Returns a reference to the shared string at the given index.

**Arguments:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `usize` | Zero-based position in the shared strings table |

**Returns:** `Some(&str)` if valid, `None` otherwise.

**Example:**
```rust
if let Some(s) = shared.get(0) {
    assert_eq!(s, "First string");
}
assert!(shared.get(9999).is_none());
```

---

#### len()

```rust
pub fn len(&self) -> usize
```

Returns the total number of shared strings in the table.

**Returns:** Number of unique shared strings (`usize`).

---

## Fuzzy Search

The fuzzy search uses the SkimMatcherV2 algorithm (similar to fzf).

### fuzzy_find()

```rust
pub fn fuzzy_find(&self, query: &str, threshold: i64) -> Vec<(usize, i64)>
```

Performs a fuzzy search across all shared strings.

**Arguments:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `query` | `&str` | Search pattern (exact or fuzzy) |
| `threshold` | `i64` | Minimum score to include a match |

**Threshold Guide:**
- `0` — return all matches
- `30-50` — typical fuzzy matches
- `100+` — near-exact matches

**Returns:** Vector of `(index, score)` tuples, sorted by descending score.

**Scoring:**
| Score | Meaning |
|-------|---------|
| 100+ | Exact match |
| 50-99 | Case-insensitive match |
| 1-49 | Fuzzy match with gaps |
| 0 | No match |

**Example:**
```rust
// Find courses related to "math"
let results = shared.fuzzy_find("math", 0);
for (idx, score) in results.iter().take(5) {
    if let Some(s) = shared.get(*idx) {
        println!("[{}] {} (score: {})", idx, s, score);
    }
}
```

---

### fuzzy_find_with_matcher()

```rust
pub fn fuzzy_find_with_matcher(
    &self,
    matcher: &SkimMatcherV2,
    query: &str,
    threshold: i64
) -> Vec<(usize, i64)>
```

Performs fuzzy search using a pre-configured matcher instance.

Useful when you need to set matcher options (e.g., case sensitivity) once and reuse it.

**Arguments:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `matcher` | `&SkimMatcherV2` | Pre-configured matcher |
| `query` | `&str` | Search pattern |
| `threshold` | `i64` | Minimum matching score |

---

### fuzzy_find_indices()

```rust
pub fn fuzzy_find_indices(&self, query: &str, threshold: i64) -> Vec<usize>
```

Convenience method returning only the indices of matching strings.

**Arguments:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `query` | `&str` | Search pattern |
| `threshold` | `i64` | Minimum matching score |

**Returns:** Vector of indices, sorted by match quality.

---

## Performance

- **Parsing:** Single-pass O(n) algorithm where n is XML size
- **Fuzzy Search:** O(n * m) where n = number of strings, m = query length
- **Memory:** Proportional to number and length of unique strings

---

## Thread Safety

All methods are immutable and can be called concurrently from multiple threads.
