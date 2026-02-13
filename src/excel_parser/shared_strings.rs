use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use quick_xml::{Reader, events::Event};
use std::mem::take;

// ---------------------------------------------------------------------------
// SharedStrings – parsed table of shared strings from Excel (xl/sharedStrings.xml)
// ---------------------------------------------------------------------------

/// Table of shared strings extracted from an Excel workbook.
///
/// Excel stores repeated string values in a central location (`xl/sharedStrings.xml`)
/// and references them by index from cell values. This struct parses that XML and
/// provides efficient access to individual strings along with fuzzy search capability.
///
/// # Memory Optimization
/// Strings are stored as `Box<str>` to reduce memory overhead. This immutable,
/// heap‑allocated representation avoids the extra capacity tracking of `String`
/// and allows cheap cloning via reference counting semantics.
///
/// # Thread Safety
/// The struct is `Send + Sync` because it contains only owned data and immutable
/// references. Multiple threads can safely access a shared instance.
///
/// # Example
/// ```
/// use excel_parser::SharedStrings;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let data = std::fs::read("xl/sharedStrings.xml")?;
/// let shared = SharedStrings::load(&data)?;
///
/// println!("Total shared strings: {}", shared.len());
/// if let Some(first) = shared.get(0) {
///     println!("First string: {}", first);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct SharedStrings {
    /// The actual strings stored as boxed slices to reduce memory overhead.
    /// `Box<str>` is a compact, immutable representation of a string on the heap.
    strings: Vec<Box<str>>,
}

