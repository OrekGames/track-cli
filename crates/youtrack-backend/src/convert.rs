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

/// Convert YouTrack ProjectCustomField to tracker-core ProjectCustomField
impl From<yt::ProjectCustomField> for core::ProjectCustomField {
    fn from(field: yt::ProjectCustomField) -> Self {
        Self {
            id: field.id,
            name: field.field.name,
            field_type: field
                .field
                .field_type
                .and_then(|ft| ft.presentation)
                .unwrap_or_else(|| "unknown".to_string()),
            required: !field.can_be_empty,
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
