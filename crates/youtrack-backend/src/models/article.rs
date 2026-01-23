use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{CommentAuthor, ProjectIdentifier, ProjectRef, Tag, TagIdentifier};

/// YouTrack Knowledge Base article
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    pub id: String,
    #[serde(default)]
    pub id_readable: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub project: Option<ProjectRef>,
    #[serde(default)]
    pub parent_article: Option<ArticleRef>,
    #[serde(default)]
    pub has_children: Option<bool>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default, with = "chrono::serde::ts_milliseconds_option")]
    pub created: Option<DateTime<Utc>>,
    #[serde(default, with = "chrono::serde::ts_milliseconds_option")]
    pub updated: Option<DateTime<Utc>>,
    #[serde(default)]
    pub reporter: Option<CommentAuthor>,
}

/// Reference to an article (minimal fields)
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArticleRef {
    pub id: String,
    #[serde(default)]
    pub id_readable: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

/// Data for creating a new article
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateArticle {
    pub project: ProjectIdentifier,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_article: Option<ArticleIdentifier>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagIdentifier>,
}

/// Data for updating an article
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateArticle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagIdentifier>,
}

/// Identifier for referencing an article
#[derive(Debug, Serialize)]
pub struct ArticleIdentifier {
    pub id: String,
}

/// Article attachment
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArticleAttachment {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, with = "chrono::serde::ts_milliseconds_option")]
    pub created: Option<DateTime<Utc>>,
    #[serde(default)]
    pub author: Option<CommentAuthor>,
}

/// Article comment (same structure as issue comment)
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArticleComment {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub author: Option<CommentAuthor>,
    #[serde(default, with = "chrono::serde::ts_milliseconds_option")]
    pub created: Option<DateTime<Utc>>,
}

/// Create a comment on an article
#[derive(Debug, Serialize)]
pub struct CreateArticleComment {
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_deserializes_from_youtrack_response() {
        let json = r##"{
            "id": "226-0",
            "idReadable": "PROJ-A-1",
            "summary": "Getting Started",
            "content": "Welcome to the guide",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "hasChildren": true,
            "tags": [],
            "created": 1640000000000,
            "updated": 1640000000000
        }"##;

        let article: Article = serde_json::from_str(json).unwrap();
        assert_eq!(article.id, "226-0");
        assert_eq!(article.id_readable, Some("PROJ-A-1".to_string()));
        assert_eq!(article.summary, "Getting Started");
        assert_eq!(article.has_children, Some(true));
    }

    #[test]
    fn create_article_serializes_correctly() {
        let create = CreateArticle {
            project: ProjectIdentifier {
                id: "0-1".to_string(),
            },
            summary: "New Article".to_string(),
            content: Some("Article content".to_string()),
            parent_article: None,
            tags: vec![],
        };

        let json = serde_json::to_string(&create).unwrap();
        assert!(json.contains("\"summary\":\"New Article\""));
        assert!(json.contains("\"content\":\"Article content\""));
        assert!(!json.contains("parentArticle"));
        assert!(!json.contains("tags"));
    }

    #[test]
    fn create_article_with_parent_serializes_correctly() {
        let create = CreateArticle {
            project: ProjectIdentifier {
                id: "0-1".to_string(),
            },
            summary: "Child Article".to_string(),
            content: None,
            parent_article: Some(ArticleIdentifier {
                id: "226-0".to_string(),
            }),
            tags: vec![],
        };

        let json = serde_json::to_string(&create).unwrap();
        assert!(json.contains("parentArticle"));
        assert!(json.contains("\"id\":\"226-0\""));
    }

    #[test]
    fn update_article_omits_none_fields() {
        let update = UpdateArticle {
            summary: Some("Updated Title".to_string()),
            content: None,
            tags: vec![],
        };

        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("\"summary\":\"Updated Title\""));
        assert!(!json.contains("content"));
        assert!(!json.contains("tags"));
    }

    #[test]
    fn article_attachment_deserializes() {
        let json = r#"{
            "id": "att-1",
            "name": "image.png",
            "size": 12345,
            "mimeType": "image/png",
            "url": "/api/files/att-1",
            "created": 1640000000000
        }"#;

        let attachment: ArticleAttachment = serde_json::from_str(json).unwrap();
        assert_eq!(attachment.id, "att-1");
        assert_eq!(attachment.name, "image.png");
        assert_eq!(attachment.size, 12345);
        assert_eq!(attachment.mime_type, Some("image/png".to_string()));
    }
}
