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

/// Fetch all pages of an offset-paged resource, deduplicating items by `key`
/// and failing loudly when pagination stalls.
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
/// `max_results` is passed explicitly — callers typically pass
/// [`get_max_results`].
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

/// Pages [`fetch_pages_to_end`] requests before giving up on a walk that never
/// ends: a million items at 100 a page, far beyond any real collection.
const MAX_PAGES: usize = 10_000;

/// One page of a backend's native paging, as [`fetch_pages_to_end`] and
/// [`fetch_window`] take it.
pub struct Page<T> {
    pub items: Vec<T>,
    /// No page follows this one, so the walk requests no more.
    pub last: bool,
}

/// Fetch pages until one is marked [`Page::last`] and return all their items
/// in order.
///
/// Nothing is capped, deduplicated or dropped: this is for collections that
/// must come back whole (labels, members, history). The caller owns its page
/// number, offset or cursor and says which page is the last, since APIs signal
/// it differently (a short page, `isLast`, a total). A walk still going after
/// 10,000 pages fails with [`TrackerError::PaginationStalled`] ("{what} did not
/// terminate after 10000 pages") instead of looping forever on a server that
/// ignores the paging parameters. An error from `fetch_page` is returned as is
/// and the pages already fetched are dropped.
pub fn fetch_pages_to_end<T, F>(what: &str, mut fetch_page: F) -> Result<Vec<T>>
where
    F: FnMut() -> Result<Page<T>>,
{
    let mut items = Vec::new();
    for _ in 0..MAX_PAGES {
        let page = fetch_page()?;
        items.extend(page.items);
        if page.last {
            return Ok(items);
        }
    }
    Err(TrackerError::PaginationStalled(format!(
        "{what} did not terminate after {MAX_PAGES} pages"
    )))
}

/// Collect `limit` items starting `skip` items in, by fetching native pages in
/// order: an offset window over an API paged by number or cursor.
///
/// `fetch_page` fetches the next page; its argument is how many more items the
/// window wants, counting the ones still to skip, for APIs that size each
/// request. The first `skip` items are dropped (whole pages at a time where
/// possible), then items are kept until there are `limit`. The walk stops when
/// the window is full, without another request (so `limit == 0` fetches
/// nothing), or after a page marked [`Page::last`]. There is no dedup and no
/// stall detection, so `last` must end a walk whose pages stop coming.
pub fn fetch_window<T, F>(limit: usize, skip: usize, mut fetch_page: F) -> Result<Vec<T>>
where
    F: FnMut(usize) -> Result<Page<T>>,
{
    let mut items = Vec::new();
    let mut remaining_skip = skip;

    while items.len() < limit {
        let need = limit - items.len();
        let page = fetch_page(remaining_skip.saturating_add(need))?;

        if remaining_skip >= page.items.len() {
            remaining_skip -= page.items.len();
        } else {
            items.extend(page.items.into_iter().skip(remaining_skip).take(need));
            remaining_skip = 0;
        }

        if page.last {
            break;
        }
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn fetch_pages_to_end_errors_after_max_pages() {
        // Arrange: a server that ignores the paging parameters never sends a
        // last page
        let mut calls = 0;
        let err = fetch_pages_to_end("'/labels'", || {
            calls += 1;
            Ok(Page {
                items: vec![calls],
                last: false,
            })
        })
        .unwrap_err();
        // Assert: the backstop gives up after MAX_PAGES requests, naming the walk
        assert_eq!(calls, 10_000);
        assert_eq!(
            err.to_string(),
            "Pagination stalled: '/labels' did not terminate after 10000 pages"
        );
    }

    #[test]
    fn fetch_window_skips_across_pages_and_stops_when_full_or_last() {
        // Arrange: four native pages, only the final one marked last
        let mut pages = vec![
            Page {
                items: vec![1, 2, 3],
                last: false,
            },
            Page {
                items: vec![4, 5, 6],
                last: false,
            },
            Page {
                items: vec![7, 8, 9],
                last: false,
            },
            Page {
                items: vec![10],
                last: true,
            },
        ]
        .into_iter();
        let mut wanted = Vec::new();
        // Act: skip a whole page and one item of the next, then take 3
        let result = fetch_window(3, 4, |want| {
            wanted.push(want);
            Ok(pages.next().expect("asked for a page past the last one"))
        })
        .unwrap();
        // Assert: each request asks for the skip still pending plus the items
        // still needed, and the walk stops once full, before the 4th page
        assert_eq!(result, vec![5, 6, 7]);
        assert_eq!(wanted, vec![7, 4, 1]);

        // A last page ends the walk even when the skip uses it up, as it does
        // every empty page: a skip past the end returns nothing
        let mut pages = vec![
            Page {
                items: vec![1, 2, 3],
                last: false,
            },
            Page {
                items: vec![4],
                last: true,
            },
        ]
        .into_iter();
        let mut wanted = Vec::new();
        let past_end = fetch_window(2, 5, |want| {
            wanted.push(want);
            Ok(pages.next().expect("asked for a page past the last one"))
        })
        .unwrap();
        assert!(past_end.is_empty());
        assert_eq!(wanted, vec![7, 4]);

        // An empty window fetches nothing, whatever the skip
        let empty: Vec<i32> = fetch_window(0, 5, |_| panic!("limit 0 must not fetch")).unwrap();
        assert!(empty.is_empty());
    }
}
