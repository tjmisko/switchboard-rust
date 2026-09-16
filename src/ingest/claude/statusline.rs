//! The Claude status line payload -> a limits snapshot and a beat.
//!
//! Provided code. It runs on every render, carries account-wide limits, and
//! is a snapshot source, so everything it yields is `Snapshot` confidence.
//!
//! `rate_limits` arrives in one of two shapes, and the adapter reads both:
//!
//! - **per window** (2.1.269/2.1.270, 543 of 552 captured payloads):
//!   `{ five_hour: { used_percentage, resets_at }, seven_day: { … } }`, with
//!   `seven_day` alone on six of them and an empty object or `null` on nine.
//!   No status is reported, so one is derived from the fullest window.
//! - **documented** `{ status, resetsAt, rateLimitType, utilization }`. Never
//!   captured, but it is the shape the harness documents, so an older or
//!   newer build may send it. It is the richer evidence when present: a
//!   reported `rejected` beats a percentage.

use serde_json::Value;

use crate::ingest::claude::transcript::limit_window;
use crate::ingest::json::{at, str_at, str_path, u64_at};
use crate::ingest::{Ctx, Input, frame::StatuslineFrame};
use crate::model::{AccountId, Beat, LimitStatus, Limits, Snapshot, SnapshotKind};
use crate::time::from_epoch_secs;

/// The windows a per-window payload may carry, in the order they are looked
/// up. The name doubles as the `rateLimitType` vocabulary.
const WINDOWS: [&str; 5] = [
    "five_hour",
    "seven_day",
    "seven_day_opus",
    "seven_day_sonnet",
    "overage",
];

/// A percentage at or above this is a warning; at or above 100 it is a
/// rejection. Both are derived, so they carry `Confidence::Snapshot`.
const WARNING_PERCENT: u8 = 80;

pub fn statusline(frame: &StatuslineFrame, account: &AccountId, ctx: &Ctx) -> Vec<Input> {
    let p = &frame.payload;
    let snapshot = |kind| {
        Input::Snapshot(Snapshot {
            taken: ctx.now,
            kind,
        })
    };

    let limits = p
        .get("rate_limits")
        .and_then(|r| documented_limits(r).or_else(|| windowed_limits(r)))
        .map(|value| {
            snapshot(SnapshotKind::Limits {
                account: account.clone(),
                value,
            })
        });

    let model = str_path(p, &["model", "id"])
        .or_else(|| str_at(p, "model"))
        .unwrap_or("");
    let beat = at(p, &["cost", "total_api_duration_ms"])
        .and_then(Value::as_u64)
        .map(|api_ms| {
            let context_tokens = at(p, &["context_window", "total_input_tokens"])
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0);
            snapshot(SnapshotKind::Beat {
                session: frame.session.clone(),
                value: Beat {
                    model: model.into(),
                    context_tokens,
                    api_ms,
                },
            })
        });

    limits.into_iter().chain(beat).collect()
}

/// The documented shape, keyed on `status`. `None` when the payload does not
/// carry one, or carries a word this crate does not know.
fn documented_limits(r: &Value) -> Option<Limits> {
    let status = match str_at(r, "status")? {
        "allowed" => LimitStatus::Allowed,
        "allowed_warning" => LimitStatus::Warning,
        "rejected" => LimitStatus::Rejected,
        _ => return None,
    };
    Some(Limits {
        status,
        resets_at: u64_at(r, "resetsAt").and_then(from_epoch_secs),
        window: str_at(r, "rateLimitType").and_then(limit_window),
        used_percent: r.get("utilization").and_then(Value::as_f64).map(percent),
    })
}

/// The per-window shape. The window closest to its cap is the one that will
/// stop the agent, so it decides the status and supplies the reset time.
/// `None` when no window carries a `used_percentage`.
fn windowed_limits(r: &Value) -> Option<Limits> {
    let (name, window, used) = WINDOWS
        .iter()
        .filter_map(|name| r.get(*name).map(|w| (*name, w)))
        .filter_map(|(name, w)| Some((name, w, used_percentage(w)?)))
        .max_by_key(|(_, _, used)| *used)?;
    let status = if used >= 100 {
        LimitStatus::Rejected
    } else if used >= WARNING_PERCENT {
        LimitStatus::Warning
    } else {
        LimitStatus::Allowed
    };
    Some(Limits {
        status,
        resets_at: u64_at(window, "resets_at").and_then(from_epoch_secs),
        window: limit_window(name),
        used_percent: Some(used),
    })
}

/// `used_percentage` is already a percentage: `1` means one percent, so the
/// fraction heuristic in `percent` must not touch it.
fn used_percentage(window: &Value) -> Option<u8> {
    window
        .get("used_percentage")
        .and_then(Value::as_f64)
        .map(|u| u.round().clamp(0.0, 100.0) as u8)
}

/// `utilization` is a fraction on some windows and a percentage on others.
fn percent(u: f64) -> u8 {
    let pct = if u <= 1.0 { u * 100.0 } else { u };
    pct.round().clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn should_derive_the_status_from_the_fullest_window_when_no_status_is_reported() {
        let r = json!({
            "five_hour": {"used_percentage": 12, "resets_at": 10},
            "seven_day": {"used_percentage": 81, "resets_at": 20}
        });
        let l = windowed_limits(&r).unwrap();
        assert_eq!(l.status, LimitStatus::Warning);
        assert_eq!(l.used_percent, Some(81));
        assert_eq!(l.window, Some(crate::model::LimitWindow::SevenDay));
        assert_eq!(l.resets_at, from_epoch_secs(20));
    }

    #[test]
    fn should_reject_at_one_hundred_percent_and_yield_nothing_for_an_empty_object() {
        let full = json!({"five_hour": {"used_percentage": 100}});
        assert_eq!(
            windowed_limits(&full).unwrap().status,
            LimitStatus::Rejected
        );
        assert_eq!(windowed_limits(&json!({})), None);
        assert_eq!(windowed_limits(&json!({"five_hour": {}})), None);
    }

    #[test]
    fn should_prefer_a_reported_status_over_a_derived_one() {
        let r = json!({
            "status": "rejected",
            "five_hour": {"used_percentage": 3}
        });
        let l = documented_limits(&r)
            .or_else(|| windowed_limits(&r))
            .unwrap();
        assert_eq!(l.status, LimitStatus::Rejected);
    }
}
