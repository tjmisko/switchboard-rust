//! T06 — usage: operator traits and borrowed map keys, then pricing.

mod common;

use std::collections::BTreeMap;

use serde_json::{Value, json};

use common::fixture;
use switchboard::usage::{
    Rates, Usage, parse_price, rates_for, response, transcript_cost, usage_by_model,
};

fn lines() -> Vec<Value> {
    std::fs::read_to_string(fixture("claude/transcript/session.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

// ---------------------------------------------------------------------------
// Core: one response, many lines
// ---------------------------------------------------------------------------

#[test]
fn should_read_a_response_key_from_message_id_or_request_id() {
    let v = json!({"type":"assistant","requestId":"req_1","message":{"model":"m","usage":{"output_tokens":5}}});
    let r = response(&v).unwrap();
    assert_eq!(r.key, "req_1");
    assert_eq!(r.usage.output, 5);
    assert_eq!(r.usage.input, 0);
    assert!(response(&json!({"type":"user"})).is_none());
}

#[test]
fn should_count_a_response_split_across_lines_once() {
    let lines = lines();
    let by_model = usage_by_model(&lines);
    let opus = by_model["claude-opus-5"];
    // msg_0001 is two lines with the same usage; msg_0002 and msg_0004 are one each.
    assert_eq!(opus.input, 2 + 1 + 5);
    assert_eq!(opus.output, 592 + 14 + 40);
    assert_eq!(opus.cache_write, 33893 + 120);
    assert_eq!(opus.cache_read, 30711 + 64604);
    assert_eq!(by_model["claude-fable-5-1"].output, 80);
    assert!(!by_model.contains_key("<synthetic>"));
    assert_eq!(by_model.len(), 2);
}

#[test]
fn should_sum_usages_with_the_operators() {
    let a = Usage {
        input: 1,
        output: 2,
        cache_read: 3,
        cache_write: 4,
    };
    let b = Usage {
        input: 10,
        output: 20,
        cache_read: 30,
        cache_write: 40,
    };
    assert_eq!((a + b).total(), 110);
    let total: Usage = [a, b, a].into_iter().sum();
    assert_eq!(total.input, 12);
    let mut c = a;
    c += b;
    assert_eq!(c, a + b);
}

// ---------------------------------------------------------------------------
// Stretch: pricing
// ---------------------------------------------------------------------------

#[test]
fn should_parse_prices_without_floats() {
    assert_eq!(parse_price("$6.25 / MTok"), Some(6_250_000));
    assert_eq!(parse_price("$0.50 / MTok"), Some(500_000));
    assert_eq!(parse_price("5"), Some(5_000_000));
    assert_eq!(parse_price(" $25 "), Some(25_000_000));
    assert_eq!(parse_price("$0.000001"), Some(1));
    assert_eq!(
        parse_price("$0.0000001"),
        None,
        "more precision than a micro-dollar"
    );
    assert_eq!(parse_price("$.5"), None);
    assert_eq!(parse_price("$1.2.3"), None);
    assert_eq!(parse_price("$-1"), None);
    assert_eq!(parse_price("$+1"), None);
    assert_eq!(parse_price("free"), None);
    assert_eq!(parse_price(""), None);
}

#[test]
fn should_price_usage_in_micro_dollars_without_overflow() {
    let rates = Rates {
        input: 5_000_000,
        output: 25_000_000,
        cache_read: 500_000,
        cache_write: 6_250_000,
    };
    let usage = Usage {
        input: 1_000_000,
        output: 2_000,
        cache_read: 0,
        cache_write: 0,
    };
    assert_eq!(rates.cost_micros(&usage), 5_000_000 + 50_000);
    let huge = Usage {
        input: u64::MAX,
        output: 0,
        cache_read: 0,
        cache_write: 0,
    };
    assert_eq!(rates.cost_micros(&huge), u64::MAX, "saturates");
}

#[test]
fn should_fall_back_to_the_base_model_when_a_context_suffix_is_unpriced() {
    let mut table = BTreeMap::new();
    let r = Rates {
        input: 1,
        output: 1,
        cache_read: 1,
        cache_write: 1,
    };
    table.insert("claude-opus-5".to_string(), r);
    assert_eq!(rates_for(&table, "claude-opus-5"), Some(&r));
    assert_eq!(rates_for(&table, "claude-opus-5[1m]"), Some(&r));
    assert_eq!(rates_for(&table, "claude-fable-5-1"), None);
}

#[test]
fn should_report_unpriced_models_instead_of_silently_costing_zero() {
    let lines = lines();
    let mut table = BTreeMap::new();
    table.insert(
        "claude-opus-5".to_string(),
        Rates {
            input: 5_000_000,
            output: 25_000_000,
            cache_read: 500_000,
            cache_write: 6_250_000,
        },
    );
    let (total, unpriced) = transcript_cost(&lines, &table);
    assert!(total > 0);
    assert_eq!(unpriced, ["claude-fable-5-1"]);
}
