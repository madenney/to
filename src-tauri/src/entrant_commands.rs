use tauri::State;
use crate::types::{SharedEntrantManager, SharedLiveStartgg, SharedSetupStore, UnifiedEntrant};

/// Setup info with seed-based sorting
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupWithSeed {
    pub id: u32,
    pub name: String,
    pub assigned_entrant_ids: Vec<u32>,
    pub highest_seed: Option<u32>,
    pub is_available: bool,
}

/// Get all unified entrants sorted for display
#[tauri::command]
pub fn get_unified_entrants(
    entrant_manager: State<'_, SharedEntrantManager>,
) -> Result<Vec<UnifiedEntrant>, String> {
    let guard = entrant_manager.lock().map_err(|e| e.to_string())?;
    Ok(guard.get_sorted_for_display())
}

/// Set slippi code for an entrant (user edit)
#[tauri::command]
pub fn set_entrant_slippi_code(
    entrant_id: u32,
    code: Option<String>,
    entrant_manager: State<'_, SharedEntrantManager>,
) -> Result<(), String> {
    let mut guard = entrant_manager.lock().map_err(|e| e.to_string())?;
    guard.set_slippi_code(entrant_id, code)
}

/// Assign entrant to setup
#[tauri::command]
pub fn assign_entrant_to_setup(
    entrant_id: u32,
    setup_id: Option<u32>,
    entrant_manager: State<'_, SharedEntrantManager>,
) -> Result<(), String> {
    let mut guard = entrant_manager.lock().map_err(|e| e.to_string())?;
    guard.assign_to_setup(entrant_id, setup_id, false)
}

/// Unassign entrant from their current setup
#[tauri::command]
pub fn unassign_entrant(
    entrant_id: u32,
    entrant_manager: State<'_, SharedEntrantManager>,
) -> Result<(), String> {
    let mut guard = entrant_manager.lock().map_err(|e| e.to_string())?;
    guard.unassign(entrant_id)
}

/// Toggle auto-assignment
#[tauri::command]
pub fn toggle_auto_assignment(
    enabled: bool,
    entrant_manager: State<'_, SharedEntrantManager>,
) -> Result<(), String> {
    let mut guard = entrant_manager.lock().map_err(|e| e.to_string())?;
    guard.set_auto_assign_enabled(enabled);
    Ok(())
}

/// Get setups sorted by highest seed of assigned players
#[tauri::command]
pub fn get_setups_sorted_by_seed(
    entrant_manager: State<'_, SharedEntrantManager>,
    setup_store: State<'_, SharedSetupStore>,
) -> Result<Vec<SetupWithSeed>, String> {
    let entrant_guard = entrant_manager.lock().map_err(|e| e.to_string())?;
    let setup_guard = setup_store.lock().map_err(|e| e.to_string())?;

    let mut setups_with_seed: Vec<SetupWithSeed> = setup_guard.setups.iter()
        .map(|setup| {
            let assigned = entrant_guard.get_by_setup(setup.id);
            let assigned_ids: Vec<u32> = assigned.iter().map(|e| e.id).collect();
            let highest_seed = entrant_guard.highest_seed_for_setup(setup.id);

            SetupWithSeed {
                id: setup.id,
                name: setup.name.clone(),
                assigned_entrant_ids: assigned_ids,
                highest_seed,
                is_available: highest_seed.is_none(),
            }
        })
        .collect();

    // Sort by highest seed (lower is better), unassigned setups last
    setups_with_seed.sort_by(|a, b| {
        match (a.highest_seed, b.highest_seed) {
            (Some(seed_a), Some(seed_b)) => seed_a.cmp(&seed_b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.id.cmp(&b.id),
        }
    });

    Ok(setups_with_seed)
}

/// Get auto-assignment status
#[tauri::command]
pub fn get_auto_assignment_status(
    entrant_manager: State<'_, SharedEntrantManager>,
) -> Result<bool, String> {
    let guard = entrant_manager.lock().map_err(|e| e.to_string())?;
    Ok(guard.is_auto_assign_enabled())
}

/// Trigger manual auto-assignment run
#[tauri::command]
pub fn run_auto_assignment(
    entrant_manager: State<'_, SharedEntrantManager>,
    setup_store: State<'_, SharedSetupStore>,
) -> Result<Vec<(u32, u32)>, String> {
    let setup_guard = setup_store.lock().map_err(|e| e.to_string())?;
    let available_setups: Vec<u32> = setup_guard.setups.iter().map(|s| s.id).collect();
    drop(setup_guard);

    let mut entrant_guard = entrant_manager.lock().map_err(|e| e.to_string())?;
    Ok(entrant_guard.auto_assign(&available_setups))
}

/// Sync entrant manager from current Start.gg state
#[tauri::command]
pub fn sync_entrants_from_startgg(
    entrant_manager: State<'_, SharedEntrantManager>,
    live_startgg: State<'_, SharedLiveStartgg>,
) -> Result<usize, String> {
    let startgg_guard = live_startgg.lock().map_err(|e| e.to_string())?;
    let state = startgg_guard.state.clone();
    drop(startgg_guard);

    if let Some(ref state) = state {
        let mut entrant_guard = entrant_manager.lock().map_err(|e| e.to_string())?;
        entrant_guard.update_from_startgg(state);
        Ok(entrant_guard.get_all().len())
    } else {
        Ok(0)
    }
}
