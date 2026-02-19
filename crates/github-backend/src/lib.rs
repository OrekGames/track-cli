pub mod client;
mod convert;
pub mod error;
pub mod models;
mod trait_impl;
mod wiki;


#[cfg(test)]
mod client_tests;
#[cfg(test)]
mod wiki_tests;

pub use wiki::WikiManager;
pub use client::GitHubClient;
pub use error::{GitHubError, Result};
pub use models::*;

// Re-export tracker-core types for convenience
pub use tracker_core::{IssueTracker, TrackerError};
