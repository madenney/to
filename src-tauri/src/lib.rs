pub mod types;
pub mod config;
pub mod replay;
pub mod dolphin;
pub mod startgg;
pub mod test_mode;
pub mod slippi;
pub mod startgg_sim_commands;
pub mod entrants;
pub mod entrant_commands;
mod startgg_sim;

use types::*;
use config::*;
use startgg::init_startgg_sim;
use config::normalize_slippi_code;
use replay::{
    build_overlay_state, is_replay_file_path, replay_slots_from_file,
};
use entrants::EntrantManager;

use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};
use axum::{
    extract::State as AxumState,
    response::IntoResponse,
    routing::{get, get_service},
    Router,
};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
use tauri::{path::BaseDirectory, Manager, State};
use tracing::{info, error};
use tracing_subscriber::EnvFilter;

// ── Setup CRUD commands ────────────────────────────────────────────────

#[tauri::command]
fn list_setups_stub() -> SetupsPayload {
    SetupsPayload {
        setups: vec![
            SetupStub { id: 1, name: "Setup 1".into(), note: Some("Waiting for wiring".into()) },
            SetupStub { id: 2, name: "Setup 2".into(), note: Some("Use this one for dev".into()) },
            SetupStub { id: 3, name: "Setup 3".into(), note: None },
            SetupStub { id: 4, name: "Setup 4".into(), note: None },
        ],
    }
}

#[tauri::command]
fn list_setups(store: State<'_, SharedSetupStore>) -> Result<Vec<Setup>, String> {
    let guard = store.lock().map_err(|e| e.to_string())?;
    Ok(guard.setups.clone())
}

#[tauri::command]
fn create_setup(store: State<'_, SharedSetupStore>) -> Result<Setup, String> {
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    if guard.setups.len() >= MAX_SETUP_COUNT {
        return Err(format!("Max setups ({MAX_SETUP_COUNT}) reached."));
    }
    let used: HashSet<u32> = guard.setups.iter().map(|s| s.id).collect();
    let mut setup_id: Option<u32> = None;
    for id in 1..=MAX_SETUP_COUNT as u32 {
        if !used.contains(&id) {
            setup_id = Some(id);
            break;
        }
    }
    let setup_id = setup_id.ok_or_else(|| "No setup slots available.".to_string())?;
    let setup = Setup {
        id: setup_id,
        name: format!("Setup {setup_id}"),
        assigned_stream: None,
    };
    guard.setups.push(setup.clone());
    guard.setups.sort_by_key(|s| s.id);
    Ok(setup)
}

#[tauri::command]
fn delete_setup(id: u32, store: State<'_, SharedSetupStore>) -> Result<(), String> {
    let (existing, existing_pid) = {
        let mut guard = store.lock().map_err(|e| e.to_string())?;
        guard.setups.retain(|s| s.id != id);
        guard.setups.sort_by_key(|s| s.id);
        (
            guard.processes.remove(&id),
            guard.process_pids.remove(&id),
        )
    };
    if let Some(child) = existing {
        dolphin::stop_dolphin_child(child)?;
    }
    if let Some(pid) = existing_pid {
        dolphin::stop_process_by_pid(pid)?;
    }
    Ok(())
}

// ── Bracket replay management commands ─────────────────────────────────

