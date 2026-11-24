//! Checking custom-field updates against a project's schema before they are
//! sent.
//!
//! A backend's schema is not always the whole truth: several write paths
//! accept alias names (`State` for `Status`) and value forms (`done` for
//! `closed`) the schema never lists. So the backend is the judge, through
//! [`IssueTracker::validation_rules`] and [`IssueTracker::check_field_value`],
//! and this module rejects only what the backend says it would reject or
//! silently drop.

use crate::models::{CustomFieldUpdate, ProjectCustomField};
use crate::strings::unicode_eq_ignore_case;
use crate::traits::IssueTracker;
use std::fmt;

/// How strictly a backend's field names can be checked.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidationRules {
    /// The schema lists every field the write path accepts, so an unknown
    /// name is an error. When false it is only worth a warning.
    pub names_exhaustive: bool,
    /// `(alias, schema name)` pairs the write path accepts, compared without
    /// regard to case.
    pub name_aliases: &'static [(&'static str, &'static str)],
    /// `(schema name, what to do instead)` for fields the write path
    /// ignores. Setting one would be dropped silently, so it is an error.
    pub read_only_fields: &'static [(&'static str, &'static str)],
}

/// A backend's verdict on one value for one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueCheck {
    /// The write path accepts it.
    Valid,
    /// The write path would reject or drop it.
    Invalid { valid_values: Vec<String> },
    /// The backend cannot tell without trying.
    Unknown,
}

/// Verdict by list membership: a value is valid when the field lists it.
/// A field with no listed values cannot be judged.
pub fn check_membership(field: &ProjectCustomField, value: &str) -> ValueCheck {
    if field.values.is_empty() {
        return ValueCheck::Unknown;
    }
    if field
        .values
        .iter()
        .any(|known| unicode_eq_ignore_case(known, value))
    {
        ValueCheck::Valid
    } else {
        ValueCheck::Invalid {
            valid_values: field.values.clone(),
        }
    }
}

/// Something wrong with a requested field update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldProblem {
    UnknownField {
        name: String,
        available: Vec<String>,
        suggestion: Option<String>,
    },
    InvalidValue {
        field: String,
        value: String,
        valid_values: Vec<String>,
        suggestion: Option<String>,
    },
    ReadOnlyField {
        name: String,
        hint: String,
    },
}

impl fmt::Display for FieldProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldProblem::UnknownField {
                name,
                available,
                suggestion,
            } => {
                write!(f, "Unknown field '{name}'.")?;
                if let Some(suggestion) = suggestion {
                    write!(f, " Did you mean '{suggestion}'?")?;
                }
                if !available.is_empty() {
                    write!(f, " Available fields: {}", available.join(", "))?;
                }
                Ok(())
            }
            FieldProblem::InvalidValue {
                field,
                value,
                valid_values,
                suggestion,
            } => {
                write!(f, "Invalid value '{value}' for field '{field}'.")?;
                if let Some(suggestion) = suggestion {
                    write!(f, " Did you mean '{suggestion}'?")?;
                }
                write!(f, " Valid values: {}", valid_values.join(", "))
            }
            FieldProblem::ReadOnlyField { name, hint } => write!(
                f,
                "Field '{name}' cannot be set with --field on this backend (it would be ignored). {hint}"
            ),
        }
    }
}

/// Outcome of checking a set of updates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Problems the backend would reject or silently drop.
    pub errors: Vec<FieldProblem>,
    /// Things worth saying that may still work.
    pub warnings: Vec<FieldProblem>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Find the schema field an update names: by name, then through an alias.
pub fn resolve_field<'a>(
    rules: &ValidationRules,
    schema: &'a [ProjectCustomField],
    name: &str,
) -> Option<&'a ProjectCustomField> {
    let by_name = |wanted: &str| {
        schema
            .iter()
            .find(|field| unicode_eq_ignore_case(&field.name, wanted))
    };
    by_name(name).or_else(|| {
        rules
            .name_aliases
            .iter()
            .filter(|(alias, _)| unicode_eq_ignore_case(alias, name))
            .find_map(|(_, target)| by_name(target))
    })
}

