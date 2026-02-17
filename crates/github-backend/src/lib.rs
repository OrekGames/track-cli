pub mod client;
mod convert;
pub mod error;
pub mod models;
mod trait_impl;

#[cfg(test)]
mod client_tests;

pub use client::GitHubClient;
pub use error::{GitHubError, Result};
pub use models::*;

// Re-export tracker-core types for convenience
pub use tracker_core::{IssueTracker, TrackerError};
