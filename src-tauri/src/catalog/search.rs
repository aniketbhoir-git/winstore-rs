//! Search query model plus the pure merge/dedupe/sort/paginate steps (SPEC §5.3).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::domain::{PackageSummary, SourceKind};

/// Largest page a caller may ask for.
pub const MAX_PAGE_SIZE: u32 = 100;
/// Page size used when the caller does not choose one.
pub const DEFAULT_PAGE_SIZE: u32 = 30;

/// Result ordering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    /// Keep the order the sources returned, in source priority order.
    #[default]
    Relevance,
    Name,
    Rating,
}

/// What the user searched for: text, optional filters, sort and page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchQuery {
    pub text: String,
    pub category: Option<String>,
    /// Restrict the search to one source.
    pub source: Option<SourceKind>,
    pub sort: SortOrder,
    /// Zero-based page index.
    pub page: u32,
    pub page_size: u32,
}

impl Default for SearchQuery {
    fn default() -> Self {
        SearchQuery {
            text: String::new(),
            category: None,
            source: None,
            sort: SortOrder::Relevance,
            page: 0,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        SearchQuery {
            text: text.into(),
            ..SearchQuery::default()
        }
    }

    /// Trimmed, lower-cased text with inner whitespace collapsed; used as the cache key.
    pub fn normalized_text(&self) -> String {
        self.text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    /// Page size clamped to `1..=MAX_PAGE_SIZE`.
    pub fn effective_page_size(&self) -> u32 {
        self.page_size.clamp(1, MAX_PAGE_SIZE)
    }

    /// How many results a source should return so the requested page can be filled.
    ///
    /// Capped at `MAX_PAGE_SIZE`: only the first 100 merged results per source are
    /// reachable, so pages past that point come back short or empty.
    pub fn fetch_limit(&self) -> u32 {
        self.page
            .saturating_add(1)
            .saturating_mul(self.effective_page_size())
            .min(MAX_PAGE_SIZE)
    }
}

/// Cross-source dedupe key: name and publisher, lower-cased, letters and digits only.
/// `None` when the publisher is empty (a winget search row has none): such a row is never
/// dropped by the name rule.
pub fn dedupe_key(summary: &PackageSummary) -> Option<String> {
    let norm = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let publisher = norm(&summary.publisher);
    if publisher.is_empty() {
        return None;
    }
    Some(format!("{}|{}", norm(&summary.name), publisher))
}

/// Concatenates per-source results in the given (priority) order and drops later duplicates.
///
/// Inside one source a row is a duplicate only when `(source, id)` repeats. Across sources a
/// row is a duplicate when an earlier source has the same `dedupe_key`.
pub fn merge(per_source: Vec<Vec<PackageSummary>>) -> Vec<PackageSummary> {
    let mut seen_ids = HashSet::new();
    let mut seen_keys: HashMap<String, SourceKind> = HashMap::new();
    per_source
        .into_iter()
        .flatten()
        .filter(|s| {
            if !seen_ids.insert((s.source, s.id.clone())) {
                return false;
            }
            let Some(key) = dedupe_key(s) else {
                return true;
            };
            match seen_keys.get(&key) {
                Some(first) => *first == s.source,
                None => {
                    seen_keys.insert(key, s.source);
                    true
                }
            }
        })
        .collect()
}

/// Sorts in place. `Relevance` keeps the merged order.
pub fn sort(results: &mut [PackageSummary], order: SortOrder) {
    match order {
        SortOrder::Relevance => {}
        SortOrder::Name => results.sort_by_cached_key(|s| s.name.to_lowercase()),
        SortOrder::Rating => results.sort_by(|a, b| {
            let ra = a.rating.unwrap_or(f32::NEG_INFINITY);
            let rb = b.rating.unwrap_or(f32::NEG_INFINITY);
            rb.total_cmp(&ra)
        }),
    }
}

/// Returns the requested page; an out-of-range page is empty.
pub fn paginate(results: Vec<PackageSummary>, query: &SearchQuery) -> Vec<PackageSummary> {
    let size = query.effective_page_size() as usize;
    let start = (query.page as usize).saturating_mul(size);
    results.into_iter().skip(start).take(size).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::package::fixtures::summary;

    fn named(src: SourceKind, id: &str, name: &str, publisher: &str) -> PackageSummary {
        let mut s = summary(src, id, name);
        s.publisher = publisher.into();
        s
    }

    #[test]
    fn normalized_text_collapses_whitespace_and_case() {
        assert_eq!(
            SearchQuery::new("  Fire   FOX \t").normalized_text(),
            "fire fox"
        );
        assert_eq!(SearchQuery::new("   ").normalized_text(), "");
    }

    #[test]
    fn page_size_and_fetch_limit_are_clamped() {
        let mut q = SearchQuery::new("x");
        assert_eq!(q.effective_page_size(), DEFAULT_PAGE_SIZE);
        q.page_size = 0;
        assert_eq!(q.effective_page_size(), 1);
        q.page_size = 1000;
        assert_eq!(q.effective_page_size(), MAX_PAGE_SIZE);
        q.page_size = 10;
        q.page = 2;
        assert_eq!(q.fetch_limit(), 30);
        q.page = u32::MAX;
        assert_eq!(q.fetch_limit(), MAX_PAGE_SIZE);
    }

    #[test]
    fn query_serde_defaults() {
        let q: SearchQuery = serde_json::from_str(r#"{"text":"vlc","sort":"name"}"#).unwrap();
        assert_eq!(q.text, "vlc");
        assert_eq!(q.sort, SortOrder::Name);
        assert_eq!(q.page_size, DEFAULT_PAGE_SIZE);
        assert!(q.source.is_none());
    }

    #[test]
    fn merge_dedupes_by_name_and_publisher_keeping_first() {
        let a = named(
            SourceKind::WingetCommunity,
            "Mozilla.Firefox",
            "Firefox",
            "Mozilla",
        );
        let dup = named(SourceKind::Scoop, "firefox", "fire-fox", "MOZILLA");
        let other = named(SourceKind::Scoop, "ff", "Firefox", "Someone Else");
        let merged = merge(vec![vec![a.clone()], vec![dup, other.clone()]]);
        assert_eq!(merged, vec![a, other]);
    }

    #[test]
    fn merge_keeps_same_name_rows_with_empty_publisher() {
        let a = named(
            SourceKind::WingetCommunity,
            "Acme.Notes",
            "Sample Notes",
            "",
        );
        let b = named(
            SourceKind::WingetCommunity,
            "Beta.Notes",
            "Sample Notes",
            "",
        );
        assert_eq!(dedupe_key(&a), None);
        let merged = merge(vec![vec![a.clone(), b.clone()]]);
        assert_eq!(merged, vec![a, b]);
    }

    #[test]
    fn merge_drops_a_repeated_id_inside_one_source() {
        let a = named(SourceKind::WingetCommunity, "Acme.Notes", "Notes", "");
        let merged = merge(vec![vec![a.clone(), a.clone()]]);
        assert_eq!(merged, vec![a]);
    }

    #[test]
    fn merge_applies_the_name_rule_across_sources_only_with_a_publisher() {
        let a = named(SourceKind::WingetCommunity, "Acme.Notes", "Notes", "Acme");
        let same = named(SourceKind::Scoop, "notes", "Notes", "ACME");
        let no_pub = named(SourceKind::Scoop, "notes2", "Notes", "");
        let merged = merge(vec![vec![a.clone()], vec![same, no_pub.clone()]]);
        assert_eq!(merged, vec![a, no_pub]);
        // Two rows of one source with the same name and a publisher both stay.
        let x = named(SourceKind::Scoop, "x", "Tool", "Pub");
        let y = named(SourceKind::Scoop, "y", "Tool", "Pub");
        assert_eq!(merge(vec![vec![x.clone(), y.clone()]]), vec![x, y]);
    }

    #[test]
    fn sort_by_name_and_rating() {
        let mut b = named(SourceKind::Scoop, "b", "beta", "p");
        b.rating = None;
        let mut a = named(SourceKind::Scoop, "a", "Alpha", "p");
        a.rating = Some(3.0);
        let mut c = named(SourceKind::Scoop, "c", "Gamma", "p");
        c.rating = Some(4.5);
        let mut v = vec![b.clone(), c.clone(), a.clone()];
        sort(&mut v, SortOrder::Relevance);
        assert_eq!(v, vec![b.clone(), c.clone(), a.clone()]);
        sort(&mut v, SortOrder::Name);
        assert_eq!(v, vec![a.clone(), b.clone(), c.clone()]);
        sort(&mut v, SortOrder::Rating);
        assert_eq!(v, vec![c, a, b]);
    }

    #[test]
    fn paginate_pages_and_out_of_range() {
        let all: Vec<_> = (0..5)
            .map(|i| named(SourceKind::Scoop, &i.to_string(), &format!("n{i}"), "p"))
            .collect();
        let mut q = SearchQuery::new("n");
        q.page_size = 2;
        q.page = 1;
        let page = paginate(all.clone(), &q);
        assert_eq!(page, all[2..4].to_vec());
        q.page = 2;
        assert_eq!(paginate(all.clone(), &q).len(), 1);
        q.page = 9;
        assert!(paginate(all, &q).is_empty());
    }
}
