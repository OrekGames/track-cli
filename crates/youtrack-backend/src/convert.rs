//! Conversion functions between YouTrack API models and tracker-core models

use crate::models as yt;
use tracker_core::models as core;

/// Convert YouTrack Issue to tracker-core Issue
impl From<yt::Issue> for core::Issue {
    fn from(issue: yt::Issue) -> Self {
        Self {
            id: issue.id,
            id_readable: issue.id_readable,
            summary: issue.summary,
            description: issue.description,
            project: core::ProjectRef {
                id: issue.project.id,
                name: issue.project.name,
                short_name: issue.project.short_name,
            },
            custom_fields: issue.custom_fields.into_iter().map(Into::into).collect(),
            tags: issue.tags.into_iter().map(Into::into).collect(),
            created: issue.created,
            updated: issue.updated,
        }
    }
}

/// Convert YouTrack CustomField to tracker-core CustomField
impl From<yt::CustomField> for core::CustomField {
    fn from(field: yt::CustomField) -> Self {
        match field {
            yt::CustomField::SingleEnum { name, value } => core::CustomField::SingleEnum {
                name,
                value: value.map(|v| v.name),
            },
            yt::CustomField::State { name, value } => core::CustomField::State {
                name,
                value: value.as_ref().map(|v| v.name.clone()),
                is_resolved: value.map(|v| v.is_resolved).unwrap_or(false),
            },
            yt::CustomField::SingleUser { name, value } => core::CustomField::SingleUser {
                name,
                login: value.as_ref().map(|v| v.login.clone()),
                display_name: value.and_then(|v| v.name),
            },
            yt::CustomField::Text { name, value } => core::CustomField::Text {
                name,
                value: value.map(|v| v.text),
            },
            yt::CustomField::Unknown => core::CustomField::Unknown {
                name: "Unknown".to_string(),
            },
        }
    }
}

/// Convert YouTrack Tag to tracker-core Tag
impl From<yt::Tag> for core::Tag {
    fn from(tag: yt::Tag) -> Self {
        Self {
            id: tag.id,
            name: tag.name,
        }
    }
}

/// Convert YouTrack Project to tracker-core Project
impl From<yt::Project> for core::Project {
    fn from(project: yt::Project) -> Self {
        Self {
            id: project.id,
            name: project.name,
            short_name: project.short_name,
            description: project.description,
        }
    }
}

