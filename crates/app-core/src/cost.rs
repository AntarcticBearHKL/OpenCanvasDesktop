//! Persisted generation-cost records, ported from
//! `web/src/stores/use-generation-cost-store.ts`: newest-first, capped at
//! [`GENERATION_COST_RECORD_LIMIT`] (200), each record carrying the model, the
//! billed unit and quantity, the USD amount, the pricing source
//! (`estimate` | `lookup` | `api`) and the reason an estimate is unpriced.
//!
//! Records are produced in three real places: a finished generation resolves
//! its cost from the response (`usage.cost` -> `api`, a generation-id lookup ->
//! `lookup`, token usage / the model price table -> `estimate`), and the Cost
//! tab's lookup asks the provider for a generation id with
//! `fetch_generation_cost` and stores the returned
//! [`open_canvas_ai::GenerationCost`] amount.

use open_canvas_ai::generation_cost::{
    estimate_generation_cost, estimate_token_cost, GenerationCostReason, GenerationCostUnit,
    TokenUsage, GENERATION_COST_RECORD_LIMIT,
};
use open_canvas_core::media_size::clamp_video_seconds;
use open_canvas_core::node::{CanvasNodeData, CanvasNodeType};

/// `GenerationCostSource` (`generation-cost.ts:5`) as persisted wire strings.
pub const COST_SOURCE_API: &str = "api";
pub const COST_SOURCE_LOOKUP: &str = "lookup";
pub const COST_SOURCE_ESTIMATE: &str = "estimate";

/// One cost record (`GenerationCostRecord`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostRecord {
    pub id: u64,
    pub node_id: String,
    pub model: String,
    pub unit: String,
    pub quantity: f64,
    pub usd: f64,
    pub priced: bool,
    pub source: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub at: u64,
}

/// What a finished generation's response already told us about its cost.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CostHint {
    /// The response carried no cost information (or the caller has none).
    #[default]
    None,
    /// The provider's own billed amount (`usage.cost`) — source `api`.
    Api(f64),
    /// Token counts the response reported, for [`estimate_token_cost`].
    Tokens(TokenUsage),
}

/// The billing shape of one finished generation: the model that ran, the unit
/// and quantity it is billed by, and the fallback the estimate uses when the
/// provider reported no amount.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationCharge {
    pub model: String,
    pub unit: GenerationCostUnit,
    pub quantity: f64,
    pub fallback: CostHint,
}

/// `Number((value).toFixed(6))` — the web's rounding of a billed amount.
fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

/// The `(unit, quantity)` a finished generation of `node` is billed by: the
/// requested image count, the video seconds, or one call (text). `None` for
/// node kinds that never generate. The model comes from the request that ran,
/// not from the node, so a config-resolved model records too.
pub fn generation_charge(node: &CanvasNodeData) -> Option<(GenerationCostUnit, f64)> {
    let meta = node.metadata.as_ref();
    match node.node_type {
        CanvasNodeType::Image | CanvasNodeType::ImageStack | CanvasNodeType::ImageGeneration => {
            let count = meta.and_then(|meta| meta.count).map_or(1.0, |count| {
                f64::from(i32::try_from(count).unwrap_or(1).max(1))
            });
            Some((GenerationCostUnit::Image, count))
        }
        CanvasNodeType::Video | CanvasNodeType::VideoGeneration => {
            let seconds =
                clamp_video_seconds(meta.and_then(|meta| meta.seconds.as_deref()).unwrap_or("6"));
            Some((
                GenerationCostUnit::VideoSecond,
                f64::from(i32::try_from(seconds).unwrap_or(6)),
            ))
        }
        CanvasNodeType::Text => Some((GenerationCostUnit::Call, 1.0)),
        _ => None,
    }
}

/// The persisted list plus the id counter (`costs.json`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostLog {
    pub records: Vec<CostRecord>,
    #[serde(default = "default_next_id")]
    pub next_id: u64,
}

fn default_next_id() -> u64 {
    1
}

impl Default for CostLog {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            next_id: 1,
        }
    }
}

