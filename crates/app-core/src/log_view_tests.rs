//! Log-viewer tests: level/category filtering, newest-first order, the display
//! cap, the expandable key and the plain-text copy/download payload.

use open_canvas_ai::runtime_log::{RuntimeLogBuffer, RuntimeLogLevel};

use super::*;

fn entry(buffer: &mut RuntimeLogBuffer, level: RuntimeLogLevel, scope: &str, message: &str) {
    buffer.push(open_canvas_ai::runtime_log::RuntimeLogEntry {
        level,
        scope: scope.to_string(),
        message: message.to_string(),
        timestamp: 1_700_000_000_000,
        data: None,
    });
}

#[test]
fn filter_passes_only_the_selected_level() {
    assert!(level_matches(FILTER_ALL, RuntimeLogLevel::Info));
    assert!(level_matches(FILTER_ALL, RuntimeLogLevel::Error));
    assert!(level_matches("warn", RuntimeLogLevel::Warn));
    assert!(!level_matches("warn", RuntimeLogLevel::Info));
    assert!(!level_matches("error", RuntimeLogLevel::Warn));
}

#[test]
fn category_filter_matches_the_entry_scope() {
    assert!(category_matches(CATEGORY_ALL, "generation"));
    assert!(category_matches("cost", "cost"));
    assert!(!category_matches("cost", "generation"));
}

#[test]
fn lines_are_newest_first_and_filtered() {
    let mut buffer = RuntimeLogBuffer::default();
    entry(&mut buffer, RuntimeLogLevel::Info, "test", "first");
    entry(&mut buffer, RuntimeLogLevel::Warn, "test", "second");
    entry(&mut buffer, RuntimeLogLevel::Error, "test", "third");

    let all = lines(&buffer, FILTER_ALL, CATEGORY_ALL);
    let messages: Vec<&str> = all.iter().map(|line| line.message.as_str()).collect();
    assert_eq!(messages, vec!["third", "second", "first"]);

    let errors = lines(&buffer, "error", CATEGORY_ALL);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "third");
    assert_eq!(errors[0].level, "error");
    assert_eq!(lines(&buffer, "info", CATEGORY_ALL)[0].message, "first");
}

#[test]
fn category_and_level_filters_combine() {
    let mut buffer = RuntimeLogBuffer::default();
    entry(&mut buffer, RuntimeLogLevel::Info, "generation", "gen-info");
    entry(
        &mut buffer,
        RuntimeLogLevel::Error,
        "generation",
        "gen-error",
    );
    entry(&mut buffer, RuntimeLogLevel::Error, "cost", "cost-error");

    let rows = lines(&buffer, "error", "cost");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message, "cost-error");

    let generation = lines(&buffer, FILTER_ALL, "generation");
    assert_eq!(generation.len(), 2);
    assert_eq!(generation[0].message, "gen-error");
}

#[test]
fn categories_are_all_plus_sorted_distinct_scopes() {
    let mut buffer = RuntimeLogBuffer::default();
    entry(&mut buffer, RuntimeLogLevel::Info, "generation", "a");
    entry(&mut buffer, RuntimeLogLevel::Info, "cost", "b");
    entry(&mut buffer, RuntimeLogLevel::Info, "generation", "c");
    assert_eq!(
        categories(&buffer),
        vec![
            CATEGORY_ALL.to_string(),
            "cost".to_string(),
            "generation".to_string()
        ]
    );
}

#[test]
fn display_window_caps_at_fifty_rows() {
    let mut buffer = RuntimeLogBuffer::default();
    for index in 0..(DISPLAY_LIMIT + 10) {
        entry(
            &mut buffer,
            RuntimeLogLevel::Info,
            "test",
            &format!("m{index}"),
        );
    }
    let visible = lines(&buffer, FILTER_ALL, CATEGORY_ALL);
    assert_eq!(visible.len(), DISPLAY_LIMIT);
    assert_eq!(
        visible[0].message,
        format!("m{}", DISPLAY_LIMIT + 9),
        "the newest entries survive the cap"
    );
}

#[test]
fn detail_and_time_project_onto_the_row() {
    let mut buffer = RuntimeLogBuffer::default();
    buffer.push(open_canvas_ai::runtime_log::RuntimeLogEntry {
        level: RuntimeLogLevel::Error,
        scope: "generation".to_string(),
        message: "boom".to_string(),
        timestamp: 1_700_000_000_000,
        data: Some("stack trace".to_string()),
    });
    let row = &lines(&buffer, FILTER_ALL, CATEGORY_ALL)[0];
    assert_eq!(row.scope, "generation");
    assert_eq!(row.detail, "stack trace");
    assert!(!row.key.is_empty(), "the key identifies the expandable row");
    assert!(!row.time.is_empty(), "a valid timestamp formats");
}

#[test]
fn visible_text_lists_rows_and_their_detail() {
    let mut buffer = RuntimeLogBuffer::default();
    entry(&mut buffer, RuntimeLogLevel::Warn, "cost", "slow");
    buffer.push(open_canvas_ai::runtime_log::RuntimeLogEntry {
        level: RuntimeLogLevel::Error,
        scope: "generation".to_string(),
        message: "boom".to_string(),
        timestamp: 1_700_000_000_000,
        data: Some("stack".to_string()),
    });
    let text = visible_text(&lines(&buffer, FILTER_ALL, CATEGORY_ALL));
    let rows: Vec<&str> = text.lines().collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].starts_with("error\t"));
    assert!(rows[0].ends_with("\tgeneration\tboom\tstack"));
    assert!(rows[1].starts_with("warn\t"));
    assert!(rows[1].ends_with("\tcost\tslow"));
}

#[test]
fn time_formatting_is_stable_for_out_of_range_values() {
    assert!(format_time(0).len() >= 11, "the epoch formats locally");
    let formatted = format_time(1_700_000_000_000);
    assert_eq!(formatted.len(), 14, "MM-DD HH:MM:SS");
    assert_eq!(format_time(u64::MAX), "", "i64 overflow renders empty");
}
