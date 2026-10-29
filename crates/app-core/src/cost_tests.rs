//! Cost-record tests: the 200-entry cap and oldest eviction, newest-first
//! order, priced/unpriced totals, clear, the estimator wiring and the
//! `costs.json` parse path (including an oversized file).

use open_canvas_ai::generation_cost::{GenerationCostUnit, GENERATION_COST_RECORD_LIMIT};

use super::*;

fn record(model: &str, usd: f64, priced: bool, at: u64) -> CostRecord {
    CostRecord {
        id: 0,
        node_id: "node-1".to_string(),
        model: model.to_string(),
        unit: "image".to_string(),
        quantity: 1.0,
        usd,
        priced,
        source: COST_SOURCE_ESTIMATE.to_string(),
        reason: None,
        at,
    }
}

#[test]
fn push_prepends_newest_first() {
    let mut log = CostLog::default();
    log.push(record("a", 0.01, true, 1));
    log.push(record("b", 0.02, true, 2));
    let models: Vec<&str> = log
        .records
        .iter()
        .map(|entry| entry.model.as_str())
        .collect();
    assert_eq!(models, vec!["b", "a"]);
    assert_eq!(log.records[0].id, 2, "ids increment in push order");
}

#[test]
fn cap_evicts_the_oldest_and_keeps_the_limit() {
    let mut log = CostLog::default();
    let limit = GENERATION_COST_RECORD_LIMIT as u64;
    for at in 0..(limit + 5) {
        log.push(record("m", 0.01, true, at));
    }
    assert_eq!(log.records.len(), GENERATION_COST_RECORD_LIMIT);
    assert_eq!(log.records[0].at, limit + 4);
    assert_eq!(
        log.records.last().map(|entry| entry.at),
        Some(5),
        "the five oldest records were evicted"
    );
}

#[test]
fn totals_count_only_priced_records() {
    let mut log = CostLog::default();
    log.push(record("priced", 0.04, true, 1));
    log.push(record("priced", 0.02, true, 2));
    log.push(record("unpriced", 0.0, false, 3));
    assert!((log.total_usd() - 0.06).abs() < 1e-9);
    assert_eq!(log.unpriced_count(), 1);
    log.clear();
    assert_eq!(log.records, Vec::<CostRecord>::new());
    assert!(log.total_usd().abs() < f64::EPSILON);
    assert_eq!(log.unpriced_count(), 0);
}

#[test]
fn record_estimate_uses_the_shared_estimator() {
    let mut log = CostLog::default();
    log.record_estimate("node-1", "gpt-image-1", GenerationCostUnit::Image, 2.0, 10);
    log.record_estimate("node-2", "nope", GenerationCostUnit::Image, 1.0, 11);
    assert_eq!(log.records[0].source, COST_SOURCE_ESTIMATE);
    assert_eq!(log.records[1].usd, 0.08);
    assert!(log.records[1].priced);
    assert_eq!(log.records[0].usd, 0.0);
    assert!(!log.records[0].priced);
    assert_eq!(log.records[0].reason.as_deref(), Some("unpriced-model"));
    assert_eq!(log.unpriced_count(), 1);
}

#[test]
fn record_lookup_marks_the_lookup_source() {
    let mut log = CostLog::default();
    log.record_lookup(
        "openai/gpt-image-2.5-sunburst",
        GenerationCostUnit::Image,
        1.0,
        0.037,
        5,
    );
    let entry = &log.records[0];
    assert_eq!(entry.source, COST_SOURCE_LOOKUP);
    assert!(entry.priced);
    assert!((log.total_usd() - 0.037).abs() < 1e-9);
}

#[test]
fn parse_round_trips_and_repairs_the_id_counter() -> Result<(), String> {
    let mut log = CostLog::default();
    log.push(record("a", 0.01, true, 1));
    log.push(record("b", 0.02, true, 2));
    let text = serde_json::to_string(&log).map_err(|error| error.to_string())?;
    let loaded = CostLog::parse(&text)?;
    assert_eq!(loaded, log);
    assert_eq!(loaded.next_id, 3);
    Ok(())
}

#[test]
fn parse_rejects_garbage_and_trims_oversized_files() -> Result<(), String> {
    assert!(CostLog::parse("not json").is_err());
    let mut log = CostLog::default();
    let limit = GENERATION_COST_RECORD_LIMIT as u64;
    for at in 0..(limit + 10) {
        log.push(record("m", 0.01, true, at));
    }
    let text = serde_json::to_string(&log).map_err(|error| error.to_string())?;
    let loaded = CostLog::parse(&text)?;
    assert_eq!(loaded.records.len(), GENERATION_COST_RECORD_LIMIT);
    Ok(())
}

#[test]
fn legacy_file_without_next_id_still_loads() -> Result<(), String> {
    let text = r#"{"records":[{"id":7,"nodeId":"n","model":"m","unit":"image","quantity":1.0,"usd":0.01,"priced":true,"source":"estimate","at":1}]}"#;
    let log = CostLog::parse(text)?;
    assert_eq!(log.records.len(), 1);
    assert_eq!(log.next_id, 8);
    Ok(())
}