/// Check `updates` against `schema` the way `tracker`'s write path would
/// treat them.
pub fn validate_fields(
    tracker: &dyn IssueTracker,
    schema: &[ProjectCustomField],
    updates: &[CustomFieldUpdate],
) -> ValidationReport {
    let rules = tracker.validation_rules();
    let mut report = ValidationReport::default();

    for update in updates {
        let name = update.name();
        let Some(field) = resolve_field(&rules, schema, name) else {
            let available: Vec<String> = schema.iter().map(|f| f.name.clone()).collect();
            let problem = FieldProblem::UnknownField {
                name: name.to_string(),
                suggestion: nearest(name, &available).map(str::to_string),
                available,
            };
            if rules.names_exhaustive {
                report.errors.push(problem);
            } else {
                report.warnings.push(problem);
            }
            continue;
        };

        if let Some((_, hint)) = rules
            .read_only_fields
            .iter()
            .find(|(read_only, _)| unicode_eq_ignore_case(read_only, &field.name))
        {
            report.errors.push(FieldProblem::ReadOnlyField {
                name: field.name.clone(),
                hint: hint.to_string(),
            });
            continue;
        }

        let values: Vec<&str> = match update {
            CustomFieldUpdate::SingleEnum { value, .. }
            | CustomFieldUpdate::State { value, .. } => {
                vec![value.as_str()]
            }
            CustomFieldUpdate::MultiEnum { values, .. } => {
                values.iter().map(String::as_str).collect()
            }
            CustomFieldUpdate::SingleUser { login, .. } => vec![login.as_str()],
        };
        for value in values {
            if let ValueCheck::Invalid { valid_values } = tracker.check_field_value(field, value) {
                report.errors.push(FieldProblem::InvalidValue {
                    field: field.name.clone(),
                    value: value.to_string(),
                    suggestion: nearest(value, &valid_values).map(str::to_string),
                    valid_values,
                });
            }
        }
    }

    report
}

/// The candidate closest to `wanted`, if one is close enough to be a likely
/// typo or abbreviation: a shared prefix, or a small edit distance.
pub fn nearest<'a>(wanted: &str, candidates: &'a [String]) -> Option<&'a str> {
    let wanted_key = wanted.trim().to_lowercase();
    if wanted_key.is_empty() {
        return None;
    }
    if let Some(prefix_match) = candidates.iter().find(|candidate| {
        let key = candidate.to_lowercase();
        key.starts_with(&wanted_key) || wanted_key.starts_with(&key)
    }) {
        return Some(prefix_match);
    }
    let limit = (wanted_key.chars().count() / 3).max(2);
    candidates
        .iter()
        .map(|candidate| {
            (
                edit_distance(&wanted_key, &candidate.to_lowercase()),
                candidate,
            )
        })
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate.as_str())
}

/// Levenshtein distance over characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(previous + 1);
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, values: &[&str]) -> ProjectCustomField {
        ProjectCustomField {
            id: name.to_lowercase(),
            name: name.to_string(),
            field_type: "enum[1]".to_string(),
            required: false,
            values: values.iter().map(|v| v.to_string()).collect(),
            state_values: vec![],
            values_complete: true,
        }
    }

    #[test]
    fn membership_is_case_insensitive_and_needs_a_list() {
        let priority = field("Priority", &["Low", "High"]);
        assert_eq!(check_membership(&priority, "high"), ValueCheck::Valid);
        assert_eq!(
            check_membership(&priority, "Urgent"),
            ValueCheck::Invalid {
                valid_values: vec!["Low".to_string(), "High".to_string()]
            }
        );
        assert_eq!(
            check_membership(&field("Notes", &[]), "x"),
            ValueCheck::Unknown
        );
    }

    #[test]
    fn fields_resolve_by_name_then_alias() {
        let schema = vec![field("Status", &["Open"])];
        let rules = ValidationRules {
            name_aliases: &[("State", "Status")],
            ..Default::default()
        };
        assert!(resolve_field(&rules, &schema, "status").is_some());
        assert!(resolve_field(&rules, &schema, "STATE").is_some());
        assert!(resolve_field(&rules, &schema, "Stage").is_none());
        assert!(resolve_field(&ValidationRules::default(), &schema, "State").is_none());
    }

    #[test]
    fn nearest_finds_abbreviations_and_typos_only() {
        let names = vec![
            "Priority".to_string(),
            "Type".to_string(),
            "Assignee".to_string(),
        ];
        assert_eq!(nearest("Prio", &names), Some("Priority"));
        assert_eq!(nearest("Tpye", &names), Some("Type"));
        assert_eq!(nearest("asignee", &names), Some("Assignee"));
        assert_eq!(nearest("Sprint", &names), None);
        assert_eq!(nearest("", &names), None);
    }

    #[test]
    fn problems_read_as_one_actionable_sentence() {
        let unknown = FieldProblem::UnknownField {
            name: "Prio".to_string(),
            available: vec!["Priority".to_string(), "Type".to_string()],
            suggestion: Some("Priority".to_string()),
        };
        assert_eq!(
            unknown.to_string(),
            "Unknown field 'Prio'. Did you mean 'Priority'? Available fields: Priority, Type"
        );
        let invalid = FieldProblem::InvalidValue {
            field: "Priority".to_string(),
            value: "Bogus".to_string(),
            valid_values: vec!["Low".to_string(), "High".to_string()],
            suggestion: None,
        };
        assert_eq!(
            invalid.to_string(),
            "Invalid value 'Bogus' for field 'Priority'. Valid values: Low, High"
        );
    }
}
