pub mod client;
pub mod confluence;
mod confluence_impl;
mod convert;
pub mod error;
pub mod models;
mod trait_impl;

#[cfg(test)]
mod client_tests;

pub use client::JiraClient;
pub use confluence::ConfluenceClient;
pub use error::{JiraError, Result};
pub use models::*;

// Re-export tracker-core types for convenience
pub use tracker_core::{IssueTracker, KnowledgeBase, TrackerError};