impl SharedStrings {
    /// Parses the shared strings XML content and builds the string table.
    ///
    /// This method reads `xl/sharedStrings.xml` from an Excel file (`.xlsx` is a ZIP
    /// archive) and extracts all `<si>` (string item) elements. Each string may
    /// contain multiple `<t>` (text) fragments that are concatenated together.
    ///
    /// # XML Structure
    /// ```xml
    /// <sst>
    ///   <si><t>First string</t></si>
    ///   <si><t>Second </t><t>string</t></si>
    ///   ...
    /// </sst>
    /// ```
    ///
    /// # Parsing Details
    /// - `trim_text(false)` preserves all whitespace; Excel strings may contain
    ///   meaningful leading/trailing spaces.
    /// - `check_end_names = false` skips expensive validation since Excel produces
    ///   well‑formed XML.
    /// - `expand_empty_elements = false` avoids creating empty events for
    ///   self‑closing tags.
    /// - A `current` buffer accumulates text from multiple `<t>` fragments within
    ///   a single `<si>` element.
    /// - `std::mem::take` resets the buffer after pushing, avoiding an extra allocation.
    ///
    /// # Arguments
    /// * `xml` – raw bytes of `xl/sharedStrings.xml`.
    ///
    /// # Returns
    /// A `SharedStrings` instance containing all extracted strings, or a
    /// `quick_xml::Error` if parsing fails.
    ///
    /// # Errors
    /// Returns `quick_xml::Error` for malformed XML, I/O errors during reading,
    /// or unsupported XML features.
    ///
    /// # Performance
    /// The parser is single‑pass and runs in O(n) time where n is the XML size.
    /// Memory usage is proportional to the number and length of unique strings.
    pub fn load(xml: &[u8]) -> Result<Self, quick_xml::Error> {
        let mut reader = Reader::from_reader(xml);
        let config = reader.config_mut();

        // Preserve all whitespace; Excel shared strings often require exact spaces.
        config.trim_text(false);
        // Skip expensive validation for known‑good Excel output.
        config.check_end_names = false;
        config.expand_empty_elements = false;

        let mut buf = Vec::new();
        let mut strings = Vec::new();
        let mut current = String::new();
        let mut in_si = false;
        let mut in_text = false;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"si" => { in_si = true; current.clear(); }
                    b"t" if in_si => { in_text = true; }
                    _ => {}
                },
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"si" => { in_si = false; strings.push(take(&mut current).into_boxed_str()); }
                    b"t" if in_text => { in_text = false; }
                    _ => {}
                },
                Ok(Event::Text(e)) if in_text => {
                    let decoded = String::from_utf8_lossy(&e);
                    current.push_str(&decoded);
                },
                Ok(Event::Eof) => break,
                Err(e) => return Err(e),
                _ => {}
            }
            
            buf.clear();
        }

        Ok(Self { strings })
    }

    // -------------------------------------------------------------------------
    // Public API
    // -------------------------------------------------------------------------

    /// Returns a reference to the shared string at the given index.
    ///
    /// Shared strings are indexed from 0 in the order they appear in the XML.
    /// This matches the indices used in cell values (e.g., cell `A1` with value
    /// index 5 refers to `shared.get(5)`).
    ///
    /// # Arguments
    /// * `index` – zero‑based position of the string in the shared strings table.
    ///
    /// # Returns
    /// `Some(&str)` if the index is valid, `None` otherwise.
    ///
    /// # Example
    /// ```
    /// # use excel_parser::SharedStrings;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let data = std::fs::read("xl/sharedStrings.xml")?;
    /// let shared = SharedStrings::load(&data)?;
    ///
    /// if let Some(s) = shared.get(0) {
    ///     assert_eq!(s, "First string");
    /// }
    /// assert!(shared.get(9999).is_none());
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn get(&self, index: usize) -> Option<&str> {
        self.strings.get(index).map(|s| &**s)
    }

    /// Returns the total number of shared strings in the table.
    ///
    /// This is the count of `<si>` elements in the source XML, which equals the
    /// maximum valid index plus one.
    ///
    /// # Returns
    /// A `usize` representing the number of unique shared strings.
    ///
    /// # Example
    /// ```
    /// # use excel_parser::SharedStrings;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let data = std::fs::read("xl/sharedStrings.xml")?;
    /// let shared = SharedStrings::load(&data)?;
    ///
    /// println!("The workbook contains {} unique strings", shared.len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Performs a fuzzy search across all shared strings.
    ///
    /// Uses the SkimMatcherV2 algorithm from the `fuzzy-matcher` crate, which
    /// provides score‑based matching similar to fzf (command-line fuzzy finder).
    /// Higher scores indicate better matches. The algorithm:
    /// - Matches characters in order (sequential matching).
    /// - Awards bonus points for consecutive matches and matches at word boundaries.
    /// - Penalizes gaps between matched characters.
    ///
    /// # Scoring
    /// - Exact match: very high score (often 100+).
    /// - Case‑insensitive match: slightly lower than exact.
    /// - Fuzzy match with gaps: lower score proportional to gap length.
    /// - No match: not included in results.
    ///
    /// # Arguments
    /// * `query` – the search pattern (can be exact text or a fuzzy pattern).
    /// * `threshold` – minimum score to include a match. Use:
    ///   - `0` to return all matches.
    ///   - `30-50` for typical fuzzy matches.
    ///   - `100+` for near‑exact matches.
    ///
    /// # Returns
    /// A vector of `(index, score)` tuples, sorted by descending score.
    /// The vector is empty if no strings meet the threshold.
    ///
    /// # Example
    /// ```
    /// # use excel_parser::SharedStrings;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let data = std::fs::read("xl/sharedStrings.xml")?;
    /// let shared = SharedStrings::load(&data)?;
    ///
    /// // Find courses related to "math" (threshold 0 = all matches)
    /// let results = shared.fuzzy_find("math", 0);
    ///
    /// for (idx, score) in results.iter().take(5) {
    ///     if let Some(s) = shared.get(*idx) {
    ///         println!("[{}] {} (score: {})", idx, s, score);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn fuzzy_find(&self, query: &str, threshold: i64) -> Vec<(usize, i64)> {
        let matcher = SkimMatcherV2::default();
        self.fuzzy_find_with_matcher(&matcher, query, threshold)
    }

    /// Performs a fuzzy search using a pre‑configured matcher instance.
    ///
    /// This overload allows reusing a configured `SkimMatcherV2` across multiple
    /// searches, which can be useful when you want to set matcher options once
    /// (e.g., case sensitivity) and reuse it.
    ///
    /// # Arguments
    /// * `matcher` – an instance of `SkimMatcherV2` (implements `FuzzyMatcher`).
    ///               Can be configured before passing (e.g., `SkimMatcherV2::default().case_sensitive(true)`).
    /// * `query` – the search pattern.
    /// * `threshold` – minimum matching score.
    ///
    /// # Returns
    /// A vector of `(index, score)` tuples sorted by descending score.
    ///
    /// # See Also
    /// [`fuzzy_find()`][Self::fuzzy_find] – simpler method using a default matcher.
    pub fn fuzzy_find_with_matcher(
        &self,
        matcher: &SkimMatcherV2,
        query: &str,
        threshold: i64
    ) -> Vec<(usize, i64)> {
        let mut results: Vec<_> = self.strings.iter()
            .enumerate().filter_map(|(i, s)| {
                matcher.fuzzy_match(s, query).map(|score| (i, score))
            }).filter(|(_, score)| *score >= threshold).collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results
    }

    /// Convenience method returning only the indices of matching strings.
    ///
    /// Equivalent to:
    /// ```ignore
    /// self.fuzzy_find(query, threshold)
    ///     .into_iter()
    ///     .map(|(i, _)| i)
    ///     .collect()
    /// ```
    ///
    /// Use this when you only need indices (e.g., for fetching the actual strings
    /// via [`get()`][Self::get]) and don't need the scores.
    ///
    /// # Arguments
    /// * `query` – the search pattern.
    /// * `threshold` – minimum matching score.
    ///
    /// # Returns
    /// A vector of indices whose strings matched the query, sorted by match quality.
    pub fn fuzzy_find_indices(&self, query: &str, threshold: i64) -> Vec<usize> {
        self.fuzzy_find(query, threshold).into_iter()
            .map(|(i, _)| i).collect()
    }
}
