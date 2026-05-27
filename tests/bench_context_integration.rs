use std::time::Instant;
use track::commands::context::IssueSummary;
use tracker_core::{Issue, CustomField, ProjectRef};

#[test]
fn bench_context_conversion_optimized() {
    let mut custom_fields = Vec::new();

    // Add some junk fields
    for i in 0..50 {
        custom_fields.push(CustomField::SingleEnum {
            name: format!("Field {}", i),
            value: Some(format!("Value {}", i)),
        });
    }

    // Add target fields
    custom_fields.push(CustomField::State {
        name: "State".to_string(),
        value: Some("In Progress".to_string()),
        is_resolved: false,
    });

    // More junk
    for i in 50..100 {
        custom_fields.push(CustomField::SingleEnum {
            name: format!("Field {}", i),
            value: Some(format!("Value {}", i)),
        });
    }

    custom_fields.push(CustomField::SingleEnum {
        name: "Priority".to_string(),
        value: Some("High".to_string()),
    });

    // More junk
    for i in 100..150 {
        custom_fields.push(CustomField::SingleEnum {
            name: format!("Field {}", i),
            value: Some(format!("Value {}", i)),
        });
    }

    custom_fields.push(CustomField::SingleUser {
        name: "Assignee".to_string(),
        login: Some("jules".to_string()),
        display_name: Some("Jules".to_string()),
    });

    let issue = Issue {
        id: "TEST-123".to_string(),
        id_readable: "TEST-123".to_string(),
        summary: "Test Issue".to_string(),
        description: None,
        project: ProjectRef {
            id: "TEST".to_string(),
            name: Some("Test Project".to_string()),
            short_name: Some("TEST".to_string()),
        },
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        custom_fields,
        tags: vec![],
    };

    let start = Instant::now();
    let iterations = 1_000_000;

    for _ in 0..iterations {
        let _summary = IssueSummary::from(&issue);
    }

    let duration = start.elapsed();
    println!("Elapsed time optimized for {} iterations: {:?}", iterations, duration);
}
