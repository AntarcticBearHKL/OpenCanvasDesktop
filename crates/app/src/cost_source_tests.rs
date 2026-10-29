//! Cost-record source classification and the cost tab's `~` prefix rule: the
//! provider's own amount (`api`), a `/generation` lookup (`lookup`) and a
//! client-side estimate (`estimate`) each land in the record with their own
//! source, and only an estimate is rendered with the `~` prefix.

use crate::config_tabs::cost_rows;
use crate::cost::{
    generation_charge, CostHint, CostLog, CostRecord, GenerationCharge, COST_SOURCE_API,
    COST_SOURCE_ESTIMATE, COST_SOURCE_LOOKUP,
};
use crate::i18n::Locale;
use open_canvas_ai::generation_cost::{GenerationCostUnit, TokenUsage};
use open_canvas_core::node::{CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, Position};

fn node(node_type: CanvasNodeType, metadata: CanvasNodeMetadata) -> CanvasNodeData {
    CanvasNodeData {
        id: "node-1".to_string(),
        node_type,
        title: "Node".to_string(),
        position: Position::new(0.0, 0.0),
        width: 240.0,
        height: 180.0,
        metadata: Some(metadata),
    }
}

fn charge(
    model: &str,
    unit: GenerationCostUnit,
    quantity: f64,
    fallback: CostHint,
) -> GenerationCharge {
    GenerationCharge {
        model: model.to_string(),
        unit,
        quantity,
        fallback,
    }
}

#[test]
fn cost_tab_groups_by_model_keeps_latest_twenty_and_shows_the_source() -> Result<(), &'static str> {
    let mut log = CostLog::default();
    for at in 0..25u64 {
        let model = if at % 2 == 0 { "alpha" } else { "beta" };
        log.push(CostRecord {
            id: 0,
            node_id: "n".into(),
            model: model.into(),
            unit: "image".into(),
            quantity: 1.0,
            usd: 0.01,
            priced: true,
            source: COST_SOURCE_ESTIMATE.into(),
            reason: None,
            at,
        });
    }
    let rows = cost_rows(Locale::EnUs, &log);
    let headings: Vec<&str> = rows
        .iter()
        .filter(|row| row.heading)
        .map(|row| row.model.as_str())
        .collect();
    assert_eq!(headings, vec!["alpha", "beta"], "first-seen model order");
    // 20 records + 2 headers; the five oldest records were not rendered.
    assert_eq!(rows.len(), 22);
    let records: Vec<&crate::CostRecordRow> = rows.iter().filter(|row| !row.heading).collect();
    assert_eq!(records.len(), 20);
    assert_eq!(records[0].source.as_str(), "estimate");
    assert!(records.iter().all(|row| !row.source.is_empty()));

    // A newer record with a different source updates its column.
    let mut log = CostLog::default();
    log.record_lookup("openai/img", GenerationCostUnit::Image, 1.0, 0.02, 5);
    let rows = cost_rows(Locale::EnUs, &log);
    let record = rows.iter().find(|row| !row.heading).ok_or("record row")?;
    assert_eq!(record.source.as_str(), COST_SOURCE_LOOKUP);
    Ok(())
}

#[test]
fn api_amount_wins_and_marks_the_api_source() {
    let mut log = CostLog::default();
    let charge = charge(
        "openai/gpt-image-2.5-sunburst",
        GenerationCostUnit::Image,
        1.0,
        CostHint::Api(0.037_5),
    );
    let source = log.record_generation("node-1", &charge, 10);
    assert_eq!(source, COST_SOURCE_API);
    let record = &log.records[0];
    assert_eq!(record.source, COST_SOURCE_API);
    assert_eq!(record.node_id, "node-1");
    assert!(record.priced);
    assert!((record.usd - 0.0375).abs() < 1e-9);
    assert!(record.reason.is_none());
}

#[test]
fn a_non_finite_api_amount_falls_back_to_the_estimate() {
    let mut log = CostLog::default();
    let charge = charge(
        "gpt-image-1",
        GenerationCostUnit::Image,
        1.0,
        CostHint::Api(f64::NAN),
    );
    let source = log.record_generation("node-1", &charge, 10);
    assert_eq!(source, COST_SOURCE_ESTIMATE);
    assert_eq!(log.records[0].source, COST_SOURCE_ESTIMATE);
    assert!((log.records[0].usd - 0.04).abs() < 1e-9);
}

