use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct TransitionsResponse {
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Deserialize)]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub to: TransitionTarget,
    #[serde(default)]
    pub is_available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionTarget {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub status_category: Option<StatusCategory>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusCategory {
    pub key: String, // "new" | "indeterminate" | "done"
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct TransitionRequest {
    pub transition: TransitionId,
}

#[derive(Debug, Serialize)]
pub struct TransitionId {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectIssueTypeStatuses {
    pub id: String,   // issue type id
    pub name: String, // issue type name
    pub statuses: Vec<ProjectStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub id: String,
    pub name: String,
    pub status_category: Option<StatusCategory>,
}
