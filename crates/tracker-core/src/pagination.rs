use crate::error::Result;
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
}
