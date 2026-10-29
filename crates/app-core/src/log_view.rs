//! Runtime-log viewer rows: the level/category filters and the display
//! projection of `open_canvas_ai::RuntimeLogBuffer` (newest first, `record`
//! keeps entries newest-first already). Kept free of Slint types so the filters
//! are unit testable; `config_tabs.rs` maps [`LogLine`] onto the view struct.
//!
//! The web's store keeps level + category filters and `DISPLAY_LIMIT = 50`
//! (`config-runtime-logs.tsx`); this port filters by level and by the entry's
//! scope (its category), caps the rendered window the same way and exposes the
//! visible rows as plain text for the Copy / Download actions.

use chrono::TimeZone;
use open_canvas_ai::runtime_log::{RuntimeLogBuffer, RuntimeLogLevel};

/// `all` filter id (the web's level Select `all` option).
pub const FILTER_ALL: &str = "all";
/// `all` category filter id (the web's category Select `all` option).
pub const CATEGORY_ALL: &str = "all";
/// The web's `DISPLAY_LIMIT`: at most this many rows render.
pub const DISPLAY_LIMIT: usize = 50;

/// One display row of the log list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Stable identity of the entry (used by the expandable detail rows).
    pub key: String,
    pub level: &'static str,
    pub scope: String,
    pub message: String,
    pub time: String,
    pub detail: String,
}

/// True when `level` passes `filter` (`all` passes everything).
pub fn level_matches(filter: &str, level: RuntimeLogLevel) -> bool {
    filter == FILTER_ALL || filter == level.as_str()
}

/// True when `scope` passes the category `filter` (`all` passes everything).
pub fn category_matches(filter: &str, scope: &str) -> bool {
    filter == CATEGORY_ALL || filter == scope
}

/// The category filter choices: `all` followed by every distinct scope in the
/// buffer, sorted so the chip row does not reshuffle as entries arrive.
pub fn categories(buffer: &RuntimeLogBuffer) -> Vec<String> {
    let mut scopes: Vec<String> = buffer
        .snapshot()
        .into_iter()
        .map(|entry| entry.scope)
        .filter(|scope| !scope.is_empty())
        .collect();
    scopes.sort();
    scopes.dedup();
    let mut categories = Vec::with_capacity(scopes.len() + 1);
    categories.push(CATEGORY_ALL.to_string());
    categories.extend(scopes);
    categories
}

/// The newest-first, filtered, capped display window of `buffer`.
pub fn lines(buffer: &RuntimeLogBuffer, level: &str, category: &str) -> Vec<LogLine> {
    buffer
        .snapshot()
        .into_iter()
        .filter(|entry| {
            level_matches(level, entry.level) && category_matches(category, &entry.scope)
        })
        .take(DISPLAY_LIMIT)
        .map(|entry| LogLine {
            key: format!("{}|{}|{}", entry.timestamp, entry.scope, entry.message),
            level: entry.level.as_str(),
            scope: entry.scope,
            message: entry.message,
            time: format_time(entry.timestamp),
            detail: entry.data.unwrap_or_default(),
        })
        .collect()
}

/// The visible rows as tab-separated plain text (the Copy / Download payload).
pub fn visible_text(lines: &[LogLine]) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(line.level);
        text.push('\t');
        text.push_str(&line.time);
        text.push('\t');
        text.push_str(&line.scope);
        text.push('\t');
        text.push_str(&line.message);
        if !line.detail.is_empty() {
            text.push('\t');
            text.push_str(&line.detail);
        }
        text.push('\n');
    }
    text
}

/// Unix-millisecond timestamp as `MM-DD HH:MM:SS` in local time; an
/// out-of-range value renders empty rather than failing.
pub fn format_time(timestamp_ms: u64) -> String {
    let Ok(millis) = i64::try_from(timestamp_ms) else {
        return String::new();
    };
    chrono::Local
        .timestamp_millis_opt(millis)
        .single()
        .map_or_else(String::new, |time| {
            time.format("%m-%d %H:%M:%S").to_string()
        })
}

#[cfg(test)]
#[path = "log_view_tests.rs"]
mod tests;