impl CostLog {
    /// Parses `costs.json`; malformed content is an error the caller reports,
    /// and an oversized list is trimmed to the cap (newest kept).
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut log: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        log.records.truncate(GENERATION_COST_RECORD_LIMIT);
        let max_id = log
            .records
            .iter()
            .map(|record| record.id)
            .max()
            .unwrap_or(0);
        if log.next_id <= max_id {
            log.next_id = max_id + 1;
        }
        Ok(log)
    }

    /// Prepends `record` (newest first) and evicts the oldest beyond the cap —
    /// the web's `prepend + slice(0, GENERATION_COST_RECORD_LIMIT)`.
    pub fn push(&mut self, mut record: CostRecord) {
        record.id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.records.insert(0, record);
        self.records.truncate(GENERATION_COST_RECORD_LIMIT);
    }

    /// Records a finished generation and returns the source the record landed
    /// on (`api` | `estimate`), mirroring the web's `resolveImageCost`: the
    /// provider's own billed amount wins, then a token-based estimate, then the
    /// model price table. `node_id` ties the record to the node that generated.
    pub fn record_generation(
        &mut self,
        node_id: &str,
        charge: &GenerationCharge,
        at: u64,
    ) -> &'static str {
        match charge.fallback {
            CostHint::Api(usd) if usd.is_finite() => {
                self.push(CostRecord {
                    id: 0,
                    node_id: node_id.to_string(),
                    model: charge.model.clone(),
                    unit: charge.unit.as_str().to_string(),
                    quantity: charge.quantity,
                    usd: round6(usd),
                    priced: true,
                    source: COST_SOURCE_API.to_string(),
                    reason: None,
                    at,
                });
                COST_SOURCE_API
            }
            CostHint::Tokens(usage) => {
                if let Some(estimate) = estimate_token_cost(&charge.model, usage) {
                    self.push(CostRecord {
                        id: 0,
                        node_id: node_id.to_string(),
                        model: charge.model.clone(),
                        unit: charge.unit.as_str().to_string(),
                        quantity: charge.quantity,
                        usd: estimate.usd,
                        priced: estimate.priced,
                        source: COST_SOURCE_ESTIMATE.to_string(),
                        reason: estimate.reason.map(reason_id).map(str::to_string),
                        at,
                    });
                    COST_SOURCE_ESTIMATE
                } else {
                    self.record_estimate(node_id, &charge.model, charge.unit, charge.quantity, at);
                    COST_SOURCE_ESTIMATE
                }
            }
            CostHint::None | CostHint::Api(_) => {
                self.record_estimate(node_id, &charge.model, charge.unit, charge.quantity, at);
                COST_SOURCE_ESTIMATE
            }
        }
    }

    /// Records an estimate for a generation (`recordGenerationCost` without an
    /// API cost): priced estimates land in the total, unpriced ones stay listed.
    pub fn record_estimate(
        &mut self,
        node_id: &str,
        model: &str,
        unit: GenerationCostUnit,
        quantity: f64,
        at: u64,
    ) {
        let estimate = estimate_generation_cost(model, unit, quantity);
        self.push(CostRecord {
            id: 0,
            node_id: node_id.to_string(),
            model: model.to_string(),
            unit: unit.as_str().to_string(),
            quantity,
            usd: estimate.usd,
            priced: estimate.priced,
            source: COST_SOURCE_ESTIMATE.to_string(),
            reason: estimate.reason.map(reason_id).map(str::to_string),
            at,
        });
    }

    /// Records an API-looked-up amount (`fetch_generation_cost` result).
    pub fn record_lookup(
        &mut self,
        model: &str,
        unit: GenerationCostUnit,
        quantity: f64,
        usd: f64,
        at: u64,
    ) {
        self.push(CostRecord {
            id: 0,
            node_id: String::new(),
            model: model.to_string(),
            unit: unit.as_str().to_string(),
            quantity,
            usd,
            priced: true,
            source: COST_SOURCE_LOOKUP.to_string(),
            reason: None,
            at,
        });
    }

    /// Records a finished generation's looked-up amount, keeping the node the
    /// record belongs to (the Cost tab's own lookup keeps
    /// [`CostLog::record_lookup`]).
    pub fn record_generation_lookup(
        &mut self,
        node_id: &str,
        model: &str,
        unit: GenerationCostUnit,
        quantity: f64,
        usd: f64,
        at: u64,
    ) {
        self.push(CostRecord {
            id: 0,
            node_id: node_id.to_string(),
            model: model.to_string(),
            unit: unit.as_str().to_string(),
            quantity,
            usd,
            priced: true,
            source: COST_SOURCE_LOOKUP.to_string(),
            reason: None,
            at,
        });
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Total of the priced records (the summary's "Total spend").
    pub fn total_usd(&self) -> f64 {
        self.records
            .iter()
            .filter(|record| record.priced)
            .map(|record| record.usd)
            .sum()
    }

    pub fn unpriced_count(&self) -> usize {
        self.records.iter().filter(|record| !record.priced).count()
    }
}

fn reason_id(reason: GenerationCostReason) -> &'static str {
    match reason {
        GenerationCostReason::UnpricedModel => "unpriced-model",
        GenerationCostReason::UnpricedUnit => "unpriced-unit",
    }
}

#[cfg(test)]
#[path = "cost_tests.rs"]
mod tests;

// `cost_source_tests.rs` stayed in `crates/app`: it asserts on the Slint row
// types `config_tabs::cost_rows` produces. Declared from `main.rs` instead.
