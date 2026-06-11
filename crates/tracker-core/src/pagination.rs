use crate::error::{Result, TrackerError};
use std::collections::HashSet;
use std::env;

pub const DEFAULT_MAX_RESULTS: usize = 1000;

/// Get the maximum results limit from environment variable or default
pub fn get_max_results() -> usize {
    env::var("TRACK_MAX_RESULTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_RESULTS)
}

/// Helper function to fetch all pages of a resource
pub fn fetch_all_pages<T, F>(mut fetch_page: F, page_size: usize) -> Result<Vec<T>>
where
    F: FnMut(usize, usize) -> Result<Vec<T>>,
{
    let max_results = get_max_results();
    let mut all_results = Vec::new();
    let mut current_offset = 0;

    loop {
        let remaining = max_results.saturating_sub(all_results.len());
        if remaining == 0 {
            break;
        }

        let limit = std::cmp::min(page_size, remaining);
        let page_results = fetch_page(current_offset, limit)?;
        let page_len = page_results.len();

        if page_len == 0 {
            break;
        }

        all_results.extend(page_results);

        if all_results.len() >= max_results || page_len < limit {
            break;
        }

        current_offset += page_len;
    }

    Ok(all_results)
}

/// Like [`fetch_all_pages`], but deduplicates items by `key` and fails loudly
/// when pagination stalls.
///
/// Pages that partially overlap the results collected so far (e.g. rows
/// shifting between requests under concurrent writes) are deduplicated and
/// the loop continues. A *full* page that contributes zero new keys means
/// the backend's pagination is not advancing (e.g. an offset parameter the
/// server ignores); returning the partial set would silently truncate the
/// results, so this returns [`TrackerError::PaginationStalled`] instead.
/// (A partial page that adds nothing new is a legitimate end of results —
/// the offset-paging contract already treats partial pages as final.)
///
/// Unlike `fetch_all_pages`, `max_results` is passed explicitly — callers
/// typically pass [`get_max_results`].
pub fn fetch_all_pages_keyed<T, F, K>(
    mut fetch_page: F,
    page_size: usize,
    max_results: usize,
    mut key: K,
) -> Result<Vec<T>>
where
    F: FnMut(usize, usize) -> Result<Vec<T>>,
    K: FnMut(&T) -> String,
{
    let mut seen = HashSet::new();
    let mut all_results: Vec<T> = Vec::new();
    let mut current_offset = 0;

    loop {
        let remaining = max_results.saturating_sub(all_results.len());
        if remaining == 0 {
            break;
        }

        let limit = std::cmp::min(page_size, remaining);
        let page_results = fetch_page(current_offset, limit)?;
        let page_len = page_results.len();

        if page_len == 0 {
            break;
        }

        let before = all_results.len();
        for item in page_results {
            if seen.insert(key(&item)) {
                all_results.push(item);
            }
        }

        // Partial page = legitimate last page under the offset contract,
        // even if everything on it was already seen.
        if page_len < limit {
            break;
        }
        if all_results.len() == before {
            return Err(TrackerError::PaginationStalled(
                "page returned no new results; backend pagination may be broken".to_string(),
            ));
        }

        // Offset is a server-side row position: advance by the raw page
        // length, not the unique count.
        current_offset += page_len;
    }

    Ok(all_results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_all_pages_single_page() {
        // Arrange: only 3 items, smaller than page size
        let data = [1, 2, 3];
        // Act
        let result = fetch_all_pages(
            |offset, limit| Ok(data[offset..].iter().copied().take(limit).collect()),
            10,
        )
        .unwrap();
        // Assert
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn fetch_all_pages_multiple_pages() {
        // Arrange: 25 items with page size 10
        let data: Vec<i32> = (1..=25).collect();
        // Act
        let result = fetch_all_pages(
            |offset, limit| Ok(data[offset..].iter().copied().take(limit).collect()),
            10,
        )
        .unwrap();
        // Assert
        assert_eq!(result.len(), 25);
        assert_eq!(result, data);
    }

    #[test]
    fn fetch_all_pages_empty_first_page() {
        // Arrange: no data
        let result: crate::error::Result<Vec<i32>> = fetch_all_pages(|_, _| Ok(vec![]), 10);
        // Assert
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn fetch_all_pages_respects_max_results() {
        // Arrange: 2000 items but max_results defaults to 1000
        let data: Vec<i32> = (1..=2000).collect();
        // Act: TRACK_MAX_RESULTS is not set, so default 1000 applies
        let result = fetch_all_pages(
            |offset, limit| Ok(data[offset..].iter().copied().take(limit).collect()),
            100,
        )
        .unwrap();
        // Assert: capped at DEFAULT_MAX_RESULTS (1000)
        assert_eq!(result.len(), DEFAULT_MAX_RESULTS);
    }

    #[test]
    fn fetch_all_pages_stops_on_partial_page() {
        // Arrange: 15 items, page size 10 — second page returns 5 (< page_size)
        let data: Vec<i32> = (1..=15).collect();
        // Act
        let result = fetch_all_pages(
            |offset, limit| Ok(data[offset..].iter().copied().take(limit).collect()),
            10,
        )
        .unwrap();
        // Assert: stops after second page (partial)
        assert_eq!(result.len(), 15);
    }

    #[test]
    fn fetch_all_pages_propagates_error() {
        // Arrange: fail on second page
        let mut call_count = 0;
        let result: crate::error::Result<Vec<i32>> = fetch_all_pages(
            |_, _| {
                call_count += 1;
                if call_count == 1 {
                    Ok(vec![1, 2, 3, 4, 5])
                } else {
                    Err(crate::error::TrackerError::Api {
                        status: 500,
                        message: "Server error".to_string(),
                    })
                }
            },
            5,
        );
        // Assert: error propagated
        assert!(result.is_err());
    }

    #[test]
    fn fetch_all_pages_keyed_collects_unique_across_pages() {
        // Arrange: 25 distinct items, page size 10
        let data: Vec<i32> = (1..=25).collect();
        // Act
        let result = fetch_all_pages_keyed(
            |offset, limit| Ok(data[offset..].iter().copied().take(limit).collect()),
            10,
            1000,
            |item| item.to_string(),
        )
        .unwrap();
        // Assert: all items, in order, no duplicates
        assert_eq!(result, data);
    }

    #[test]
    fn fetch_all_pages_keyed_dedups_overlapping_pages() {
        // Arrange: each page re-includes the previous page's last 2 items
        // (rows shifting under concurrent writes)
        let data: Vec<i32> = (1..=20).collect();
        let result = fetch_all_pages_keyed(
            |offset, limit| {
                let start = offset.saturating_sub(2);
                Ok(data[start..].iter().copied().take(limit).collect())
            },
            10,
            1000,
            |item| item.to_string(),
        )
        .unwrap();
        // Assert: overlap deduplicated, loop continued while progress was made
        assert_eq!(result, data);
    }

    #[test]
    fn fetch_all_pages_keyed_errors_when_page_has_no_new_keys() {
        // Arrange: server ignores the offset and always returns the same page
        // (the broken-pagination failure mode from issue #252)
        let mut call_count = 0;
        let result = fetch_all_pages_keyed(
            |_, limit| {
                call_count += 1;
                Ok((1..=limit as i32).collect())
            },
            10,
            1000,
            |item: &i32| item.to_string(),
        );
        // Assert: fails loudly instead of returning truncated data
        assert!(matches!(result, Err(TrackerError::PaginationStalled(_))));
        assert_eq!(call_count, 2);
    }

    #[test]
    fn fetch_all_pages_keyed_accepts_duplicate_partial_final_page() {
        // Arrange: the final page is partial AND consists entirely of
        // already-seen rows (rows shifted back between requests). That is a
        // legitimate end of results, not a stall.
        let result = fetch_all_pages_keyed(
            |offset, _| {
                if offset == 0 {
                    Ok((1..=10).collect())
                } else {
                    Ok((6..=10).collect())
                }
            },
            10,
            1000,
            |item: &i32| item.to_string(),
        )
        .unwrap();
        assert_eq!(result, (1..=10).collect::<Vec<i32>>());
    }

    #[test]
    fn fetch_all_pages_keyed_respects_max_results_param() {
        // Arrange: plenty of data but cap at 15
        let data: Vec<i32> = (1..=100).collect();
        let mut limits = Vec::new();
        let result = fetch_all_pages_keyed(
            |offset, limit| {
                limits.push(limit);
                Ok(data[offset..].iter().copied().take(limit).collect())
            },
            10,
            15,
            |item| item.to_string(),
        )
        .unwrap();
        // Assert: capped at 15; second request shrank to the remainder
        assert_eq!(result.len(), 15);
        assert_eq!(limits, vec![10, 5]);
    }

    #[test]
    fn fetch_all_pages_keyed_stops_on_partial_page() {
        // Arrange: 15 items, page size 10 — second page returns 5 (< limit)
        let data: Vec<i32> = (1..=15).collect();
        let result = fetch_all_pages_keyed(
            |offset, limit| Ok(data[offset..].iter().copied().take(limit).collect()),
            10,
            1000,
            |item| item.to_string(),
        )
        .unwrap();
        assert_eq!(result.len(), 15);
    }

    #[test]
    fn fetch_all_pages_keyed_propagates_error() {
        let mut call_count = 0;
        let result: crate::error::Result<Vec<i32>> = fetch_all_pages_keyed(
            |_, _| {
                call_count += 1;
                if call_count == 1 {
                    Ok(vec![1, 2, 3, 4, 5])
                } else {
                    Err(crate::error::TrackerError::Api {
                        status: 500,
                        message: "Server error".to_string(),
                    })
                }
            },
            5,
            1000,
            |item| item.to_string(),
        );
        assert!(result.is_err());
    }
}
