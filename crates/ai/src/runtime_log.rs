//! Host-agnostic runtime log, ported from `web/src/lib/runtime-log.ts` and the
//! append/clear limits of `use-runtime-log-store`.
//!
//! The web keeps a `localforage`-persisted 500-entry store whose `append`
//! prepends the newest entry and slices to the limit. This port is a
//! framework-free in-memory ring buffer with a smaller default cap so the app
//! owns persistence and rendering; entries stay newest-first. There are no
//! globals: a caller holds its own [`RuntimeLogBuffer`].

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default ring-buffer capacity (the web's `RUNTIME_LOG_LIMIT`).
pub const RUNTIME_LOG_LIMIT: usize = 500;
/// `MAX_LOG_TEXT` (`runtime-log.ts:5`).
pub const MAX_LOG_TEXT: usize = 4000;

/// `RuntimeLogLevel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeLogLevel {
    #[default]
    Info,
    Warn,
    Error,
}

impl RuntimeLogLevel {
    /// The wire value (`"info" | "warn" | "error"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// `RuntimeLogEntry`: one record. `timestamp` is Unix milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLogEntry {
    pub level: RuntimeLogLevel,
    pub scope: String,
    pub message: String,
    pub timestamp: u64,
    pub data: Option<String>,
}

impl RuntimeLogEntry {
    /// Builds an entry stamped with the current time.
    pub fn new(
        level: RuntimeLogLevel,
        scope: impl Into<String>,
        message: impl Into<String>,
        data: Option<String>,
    ) -> Self {
        Self {
            level,
            scope: scope.into(),
            message: message.into(),
            timestamp: now_millis(),
            data,
        }
    }
}

/// `Date.now()` in milliseconds (0 if the clock predates the Unix epoch).
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// `truncateLogText` (`runtime-log.ts:8`): clamp to `max` characters, appending
/// the dropped count. The web's Chinese suffix is rendered in English here (the
/// desktop app is en-US only). The clamp is on Unicode scalar values, so it can
/// never split a multi-byte character.
pub fn truncate_log_text(text: &str, max: usize) -> String {
    let length = text.chars().count();
    if length <= max {
        return text.to_string();
    }
    let preview: String = text.chars().take(max).collect();
    format!("{preview}… (truncated {} chars)", length - max)
}

/// A bounded, newest-first ring buffer of [`RuntimeLogEntry`].
#[derive(Debug, Clone)]
pub struct RuntimeLogBuffer {
    capacity: usize,
    entries: VecDeque<RuntimeLogEntry>,
}

impl RuntimeLogBuffer {
    /// Creates a buffer holding at most `capacity` entries (at least one).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }

    /// Appends `entry`, evicting the oldest once the capacity is exceeded.
    pub fn push(&mut self, entry: RuntimeLogEntry) {
        self.entries.push_front(entry);
        self.entries.truncate(self.capacity);
    }

    /// Appends a record with a freshly stamped timestamp, clamping `data` to
    /// [`MAX_LOG_TEXT`] (`recordRuntimeLog`, `runtime-log.ts:24`).
    pub fn record(
        &mut self,
        level: RuntimeLogLevel,
        scope: impl Into<String>,
        message: impl Into<String>,
        data: Option<String>,
    ) {
        self.push(RuntimeLogEntry {
            level,
            scope: scope.into(),
            message: message.into(),
            timestamp: now_millis(),
            data: data.map(|text| truncate_log_text(&text, MAX_LOG_TEXT)),
        });
    }

    /// A newest-first copy of the buffered entries.
    pub fn snapshot(&self) -> Vec<RuntimeLogEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Drops every entry.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// The number of buffered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no entry is buffered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The maximum number of entries retained.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for RuntimeLogBuffer {
    fn default() -> Self {
        Self::new(RUNTIME_LOG_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(message: &str) -> RuntimeLogEntry {
        RuntimeLogEntry {
            level: RuntimeLogLevel::Info,
            scope: "test".to_string(),
            message: message.to_string(),
            timestamp: 0,
            data: None,
        }
    }

    #[test]
    fn ring_buffer_evicts_the_oldest_at_capacity() {
        let mut buffer = RuntimeLogBuffer::new(3);
        for index in 0..5 {
            buffer.push(entry(&format!("m{index}")));
        }
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.capacity(), 3);
        let messages: Vec<String> = buffer
            .snapshot()
            .into_iter()
            .map(|entry| entry.message)
            .collect();
        assert_eq!(messages, vec!["m4", "m3", "m2"]);
    }

    #[test]
    fn push_then_clear_empties_the_buffer() {
        let mut buffer = RuntimeLogBuffer::default();
        buffer.push(entry("a"));
        buffer.push(entry("b"));
        assert!(!buffer.is_empty());
        buffer.clear();
        assert!(buffer.is_empty());
        assert!(buffer.snapshot().is_empty());
    }

    #[test]
    fn default_capacity_is_the_web_limit() {
        assert_eq!(RUNTIME_LOG_LIMIT, 500);
        assert_eq!(RuntimeLogBuffer::default().capacity(), RUNTIME_LOG_LIMIT);
        // A zero capacity still retains one entry rather than panicking.
        let mut buffer = RuntimeLogBuffer::new(0);
        buffer.push(entry("only"));
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn record_clamps_data_and_stamps_the_time() {
        let mut buffer = RuntimeLogBuffer::default();
        let long = "a".repeat(MAX_LOG_TEXT + 5);
        buffer.record(RuntimeLogLevel::Error, "generation", "boom", Some(long));
        let entry = buffer
            .snapshot()
            .into_iter()
            .next()
            .unwrap_or_else(|| unreachable!("one entry"));
        assert_eq!(entry.level, RuntimeLogLevel::Error);
        assert_eq!(entry.scope, "generation");
        assert!(entry.timestamp > 0);
        let data = entry.data.unwrap_or_default();
        assert!(data.ends_with("… (truncated 5 chars)"));
    }

    #[test]
    fn truncate_keeps_short_text_and_counts_dropped_chars() {
        assert_eq!(truncate_log_text("hello", 10), "hello");
        assert_eq!(truncate_log_text("hello", 2), "he… (truncated 3 chars)");
        assert_eq!(truncate_log_text("hello", 0), "… (truncated 5 chars)");
        // Multi-byte safe: 5 chars, keep 2.
        assert_eq!(truncate_log_text("héllo", 2), "hé… (truncated 3 chars)");
    }
}
