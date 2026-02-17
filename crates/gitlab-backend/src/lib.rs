pub mod client;
mod convert;
pub mod error;
pub mod models;
mod trait_impl;

#[cfg(test)]
mod client_tests;

pub use client::GitLabClient;
pub use error::{GitLabError, Result};
pub use models::*;

pub use tracker_core::{IssueTracker, KnowledgeBase, TrackerError};
