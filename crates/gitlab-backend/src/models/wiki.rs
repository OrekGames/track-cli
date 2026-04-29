use serde::{Deserialize, Serialize};

/// GitLab project wiki page.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabWikiPage {
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub encoding: Option<String>,
}

/// Request to create a GitLab wiki page.
#[derive(Debug, Clone, Serialize)]
pub struct CreateGitLabWikiPage {
    pub title: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// Request to update a GitLab wiki page.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateGitLabWikiPage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// GitLab wiki attachment upload response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabWikiAttachment {
    pub file_name: String,
    pub file_path: String,
    #[serde(default)]
    pub branch: Option<String>,
    pub link: GitLabWikiAttachmentLink,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabWikiAttachmentLink {
    pub url: String,
    pub markdown: String,
}