#[tauri::command]
fn list_bracket_configs() -> Result<Vec<BracketConfigInfo>, String> {
    let dir = startgg_sim_configs_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("read bracket dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("bracket")
            .to_string();
        let rel = path
            .strip_prefix(repo_root())
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        out.push(BracketConfigInfo { name, path: rel });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[tauri::command]
fn list_bracket_set_replay_paths(config_path: String, set_id: u64) -> Result<Vec<String>, String> {
    let paths = startgg::read_bracket_set_replay_paths(&config_path, set_id)?;
    Ok(paths
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect())
}

#[tauri::command]
fn list_bracket_replay_sets(config_path: String) -> Result<Vec<u64>, String> {
    let resolved = resolve_startgg_sim_config_path(&config_path);
    if !resolved.is_file() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&resolved)
        .map_err(|e| format!("read bracket config {}: {e}", resolved.display()))?;
    let value: Value = serde_json::from_str(&data)
        .map_err(|e| format!("parse bracket config {}: {e}", resolved.display()))?;

    let mut out = Vec::new();
    if let Some(sets) = value
        .get("referenceReplayMap")
        .and_then(|map| map.get("sets"))
        .and_then(|sets| sets.as_array())
    {
        for set in sets {
            let id = set.get("id").and_then(|v| v.as_u64());
            let replays = set.get("replays").and_then(|v| v.as_array());
            if let (Some(id), Some(replays)) = (id, replays) {
                if replays.iter().any(|entry| entry.get("path").and_then(|p| p.as_str()).is_some()) {
                    out.push(id);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

#[tauri::command]
fn update_bracket_set_replays(
    config_path: String,
    set_id: u64,
    replay_paths: Vec<String>,
) -> Result<(), String> {
    let resolved = resolve_startgg_sim_config_path(&config_path);
    if !resolved.is_file() {
        return Err(format!("Bracket config not found at {}", resolved.display()));
    }
    if replay_paths.is_empty() {
        return Err("No replay paths provided.".to_string());
    }

    let mut unique_paths: Vec<PathBuf> = Vec::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    for raw in replay_paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        if !is_replay_file_path(&path) {
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if seen_paths.insert(path.clone()) {
            unique_paths.push(path);
        }
    }

    if unique_paths.is_empty() {
        return Err("No valid .slp files found.".to_string());
    }

    let data = fs::read_to_string(&resolved)
        .map_err(|e| format!("read bracket config {}: {e}", resolved.display()))?;
    let mut value: Value = serde_json::from_str(&data)
        .map_err(|e| format!("parse bracket config {}: {e}", resolved.display()))?;

    let root = value
        .as_object_mut()
        .ok_or_else(|| "Bracket config must be a JSON object.".to_string())?;
    let replay_map = root
        .entry("referenceReplayMap")
        .or_insert_with(|| json!({ "sets": [] }));
    let replay_map_obj = replay_map
        .as_object_mut()
        .ok_or_else(|| "referenceReplayMap must be an object.".to_string())?;
    let sets_value = replay_map_obj
        .entry("sets")
        .or_insert_with(|| Value::Array(Vec::new()));
    let sets = sets_value
        .as_array_mut()
        .ok_or_else(|| "referenceReplayMap sets must be an array.".to_string())?;

    let mut entries = Vec::new();
    for path in unique_paths {
        let slots = replay_slots_from_file(&path);
        let mut entry = serde_json::Map::new();
        entry.insert("path".to_string(), Value::String(path.to_string_lossy().to_string()));
        if !slots.is_empty() {
            entry.insert("slots".to_string(), Value::Array(slots));
        }
        entries.push(Value::Object(entry));
    }
    let replay_entries = Value::Array(entries);

    let mut updated = false;
    for set in sets.iter_mut() {
        if set.get("id").and_then(|v| v.as_u64()) == Some(set_id) {
            if let Some(obj) = set.as_object_mut() {
                obj.insert("replays".to_string(), replay_entries.clone());
            } else {
                *set = json!({ "id": set_id, "replays": replay_entries.clone() });
            }
            updated = true;
            break;
        }
    }
    if !updated {
        sets.push(json!({ "id": set_id, "replays": replay_entries }));
    }

    let payload = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(&resolved, payload)
        .map_err(|e| format!("write bracket config {}: {e}", resolved.display()))?;
    Ok(())
}

#[tauri::command]
fn list_bracket_replay_pairs(config_path: String) -> Result<Vec<String>, String> {
    let resolved = resolve_startgg_sim_config_path(&config_path);
    if !resolved.is_file() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&resolved)
        .map_err(|e| format!("read bracket config {}: {e}", resolved.display()))?;
    let value: Value = serde_json::from_str(&data)
        .map_err(|e| format!("parse bracket config {}: {e}", resolved.display()))?;

    let mut pairs: HashSet<String> = HashSet::new();
    if let Some(sets) = value
        .get("referenceReplayMap")
        .and_then(|map| map.get("sets"))
        .and_then(|sets| sets.as_array())
    {
        for set in sets {
            let replays = match set.get("replays").and_then(|v| v.as_array()) {
                Some(replays) => replays,
                None => continue,
            };
            for replay_entry in replays {
                let path = replay_entry.get("path").and_then(|v| v.as_str()).unwrap_or("").trim();
                if path.is_empty() {
                    continue;
                }
                let mut unique: Vec<String> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                if let Some(slots) = replay_entry.get("slots").and_then(|v| v.as_array()) {
                    for slot in slots {
                        if let Some(code) = slot.get("slippiCode").and_then(|v| v.as_str()) {
                            if let Some(normalized) = normalize_slippi_code(code) {
                                if seen.insert(normalized.clone()) {
                                    unique.push(normalized);
                                }
                            }
                        }
                    }
                }
                if unique.len() != 2 {
                    continue;
                }
                let key = config::replay_pair_key(&unique[0], &unique[1]);
                pairs.insert(key);
            }
        }
    }
    let mut out: Vec<String> = pairs.into_iter().collect();
    out.sort();
    Ok(out)
}

// ── Config commands ────────────────────────────────────────────────────

#[tauri::command]
fn load_config() -> Result<AppConfig, String> {
    let config = load_config_inner()?;
    let _ = dolphin::ensure_slippi_wrapper();
    Ok(config)
}

#[tauri::command]
fn save_config(
    config: AppConfig,
    test_state: State<'_, SharedTestState>,
    live_startgg: State<'_, SharedLiveStartgg>,
) -> Result<AppConfig, String> {
    let saved = save_config_inner(config)?;
    let _ = dolphin::ensure_slippi_wrapper();
    if let Ok(mut guard) = test_state.lock() {
        sync_startgg_sim_path_from_config(&mut guard, &saved);
    }
    if let Ok(mut guard) = live_startgg.lock() {
        sync_live_startgg_from_config(&mut guard, &saved);
    }
    Ok(saved)
}

// ── Start.gg live snapshot command ─────────────────────────────────────

#[tauri::command]
fn startgg_live_snapshot(
    live_startgg: State<'_, SharedLiveStartgg>,
    force: Option<bool>,
) -> StartggLiveSnapshot {
    let config = load_config_inner().unwrap_or_else(|_| AppConfig::default());
    let state = startgg::maybe_refresh_live_startgg(&config, &live_startgg, force.unwrap_or(false));
    let (last_error, last_fetch_ms) = {
        let guard = live_startgg.lock().unwrap_or_else(|e| e.into_inner());
        let last_fetch_ms = guard.last_fetch.and_then(|time| {
            time
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_millis() as u64)
        });
        (guard.last_error.clone(), last_fetch_ms)
    };
    StartggLiveSnapshot {
        state,
        last_error,
        last_fetch_ms,
    }
}

// ── Overlay HTTP server ────────────────────────────────────────────────

fn resolve_overlay_dirs(app: &tauri::App) -> OverlayDirs {
    let root = if let Some(raw) = env_default("OVERLAY_DIR") {
        resolve_repo_path(&raw)
    } else {
        app
            .path()
            .resolve("overlay", BaseDirectory::Resource)
            .ok()
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| repo_root().join("overlay"))
    };

    OverlayDirs {
        root: root.clone(),
        resources: root.join("resources"),
        upcoming: root.join("upcoming"),
        dual: root.join("dual"),
        quad: root.join("quad"),
    }
}

fn overlay_router(state: OverlayServerState, static_dir: PathBuf, resources_dir: PathBuf) -> Router {
    let static_files = get_service(ServeDir::new(static_dir));
    let resource_files = get_service(ServeDir::new(resources_dir));

    Router::new()
        .route("/state.json", get(get_overlay_state_json))
        .nest_service("/resources", resource_files)
        .nest_service("/", static_files)
        .with_state(state)
}

async fn start_overlay_server(
    state: OverlayServerState,
    static_dir: PathBuf,
    resources_dir: PathBuf,
    addr: &str,
    label: &str,
) {
    let app = overlay_router(state, static_dir, resources_dir);
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("{label} overlay server failed to bind {addr}: {e}");
            return;
        }
    };
    info!("{label} overlay server listening at http://{addr}/");
    if let Err(e) = axum::serve(listener, app).await {
        error!("{label} overlay server error: {e}");
    }
}

async fn get_overlay_state_json(AxumState(state): AxumState<OverlayServerState>) -> impl IntoResponse {
    let setups = {
        let guard = state.setup_store.lock().unwrap_or_else(|e| e.into_inner());
        guard.setups.clone()
    };
    let config = load_config_inner().unwrap_or_else(|_| AppConfig::default());

    let (startgg_state, active_sets, replay_map) = if config.test_mode {
        let now = now_ms();
        let mut guard = state.test_state.lock().unwrap_or_else(|e| e.into_inner());
        sync_startgg_sim_path_from_config(&mut guard, &config);

        let should_use_startgg = !config.test_bracket_path.trim().is_empty() || guard.startgg_sim.is_some();
        let startgg_state = if should_use_startgg && init_startgg_sim(&mut guard, now).is_ok() {
            guard.startgg_sim.as_mut().map(|sim| sim.state(now))
        } else {
            None
        };
        let active_sets = guard.active_replay_sets.clone();
        let replay_map = guard.spoof_replays.clone();
        (startgg_state, Some(active_sets), replay_map)
    } else {
        let live_state = startgg::maybe_refresh_live_startgg(&config, &state.live_startgg, false);
        (live_state, None, HashMap::new())
    };

    let mut cache = state.replay_cache.lock().unwrap_or_else(|e| e.into_inner());
    let payload = build_overlay_state(
        &setups,
        startgg_state.as_ref(),
        active_sets.as_ref(),
        &config,
        &replay_map,
        &mut cache,
    );
    let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    (
        [
            ("Content-Type", "application/json"),
            ("Cache-Control", "no-store"),
            ("Pragma", "no-cache"),
            ("Expires", "0"),
        ],
        body,
    )
}

// ── Entry point ────────────────────────────────────────────────────────

pub fn run() {
    load_env_file();

    // Initialize tracing with file + stderr output
    let logs_dir = repo_root().join("logs");
    fs::create_dir_all(&logs_dir).ok();
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();
    info!("Melee Stream Tool starting");
    log_env_warnings();

    let setup_store: SharedSetupStore = Arc::new(Mutex::new(SetupStore::bootstrap_from_existing()));
    let test_state: SharedTestState = Arc::new(Mutex::new(TestModeState::default()));
    let live_startgg: SharedLiveStartgg = Arc::new(Mutex::new(LiveStartggState::default()));
    let replay_cache: SharedOverlayCache = Arc::new(Mutex::new(OverlayReplayCache::default()));
    let entrant_manager: SharedEntrantManager = Arc::new(Mutex::new(EntrantManager::new()));
    startgg::spawn_startgg_polling(live_startgg.clone(), Some(entrant_manager.clone()));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(setup_store.clone())
        .manage(test_state.clone())
        .manage(live_startgg.clone())
        .manage(replay_cache.clone())
        .manage(entrant_manager.clone())
        .setup(move |app| {
            let overlay_dirs = resolve_overlay_dirs(app);
            let OverlayDirs { root, resources, upcoming, dual, quad } = overlay_dirs;

            fs::create_dir_all(&root).ok();
            fs::create_dir_all(&resources).ok();
            fs::create_dir_all(&upcoming).ok();
            fs::create_dir_all(&dual).ok();
            fs::create_dir_all(&quad).ok();

            let overlay_state = OverlayServerState {
                setup_store: setup_store.clone(),
                test_state: test_state.clone(),
                live_startgg: live_startgg.clone(),
                replay_cache: replay_cache.clone(),
            };

            tauri::async_runtime::spawn(start_overlay_server(
                overlay_state.clone(),
                root,
                resources.clone(),
                "127.0.0.1:17890",
                "Main",
            ));

            tauri::async_runtime::spawn(start_overlay_server(
                overlay_state.clone(),
                upcoming,
                resources.clone(),
                "127.0.0.1:17891",
                "Upcoming",
            ));

            tauri::async_runtime::spawn(start_overlay_server(
                overlay_state.clone(),
                dual,
                resources.clone(),
                "127.0.0.1:17892",
                "Dual",
            ));

            tauri::async_runtime::spawn(start_overlay_server(
                overlay_state,
                quad,
                resources,
                "127.0.0.1:17893",
                "Quad",
            ));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_setups_stub,
            list_setups,
            create_setup,
            delete_setup,
            slippi::find_slippi_launcher_window,
            slippi::scan_slippi_streams,
            slippi::refresh_slippi_launcher,
            slippi::watch_slippi_stream,
            dolphin::launch_dolphin_for_setup,
            slippi::assign_stream_to_setup,
            slippi::clear_setup_assignment,
            slippi::launch_slippi_app,
            slippi::relaunch_slippi_app,
            dolphin::launch_dolphin_cli,
            test_mode::spoof_live_games,
            test_mode::spoof_bracket_set_replays,
            test_mode::spoof_bracket_set_replay,
            test_mode::cancel_spoof_bracket_set_replays,
            list_bracket_configs,
            list_bracket_replay_sets,
            list_bracket_set_replay_paths,
            update_bracket_set_replays,
            list_bracket_replay_pairs,
            startgg_sim_commands::startgg_sim_state,
            startgg_sim_commands::startgg_sim_reset,
            startgg_sim_commands::startgg_sim_advance_set,
            startgg_sim_commands::startgg_sim_force_winner,
            startgg_sim_commands::startgg_sim_mark_dq,
            startgg_sim_commands::startgg_sim_raw_state,
            startgg_sim_commands::startgg_sim_raw_reset,
            startgg_sim_commands::startgg_sim_raw_advance_set,
            startgg_sim_commands::startgg_sim_raw_start_set,
            startgg_sim_commands::startgg_sim_raw_update_scores,
            startgg_sim_commands::startgg_sim_raw_apply_replay_result,
            startgg_sim_commands::startgg_sim_raw_step_set,
            startgg_sim_commands::startgg_sim_raw_finalize_reference_set,
            startgg_sim_commands::startgg_sim_raw_finish_set,
            startgg_sim_commands::startgg_sim_raw_complete_bracket,
            startgg_sim_commands::startgg_sim_raw_force_winner,
            startgg_sim_commands::startgg_sim_raw_mark_dq,
            startgg_sim_commands::startgg_sim_raw_reset_set,
            startgg_sim_commands::startgg_sim_clear_persisted_state,
            startgg_sim_commands::startgg_sim_persistence_status,
            test_mode::set_broadcast_players,
            startgg_live_snapshot,
            load_config,
            save_config,
            entrant_commands::get_unified_entrants,
            entrant_commands::set_entrant_slippi_code,
            entrant_commands::assign_entrant_to_setup,
            entrant_commands::unassign_entrant,
            entrant_commands::toggle_auto_assignment,
            entrant_commands::get_setups_sorted_by_seed,
            entrant_commands::get_auto_assignment_status,
            entrant_commands::run_auto_assignment,
            entrant_commands::sync_entrants_from_startgg
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri app");
}
