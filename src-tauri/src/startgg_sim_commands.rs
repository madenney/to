use crate::config::*;
use crate::types::*;
use crate::startgg::{init_startgg_sim, load_startgg_sim_config, load_startgg_sim_config_from};
use crate::replay::{replay_winner_identity, set_slot_index_for_identity, tag_from_code, next_reference_step_scores};
use crate::startgg_sim::{StartggSim, StartggSimState};
use serde_json::Value;
use std::path::PathBuf;
use tauri::State;

// ── Helpers ─────────────────────────────────────────────────────────────

/// Lock the mutex, initialize the sim if needed, then call `f` with `(&mut StartggSim, now_ms)`.
fn with_sim<F, R>(test_state: &State<'_, SharedTestState>, f: F) -> Result<R, String>
where
    F: FnOnce(&mut StartggSim, u64) -> Result<R, String>,
{
    let now = now_ms();
    let mut guard = test_state.lock().map_err(|e| e.to_string())?;
    init_startgg_sim(&mut guard, now)?;
    let sim = guard.startgg_sim.as_mut()
        .ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
    f(sim, now)
}

/// Lock the mutex, then call `f` with `(&mut TestModeState, now_ms)` — for reset
/// commands that bypass init and create a new sim.
fn with_test_state<F, R>(test_state: &State<'_, SharedTestState>, f: F) -> Result<R, String>
where
    F: FnOnce(&mut TestModeState, u64) -> Result<R, String>,
{
    let now = now_ms();
    let mut guard = test_state.lock().map_err(|e| e.to_string())?;
    f(&mut guard, now)
}

fn check_test_mode() -> Result<(), String> {
    if !app_test_mode_enabled() {
        return Err("Test mode is disabled in settings.".to_string());
    }
    Ok(())
}

// ── Commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn startgg_sim_state(
    since_ms: Option<u64>,
    test_state: State<'_, SharedTestState>,
) -> Result<StartggSimState, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| Ok(sim.state_since(now, since_ms)))
}

#[tauri::command]
pub fn startgg_sim_reset(
    config_path: Option<String>,
    test_state: State<'_, SharedTestState>,
) -> Result<StartggSimState, String> {
    check_test_mode()?;
    with_test_state(&test_state, |guard, now| {
        let resolved_path = config_path
            .as_deref()
            .map(resolve_startgg_sim_config_path);
        let config = if let Some(path) = resolved_path.clone().or_else(|| guard.startgg_config_path.clone()) {
            load_startgg_sim_config_from(&path)?
        } else {
            load_startgg_sim_config()?
        };
        if resolved_path.is_some() {
            guard.startgg_config_path = resolved_path;
        }
        guard.startgg_sim = Some(StartggSim::new(config, now)?);
        let sim = guard.startgg_sim.as_mut()
            .ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
        Ok(sim.state(now))
    })
}

#[tauri::command]
pub fn startgg_sim_advance_set(set_id: u64, test_state: State<'_, SharedTestState>) -> Result<StartggSimState, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        sim.advance_set(set_id, now)?;
        Ok(sim.state(now))
    })
}

#[tauri::command]
pub fn startgg_sim_force_winner(
    set_id: u64,
    winner_slot: u8,
    test_state: State<'_, SharedTestState>,
) -> Result<StartggSimState, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        sim.force_winner(set_id, winner_slot as usize, now)?;
        Ok(sim.state(now))
    })
}

#[tauri::command]
pub fn startgg_sim_mark_dq(
    set_id: u64,
    dq_slot: u8,
    test_state: State<'_, SharedTestState>,
) -> Result<StartggSimState, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        sim.mark_dq(set_id, dq_slot as usize, now)?;
        Ok(sim.state(now))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_state(
    since_ms: Option<u64>,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| Ok(sim.raw_response(now, since_ms)))
}

