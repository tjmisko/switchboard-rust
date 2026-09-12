//! The Claude status line payload -> a limits snapshot and a beat.
//!
//! Provided code. It runs on every render, carries account-wide limits, and
//! is a snapshot source, so everything it yields is `Snapshot` confidence.

use serde_json::Value;

use crate::ingest::claude::transcript::limit_window;
use crate::ingest::json::{at, str_at, str_path, u64_at};
use crate::ingest::{Ctx, Input, frame::StatuslineFrame};
use crate::model::{AccountId, Beat, LimitStatus, Limits, Snapshot, SnapshotKind};
use crate::time::from_epoch_secs;

pub fn statusline(frame: &StatuslineFrame, account: &AccountId, ctx: &Ctx) -> Vec<Input> {
    let p = &frame.payload;
    let snapshot = |kind| {
        Input::Snapshot(Snapshot {
            taken: ctx.now,
            kind,
        })
    };

    let limits = p.get("rate_limits").and_then(|r| {
        let status = match str_at(r, "status")? {
            "allowed" => LimitStatus::Allowed,
            "allowed_warning" => LimitStatus::Warning,
            "rejected" => LimitStatus::Rejected,
            _ => return None,
        };
        Some(snapshot(SnapshotKind::Limits {
            account: account.clone(),
            value: Limits {
                status,
                resets_at: u64_at(r, "resetsAt").and_then(from_epoch_secs),
                window: str_at(r, "rateLimitType").and_then(limit_window),
                used_percent: r.get("utilization").and_then(Value::as_f64).map(percent),
            },
        }))
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

/// `utilization` is a fraction on some windows and a percentage on others.
fn percent(u: f64) -> u8 {
    let pct = if u <= 1.0 { u * 100.0 } else { u };
    pct.round().clamp(0.0, 100.0) as u8
}
