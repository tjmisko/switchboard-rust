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
            let slot = board.accounts.entry(account.clone()).or_default();
            let newer = slot.limits.as_ref().is_none_or(|old| old.as_of <= taken);
            if !newer || slot.limits.as_ref().map(|l| &l.value) == Some(&value) {
                return effects.finish();
            }
            slot.limits = Some(AsOf {
                value,
                as_of: taken,
            });
            effects.publish_if(true);

            // Account-wide: every agent on the account may have moved.
            let Board { agents, accounts } = board;
            for agent in agents.values_mut().filter(|a| a.account == account) {
                let limits = limits_for(accounts, agent);
                effects.history(settle(
                    agent,
                    limits,
                    Cause::Agent,
                    Confidence::Snapshot,
                    taken,
                ));
            }
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
            let Board { agents, accounts } = board;
            let Some(agent) = agents
                .values_mut()
                .find(|a| a.session.as_ref() == Some(&session))
            else {
                return effects.finish();
            };
            let fresh = AsOf {
                value: (),
                as_of: taken,
            };
            if !fresh.supersedes(agent.last_event) {
                // Taken before an event we have already applied. It may show a
                // subagent that `SubagentStop` has since closed.
                return effects.finish();
            }
            agent.subagents = replace_panel(&agent.subagents, rows, taken);
            effects.publish_if(true);
            let limits = limits_for(accounts, agent);
            effects.history(settle(
                agent,
                limits,
                Cause::Agent,
                Confidence::Snapshot,
                taken,
            ));
        }

        SnapshotKind::Waits { session, rows } => {
            let Board { agents, accounts } = board;
            let Some(agent) = agents
                .values_mut()
                .find(|a| a.session.as_ref() == Some(&session))
            else {
                return effects.finish();
            };
            // No event creates or ends a wait, so there is nothing for a
            // `Stop` snapshot to be stale against: it replaces outright.
            let old = std::mem::take(&mut agent.waits);
            agent.waits = replace_waits(old, rows, taken);
            effects.publish_if(true);
            let limits = limits_for(accounts, agent);
            effects.history(settle(
                agent,
                limits,
                Cause::Agent,
                Confidence::Snapshot,
                taken,
            ));
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
        title: row.name,
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
        // The session file's `name` and a hook's `session_title` report the
        // same fact, so the most recent reading wins: a rename lands on the
        // next sweep. Absent means the file did not say, not that the name
        // went away.
        if row.name.is_some() && agent.title != row.name {
            agent.title = row.name;
            effects.publish_if(true);
        }
        // A sweep may name the session before any hook does.
        if let (None, Some(session)) = (&agent.session, row.session) {
            effects.watch(agent, &session);
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
        effects.watch(&agent, session);
    }
    board.agents.insert(agent.process, agent);
}

/// The panel replaces the table. What an event learned about a row that is
/// still present — its gate — is kept; a row that is absent is gone.
fn replace_panel(old: &Subagents, rows: Vec<SubagentRow>, taken: Instant) -> Subagents {
    let rows = rows
        .into_iter()
        .map(|row| {
            let previous = old.rows.get(&row.subagent);
            let subagent = Subagent {
                kind: row.kind,
                status: row.status,
                parent: previous.and_then(|p| p.parent.clone()),
                started: row.started,
                label: row.label,
                tokens: row.tokens,
                gate: previous.and_then(|p| p.gate.clone()),
                last_seen: taken,
            };
            (row.subagent, subagent)
        })
        .collect();
    Subagents {
        rows,
        as_of: Some(taken),
        source: SubagentSource::Panel,
    }
}

fn replace_waits(
    mut old: BTreeMap<crate::model::WaitId, Wait>,
    rows: Vec<WaitRow>,
    taken: Instant,
) -> BTreeMap<crate::model::WaitId, Wait> {
    rows.into_iter()
        .map(|row| {
            // `remove` hands back ownership of the old entry, so its `armed`
            // time survives without a clone.
            let armed = old.remove(&row.id).map_or(taken, |w| w.armed);
            let wait = Wait {
                kind: row.kind,
                armed,
                expected_by: None,
                expires: taken + WAIT_EXPIRY,
            };
            (row.id, wait)
        })
        .collect()
}