#[tauri::command]
pub fn startgg_sim_raw_reset(
    config_path: Option<String>,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_test_state(&test_state, |guard, now| {
        let resolved_path = config_path
            .as_deref()
            .map(resolve_startgg_sim_config_path);
        let config = if let Some(path) = resolved_path.clone().or_else(|| guard.startgg_config_path.clone()) {
            load_startgg_sim_config_from(&path)?
        } else {
            load_startgg_sim_config()?
        };
        if resolved_path.is_some() {
            guard.startgg_config_path = resolved_path;
        }
        guard.startgg_sim = Some(StartggSim::new(config, now)?);
        let sim = guard.startgg_sim.as_mut()
            .ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_advance_set(
    set_id: u64,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        sim.advance_set(set_id, now)?;
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_start_set(
    set_id: u64,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        sim.start_set_manual(set_id, now)?;
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_update_scores(
    set_id: u64,
    scores: Vec<u8>,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    if scores.len() != 2 {
        return Err("Scores must include exactly two values.".to_string());
    }
    with_sim(&test_state, |sim, now| {
        sim.update_set_scores_manual(set_id, [scores[0], scores[1]], now)?;
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_apply_replay_result(
    set_id: u64,
    replay_path: String,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    let replay_path = replay_path.trim().to_string();
    if replay_path.is_empty() {
        return Err("Replay path is empty.".to_string());
    }
    let mut resolved = PathBuf::from(&replay_path);
    if !resolved.is_absolute() {
        resolved = resolve_repo_path(&replay_path);
    }
    if !resolved.is_file() {
        return Err(format!("Replay not found at {}", resolved.display()));
    }

    let (winner_code, winner_tag) = replay_winner_identity(&resolved)?;
    let winner_tag = winner_tag.or_else(|| winner_code.as_deref().map(tag_from_code));

    with_sim(&test_state, |sim, now| {
        let state_snapshot = sim.state(now);
        let set = state_snapshot
            .sets
            .iter()
            .find(|candidate| candidate.id == set_id)
            .ok_or_else(|| "Set not found.".to_string())?;
        let winner_slot = set_slot_index_for_identity(
            set,
            winner_code.as_deref(),
            winner_tag.as_deref(),
        )
        .ok_or_else(|| "Winner not found in set slots.".to_string())?;

        let current_scores = [
            set.slots.get(0).and_then(|slot| slot.score).unwrap_or(0),
            set.slots.get(1).and_then(|slot| slot.score).unwrap_or(0),
        ];
        let mut next_scores = current_scores;
        if winner_slot < 2 {
            next_scores[winner_slot] = next_scores[winner_slot].saturating_add(1);
        }
        sim.update_set_scores_manual(
            set_id,
            [next_scores[0] as u8, next_scores[1] as u8],
            now,
        )?;
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_step_set(
    set_id: u64,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        let outcome = sim
            .reference_outcome_for_set(set_id)
            .ok_or_else(|| "No reference outcome found for this set.".to_string())?;
        if let Some(dq_slot) = outcome.dq_slot {
            sim.mark_dq(set_id, dq_slot, now)?;
            return Ok(sim.raw_response(now, None));
        }
        let snapshot = sim.state(now);
        let set = snapshot
            .sets
            .iter()
            .find(|candidate| candidate.id == set_id)
            .ok_or_else(|| "Set not found.".to_string())?;
        if set.state == "completed" || set.state == "skipped" {
            return Ok(sim.raw_response(now, None));
        }
        let current_scores = [
            set.slots.get(0).and_then(|slot| slot.score).unwrap_or(0),
            set.slots.get(1).and_then(|slot| slot.score).unwrap_or(0),
        ];
        let target_scores = outcome.scores;
        if current_scores[0] >= target_scores[0] && current_scores[1] >= target_scores[1] {
            sim.finish_set_manual(set_id, outcome.winner_slot, target_scores, now)?;
            return Ok(sim.raw_response(now, None));
        }
        let Some(next_scores) =
            next_reference_step_scores(current_scores, target_scores, outcome.winner_slot)
        else {
            return Ok(sim.raw_response(now, None));
        };
        if next_scores == target_scores {
            sim.finish_set_manual(set_id, outcome.winner_slot, target_scores, now)?;
        } else {
            sim.update_set_scores_manual(set_id, [next_scores[0], next_scores[1]], now)?;
        }
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_finalize_reference_set(
    set_id: u64,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        let outcome = sim
            .reference_outcome_for_set(set_id)
            .ok_or_else(|| "No reference outcome found for this set.".to_string())?;
        if let Some(dq_slot) = outcome.dq_slot {
            sim.mark_dq(set_id, dq_slot, now)?;
        } else {
            sim.finish_set_manual(set_id, outcome.winner_slot, outcome.scores, now)?;
        }
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_finish_set(
    set_id: u64,
    winner_slot: u8,
    scores: Vec<u8>,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    if scores.len() != 2 {
        return Err("Scores must include exactly two values.".to_string());
    }
    with_sim(&test_state, |sim, now| {
        sim.finish_set_manual(set_id, winner_slot as usize, [scores[0], scores[1]], now)?;
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_complete_bracket(
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        if sim.has_reference_sets() {
            sim.complete_from_reference(now)?;
        } else {
            sim.complete_all_sets(now)?;
        }
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_force_winner(
    set_id: u64,
    winner_slot: u8,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        sim.force_winner(set_id, winner_slot as usize, now)?;
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_mark_dq(
    set_id: u64,
    dq_slot: u8,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        sim.mark_dq(set_id, dq_slot as usize, now)?;
        Ok(sim.raw_response(now, None))
    })
}

#[tauri::command]
pub fn startgg_sim_raw_reset_set(
    set_id: u64,
    test_state: State<'_, SharedTestState>,
) -> Result<Value, String> {
    check_test_mode()?;
    with_sim(&test_state, |sim, now| {
        sim.reset_set_and_dependents(set_id, now)?;
        Ok(sim.raw_response(now, None))
    })
}