/// Convert YouTrack ProjectCustomFieldExt to tracker-core ProjectCustomField (with values)
impl From<yt::ProjectCustomFieldExt> for core::ProjectCustomField {
    fn from(field: yt::ProjectCustomFieldExt) -> Self {
        let field_type = field
            .field
            .field_type
            .as_ref()
            .and_then(|ft| ft.presentation.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Check if this is a state field (field_type starts with "state")
        let is_state_field = field_type.starts_with("state");

        // Extract enum values and state values from bundle if available
        let (values, state_values) = match field.bundle {
            Some(bundle) => {
                let values: Vec<String> = bundle.values.iter().map(|v| v.name.clone()).collect();

                // For state fields, also extract workflow metadata
                let state_values: Vec<core::StateValueInfo> = if is_state_field {
                    bundle
                        .values
                        .iter()
                        .map(|v| core::StateValueInfo {
                            name: v.name.clone(),
                            is_resolved: v.is_resolved.unwrap_or(false),
                            ordinal: v.ordinal.unwrap_or(0),
                        })
                        .collect()
                } else {
                    vec![]
                };

                (values, state_values)
            }
            None => (vec![], vec![]),
        };

        Self {
            id: field.id,
            name: field.field.name,
            field_type,
            required: !field.can_be_empty,
            values,
            state_values,
        }
    }
}

/// Convert YouTrack User to tracker-core User
impl From<yt::User> for core::User {
    fn from(user: yt::User) -> Self {
        Self {
            id: user.id,
            login: Some(user.login),
            display_name: user.full_name.unwrap_or_else(|| "Unknown".to_string()),
        }
    }
}

/// Convert YouTrack IssueLinkType to tracker-core IssueLinkType
impl From<yt::IssueLinkType> for core::IssueLinkType {
    fn from(lt: yt::IssueLinkType) -> Self {
        Self {
            id: lt.id,
            name: lt.name,
            source_to_target: lt.source_to_target,
            target_to_source: lt.target_to_source,
            directed: lt.directed,
        }
    }
}

/// Convert YouTrack IssueTag to tracker-core IssueTag
impl From<yt::IssueTag> for core::IssueTag {
    fn from(tag: yt::IssueTag) -> Self {
        Self {
            id: tag.id,
            name: tag.name,
            color: tag.color.map(|c| core::TagColor {
                id: c.id,
                background: c.background,
                foreground: c.foreground,
            }),
            issues_count: tag.issues_count,
        }
    }
}

/// Convert YouTrack IssueLink to tracker-core IssueLink
impl From<yt::IssueLink> for core::IssueLink {
    fn from(link: yt::IssueLink) -> Self {
        Self {
            id: link.id,
            direction: link.direction,
            link_type: core::IssueLinkType {
                id: link.link_type.id,
                name: link.link_type.name,
                source_to_target: link.link_type.source_to_target,
                target_to_source: link.link_type.target_to_source,
                directed: link.link_type.directed,
            },
            issues: link.issues.into_iter().map(Into::into).collect(),
        }
    }
}

/// Convert YouTrack LinkedIssue to tracker-core LinkedIssue
impl From<yt::LinkedIssue> for core::LinkedIssue {
    fn from(issue: yt::LinkedIssue) -> Self {
        Self {
            id: issue.id,
            id_readable: issue.id_readable,
            summary: issue.summary,
        }
    }
}

/// Convert YouTrack IssueComment to tracker-core Comment
impl From<yt::IssueComment> for core::Comment {
    fn from(comment: yt::IssueComment) -> Self {
        Self {
            id: comment.id,
            text: comment.text,
            author: comment.author.map(|a| core::CommentAuthor {
                login: a.login,
                name: a.name,
            }),
            created: comment.created,
        }
    }
}

/// Convert tracker-core CreateIssue to YouTrack CreateIssue
impl From<&core::CreateIssue> for yt::CreateIssue {
    fn from(create: &core::CreateIssue) -> Self {
        Self {
            project: yt::ProjectIdentifier {
                id: create.project_id.clone(),
            },
            summary: create.summary.clone(),
            description: create.description.clone(),
            custom_fields: create.custom_fields.iter().map(Into::into).collect(),
            tags: create
                .tags
                .iter()
                .map(|name| yt::TagIdentifier::from_name(name.clone()))
                .collect(),
        }
    }
}

/// Convert tracker-core UpdateIssue to YouTrack UpdateIssue
impl From<&core::UpdateIssue> for yt::UpdateIssue {
    fn from(update: &core::UpdateIssue) -> Self {
        Self {
            summary: update.summary.clone(),
            description: update.description.clone(),
            custom_fields: update.custom_fields.iter().map(Into::into).collect(),
            tags: update
                .tags
                .iter()
                .map(|name| yt::TagIdentifier::from_name(name.clone()))
                .collect(),
        }
    }
}

/// Convert tracker-core CustomFieldUpdate to YouTrack CustomFieldUpdate
impl From<&core::CustomFieldUpdate> for yt::CustomFieldUpdate {
    fn from(update: &core::CustomFieldUpdate) -> Self {
        match update {
            core::CustomFieldUpdate::SingleEnum { name, value } => {
                yt::CustomFieldUpdate::SingleEnum {
                    name: name.clone(),
                    value: Some(yt::EnumValueInput {
                        name: value.clone(),
                    }),
                }
            }
            core::CustomFieldUpdate::MultiEnum { name, values } => {
                yt::CustomFieldUpdate::MultiEnum {
                    name: name.clone(),
                    value: values
                        .iter()
                        .map(|v| yt::EnumValueInput { name: v.clone() })
                        .collect(),
                }
            }
            core::CustomFieldUpdate::State { name, value } => yt::CustomFieldUpdate::State {
                name: name.clone(),
                value: Some(yt::StateValueInput {
                    name: value.clone(),
                }),
            },
            core::CustomFieldUpdate::SingleUser { name, login } => {
                yt::CustomFieldUpdate::SingleUser {
                    name: name.clone(),
                    value: Some(yt::UserValueInput {
                        login: login.clone(),
                    }),
                }
            }
        }
    }
}

// ============================================================================
// Article Conversions
// ============================================================================

/// Convert YouTrack Article to tracker-core Article
impl From<yt::Article> for core::Article {
    fn from(article: yt::Article) -> Self {
        Self {
            id: article.id,
            id_readable: article.id_readable.unwrap_or_default(),
            summary: article.summary,
            content: article.content,
            project: article
                .project
                .map(|p| core::ProjectRef {
                    id: p.id,
                    name: p.name,
                    short_name: p.short_name,
                })
                .unwrap_or_else(|| core::ProjectRef {
                    id: String::new(),
                    name: None,
                    short_name: None,
                }),
            parent_article: article.parent_article.map(|p| core::ArticleRef {
                id: p.id,
                id_readable: p.id_readable,
                summary: p.summary,
            }),
            has_children: article.has_children.unwrap_or(false),
            tags: article.tags.into_iter().map(Into::into).collect(),
            created: article.created.unwrap_or_else(chrono::Utc::now),
            updated: article.updated.unwrap_or_else(chrono::Utc::now),
            reporter: article.reporter.map(|r| core::CommentAuthor {
                login: r.login,
                name: r.name,
            }),
        }
    }
}

/// Convert YouTrack ArticleAttachment to tracker-core ArticleAttachment
impl From<yt::ArticleAttachment> for core::ArticleAttachment {
    fn from(attachment: yt::ArticleAttachment) -> Self {
        Self {
            id: attachment.id,
            name: attachment.name,
            size: attachment.size,
            mime_type: attachment.mime_type,
            url: attachment.url,
            created: attachment.created,
        }
    }
}

/// Convert YouTrack ArticleComment to tracker-core Comment
impl From<yt::ArticleComment> for core::Comment {
    fn from(comment: yt::ArticleComment) -> Self {
        Self {
            id: comment.id,
            text: comment.text,
            author: comment.author.map(|a| core::CommentAuthor {
                login: a.login,
                name: a.name,
            }),
            created: comment.created,
        }
    }
}

/// Convert tracker-core CreateProject to YouTrack CreateProject
impl From<&core::CreateProject> for yt::CreateProject {
    fn from(create: &core::CreateProject) -> Self {
        Self {
            name: create.name.clone(),
            short_name: create.short_name.clone(),
            description: create.description.clone(),
        }
    }
}

/// Convert tracker-core CreateArticle to YouTrack CreateArticle
impl From<&core::CreateArticle> for yt::CreateArticle {
    fn from(create: &core::CreateArticle) -> Self {
        Self {
            project: yt::ProjectIdentifier {
                id: create.project_id.clone(),
            },
            summary: create.summary.clone(),
            content: create.content.clone(),
            parent_article: create
                .parent_article_id
                .as_ref()
                .map(|id| yt::ArticleIdentifier { id: id.clone() }),
            tags: create
                .tags
                .iter()
                .map(|name| yt::TagIdentifier::from_name(name.clone()))
                .collect(),
        }
    }
}

/// Convert tracker-core UpdateArticle to YouTrack UpdateArticle
impl From<&core::UpdateArticle> for yt::UpdateArticle {
    fn from(update: &core::UpdateArticle) -> Self {
        Self {
            summary: update.summary.clone(),
            content: update.content.clone(),
            tags: update
                .tags
                .iter()
                .map(|name| yt::TagIdentifier::from_name(name.clone()))
                .collect(),
        }
    }
}

// ============================================================================
// Custom Field Admin Conversions
// ============================================================================

/// Convert YouTrack CustomFieldResponse to tracker-core CustomFieldDefinition
pub fn custom_field_response_to_core(
    field: yt::CustomFieldResponse,
) -> core::CustomFieldDefinition {
    core::CustomFieldDefinition {
        id: field.id,
        name: field.name,
        field_type: field.field_type.id,
        instances_count: field.instances,
    }
}

/// Convert YouTrack BundleResponse to tracker-core BundleDefinition
pub fn bundle_response_to_core(bundle: yt::BundleResponse) -> core::BundleDefinition {
    // Extract bundle type from $type field (e.g., "EnumBundle" -> "enum")
    let bundle_type = match bundle.bundle_type.as_str() {
        "EnumBundle" => "enum",
        "StateBundle" => "state",
        "OwnedFieldBundle" => "ownedField",
        "VersionBundle" => "version",
        "BuildBundle" => "build",
        other => other,
    }
    .to_string();

    core::BundleDefinition {
        id: bundle.id,
        name: bundle.name,
        bundle_type,
        values: bundle
            .values
            .into_iter()
            .map(bundle_value_response_to_core)
            .collect(),
    }
}

/// Convert YouTrack BundleValueResponse to tracker-core BundleValueDefinition
pub fn bundle_value_response_to_core(
    value: yt::BundleValueResponse,
) -> core::BundleValueDefinition {
    core::BundleValueDefinition {
        id: value.id,
        name: value.name,
        description: value.description,
        is_resolved: value.is_resolved,
        ordinal: value.ordinal,
    }
}

/// Convert YouTrack ProjectCustomFieldResponse to tracker-core ProjectCustomField
pub fn project_custom_field_response_to_core(
    field: yt::ProjectCustomFieldResponse,
) -> core::ProjectCustomField {
    let field_type = field
        .field
        .field_type
        .as_ref()
        .map(|ft| ft.id.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let is_state_field = field_type.starts_with("state");

    let (values, state_values) = match field.bundle {
        Some(bundle) => {
            let values: Vec<String> = bundle.values.iter().map(|v| v.name.clone()).collect();

            let state_values: Vec<core::StateValueInfo> = if is_state_field {
                bundle
                    .values
                    .iter()
                    .map(|v| core::StateValueInfo {
                        name: v.name.clone(),
                        is_resolved: v.is_resolved.unwrap_or(false),
                        ordinal: v.ordinal.unwrap_or(0),
                    })
                    .collect()
            } else {
                vec![]
            };

            (values, state_values)
        }
        None => (vec![], vec![]),
    };

    core::ProjectCustomField {
        id: field.id,
        name: field.field.name,
        field_type,
        required: !field.can_be_empty,
        values,
        state_values,
    }
}
