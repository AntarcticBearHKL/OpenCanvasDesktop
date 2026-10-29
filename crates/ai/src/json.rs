//! Small `serde_json::Value` → number coercion shared by the generation-cost and
//! account parsers. It mirrors the web's `Number(value)` reads
//! (`toAmount` / `toNullableAmount` in `openrouter-account.ts`,
//! `Number.isFinite(Number(v))` in `image.ts`): numbers, numeric strings, and
//! booleans coerce; `null`, objects, arrays, and non-numeric strings do not.

use serde_json::Value;

/// [`Number`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number)
/// limited to finite results; `None` when the value is absent or not numeric.
pub(crate) fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|amount| amount.is_finite()),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|amount| amount.is_finite()),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        _ => None,
    }
}
