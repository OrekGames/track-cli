pub mod client;
mod convert;
pub mod models;
mod trait_impl;
pub(crate) mod wiki;

#[cfg(test)]
mod client_tests;
#[cfg(test)]
mod convert_tests;
#[cfg(test)]
mod wiki_tests;

pub use client::{GitHubClient, is_rate_limit_error};
pub use models::*;
pub use wiki::WikiManager;

// Re-export tracker-core types for convenience
pub use tracker_core::{IssueTracker, TrackerError};