#[test]
fn token_usage_prices_the_token_model_as_an_estimate() {
    let mut log = CostLog::default();
    let usage = TokenUsage {
        prompt_tokens: Some(1000.0),
        completion_tokens: Some(500.0),
        total_tokens: None,
        has_reference: false,
    };
    let charge = charge(
        "openai/gpt-image-2.5-sunburst",
        GenerationCostUnit::Image,
        1.0,
        CostHint::Tokens(usage),
    );
    let source = log.record_generation("node-1", &charge, 5);
    assert_eq!(source, COST_SOURCE_ESTIMATE);
    assert!((log.records[0].usd - 0.02).abs() < 1e-9);
    assert_eq!(log.records[0].source, COST_SOURCE_ESTIMATE);
}

#[test]
fn token_usage_without_a_token_price_falls_back_to_the_model_table() {
    let mut log = CostLog::default();
    let usage = TokenUsage {
        prompt_tokens: Some(10.0),
        ..TokenUsage::default()
    };
    let charge = charge(
        "gpt-image-1",
        GenerationCostUnit::Image,
        2.0,
        CostHint::Tokens(usage),
    );
    let source = log.record_generation("node-1", &charge, 5);
    assert_eq!(source, COST_SOURCE_ESTIMATE);
    assert!((log.records[0].usd - 0.08).abs() < 1e-9);
}

#[test]
fn generation_lookup_keeps_the_node_and_marks_the_lookup_source() {
    let mut log = CostLog::default();
    log.record_generation_lookup(
        "node-7",
        "gpt-image-1",
        GenerationCostUnit::Image,
        1.0,
        0.02,
        9,
    );
    let record = &log.records[0];
    assert_eq!(record.source, COST_SOURCE_LOOKUP);
    assert_eq!(record.node_id, "node-7");
    assert!(record.priced);
    assert!((record.usd - 0.02).abs() < 1e-9);
}

#[test]
fn generation_charge_follows_the_node_kind() {
    let text = node(CanvasNodeType::Text, CanvasNodeMetadata::default());
    assert_eq!(
        generation_charge(&text),
        Some((GenerationCostUnit::Call, 1.0))
    );
    let image = node(
        CanvasNodeType::ImageGeneration,
        CanvasNodeMetadata {
            count: Some(3),
            ..CanvasNodeMetadata::default()
        },
    );
    assert_eq!(
        generation_charge(&image),
        Some((GenerationCostUnit::Image, 3.0))
    );
    let video = node(
        CanvasNodeType::VideoGeneration,
        CanvasNodeMetadata {
            seconds: Some("6".to_string()),
            ..CanvasNodeMetadata::default()
        },
    );
    assert_eq!(
        generation_charge(&video),
        Some((GenerationCostUnit::VideoSecond, 6.0))
    );
    let prompt = node(CanvasNodeType::Prompt, CanvasNodeMetadata::default());
    assert_eq!(generation_charge(&prompt), None);
}

#[test]
fn cost_tab_prefixes_only_estimates_with_a_tilde() {
    let mut log = CostLog::default();
    let api = charge(
        "gpt-image-1",
        GenerationCostUnit::Image,
        1.0,
        CostHint::Api(0.04),
    );
    log.record_generation("node-1", &api, 3);
    log.record_generation_lookup(
        "node-2",
        "gpt-image-1",
        GenerationCostUnit::Image,
        1.0,
        0.05,
        2,
    );
    let estimated = charge(
        "gpt-image-1",
        GenerationCostUnit::Image,
        1.0,
        CostHint::None,
    );
    log.record_generation("node-3", &estimated, 1);
    let unpriced = charge("nope", GenerationCostUnit::Image, 1.0, CostHint::None);
    log.record_generation("node-4", &unpriced, 0);

    let rows = cost_rows(Locale::EnUs, &log);
    let records: Vec<&crate::CostRecordRow> = rows.iter().filter(|row| !row.heading).collect();
    let amount = |index: usize| records.get(index).map(|row| row.amount.to_string());
    assert_eq!(
        amount(3),
        Some("$0.04".to_string()),
        "api amounts are exact"
    );
    assert_eq!(amount(2), Some("$0.05".to_string()), "lookups are exact");
    assert_eq!(
        amount(1),
        Some("~$0.04".to_string()),
        "estimates are marked"
    );
    assert_eq!(amount(0), Some("Unpriced".to_string()));
    assert_eq!(
        records
            .iter()
            .filter(|row| row.amount.starts_with('~'))
            .count(),
        1,
        "only the estimate carries the ~ prefix"
    );
    assert_eq!(records[3].source.as_str(), COST_SOURCE_API);
    assert_eq!(records[2].source.as_str(), COST_SOURCE_LOOKUP);
    assert_eq!(records[1].source.as_str(), COST_SOURCE_ESTIMATE);
}
