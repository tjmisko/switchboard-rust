//! Snapshots: stale-able, self-repairing, replacing a whole scope.
//!
//! The one rule that makes mixing them with events safe: a snapshot never
//! overwrites something an event established after the snapshot was taken.
//! `AsOf::supersedes` is that rule; each scope below says which event it is
//! checked against.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::model::{
    Activity, Agent, AsOf, Board, Cause, Compaction, Confidence, Effect, HistoryEntry, Life, Phase,
    Placement, ProcessRow, Prompts, Shown, Snapshot, SnapshotKind, Subagent, SubagentRow,
    SubagentSource, Subagents, Wait, WaitRow,
};
use crate::paths::workspace_name;
use crate::reduce::{Effects, WAIT_EXPIRY, default_account, limits_for, settle};

pub fn apply_snapshot(board: &mut Board, snapshot: Snapshot) -> Vec<Effect> {
    let taken = snapshot.taken;
    let mut effects = Effects::new();

    match snapshot.kind {
        SnapshotKind::Processes(rows) => {
            for row in rows {
                discover(board, row, taken, &mut effects);
            }
        }

        SnapshotKind::Limits { account, value } => {
            todo!("T09: newer-or-equal only; unchanged publishes nothing; re-settle the account")
        }

        SnapshotKind::Beat { session, value } => {
            let Some(agent) = board
                .agents
                .values_mut()
                .find(|a| a.session.as_ref() == Some(&session))
            else {
                return effects.finish();
            };
            let newer = agent.beat.as_ref().is_none_or(|b| b.as_of <= taken);
            if newer {
                agent.beat = Some(AsOf {
                    value,
                    as_of: taken,
                });
            }
        }

        SnapshotKind::Subagents { session, rows } => {
            todo!("T09: find the agent by session; drop if not `AsOf::supersedes` its last event")
        }

        SnapshotKind::Waits { session, rows } => {
            // No event creates or ends a wait, so there is nothing for a
            // `Stop` snapshot to be stale against: it replaces outright.
            todo!("T09")
        }

        SnapshotKind::Placements(rows) => {
            for row in rows {
                if let Some(agent) = board.agents.get_mut(&row.process)
                    && agent.placement != row.placement
                {
                    agent.placement = row.placement;
                    effects.publish_if(true);
                }
            }
        }
    }

    effects.finish()
}

/// A new agent, as a sweep first sees it.
pub fn discovered(row: ProcessRow, now: Instant) -> Agent {
    Agent {
        process: row.key,
        provider: row.provider,
        life: Life::Alive,
        session: row.session,
        account: default_account(),
        title: None,
        workspace: workspace_name(&row.cwd).map(Box::from),
        cwd: row.cwd,
        placement: Placement::Unknown,
        cpu_ticks: row.cpu_ticks,
        activity: Activity::Unobserved { since: now },
        prompts: Prompts::default(),
        compaction: Compaction::None,
        waits: BTreeMap::new(),
        subagents: Subagents::default(),
        beat: None,
        shown: Shown::new(Phase::Unknown, now),
        last_event: None,
    }
}

fn discover(board: &mut Board, row: ProcessRow, taken: Instant, effects: &mut Effects) {
    if let Some(agent) = board.agents.get_mut(&row.key) {
        agent.cpu_ticks = row.cpu_ticks;
        // A sweep may name the session before any hook does.
        if let (None, Some(session)) = (&agent.session, row.session) {
            effects.push(Effect::WatchTranscript {
                process: agent.process,
                session: session.clone(),
                cwd: agent.cwd.clone(),
            });
            agent.session = Some(session);
            effects.publish_if(true);
        }
        return;
    }

    let agent = discovered(row, taken);
    effects.push(Effect::AppendHistory(HistoryEntry::AgentFound {
        process: agent.process,
        cwd: agent.cwd.clone(),
    }));
    if let Some(session) = &agent.session {
        effects.push(Effect::WatchTranscript {
            process: agent.process,
            session: session.clone(),
            cwd: agent.cwd.clone(),
        });
    }
    board.agents.insert(agent.process, agent);
}

/// The panel replaces the table. What an event learned about a row that is
/// still present — its gate — is kept; a row that is absent is gone.
fn replace_panel(old: &Subagents, rows: Vec<SubagentRow>, taken: Instant) -> Subagents {
    todo!("T09")
}

fn replace_waits(
    mut old: BTreeMap<crate::model::WaitId, Wait>,
    rows: Vec<WaitRow>,
    taken: Instant,
) -> BTreeMap<crate::model::WaitId, Wait> {
    todo!("T09")
}
