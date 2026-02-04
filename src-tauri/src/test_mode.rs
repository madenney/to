use crate::config::*;
use crate::types::*;
use crate::replay::*;
use crate::dolphin::stop_child_process;
use crate::startgg::{init_startgg_sim, build_bracket_replay_map, read_bracket_set_replay_paths};
use chrono::{DateTime, Local};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State};

use chrono::Duration as ChronoDuration;
use std::process::Child;

// ── Env helpers ─────────────────────────────────────────────────────────

pub fn replay_spoof_mode() -> ReplaySpoofMode {
    if env_flag_true("SPOOF_REPLAY_COPY") {
        return ReplaySpoofMode::Copy;
    }
    if let Ok(raw) = env::var("SPOOF_REPLAY_MODE") {
        let normalized = raw.trim().to_ascii_lowercase();
        if matches!(normalized.as_str(), "copy" | "instant" | "fast") {
            return ReplaySpoofMode::Copy;
        }
        if matches!(normalized.as_str(), "stream" | "realtime" | "real-time") {
            return ReplaySpoofMode::Stream;
        }
    }
    ReplaySpoofMode::Stream
}

pub fn replay_spoof_gap_ms() -> u64 {
    env::var("SPOOF_REPLAY_GAP_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(1500)
}

// ── Mock streams ────────────────────────────────────────────────────────

pub fn slippi_mock_streams_path() -> Option<PathBuf> {
    env::var("SLIPPI_MOCK_STREAMS_PATH")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn mock_streams_enabled() -> bool {
    env_flag_true("SLIPPI_MOCK_STREAMS") || slippi_mock_streams_path().is_some()
}

pub fn default_mock_streams_path() -> PathBuf {
    repo_root().join("test_files").join("mock_streams.json")
}

pub fn load_mock_streams(path: &PathBuf) -> Result<Vec<SlippiStream>, String> {
    let data = fs::read_to_string(path)
        .map_err(|e| format!("read mock streams {}: {e}", path.display()))?;
    let mut streams = serde_json::from_str::<Vec<SlippiStream>>(&data)
        .map_err(|e| format!("parse mock streams {}: {e}", path.display()))?;
    for (idx, stream) in streams.iter_mut().enumerate() {
        if stream.id.trim().is_empty() {
            stream.id = format!("mock-{}", idx + 1);
        }
        if stream.window_title.is_none() {
            stream.window_title = Some("Mock Slippi Launcher".to_string());
        }
        if stream.is_playing.is_none() {
            stream.is_playing = Some(false);
        }
        if stream.source.is_none() {
            stream.source = Some("mock".to_string());
        }
    }
    Ok(streams)
}

// ── Test mode stream generation ─────────────────────────────────────────

pub fn test_mode_streams() -> Result<Vec<SlippiStream>, String> {
    if let Some(path) = slippi_mock_streams_path() {
        return load_mock_streams(&path);
    }
    if env_flag_true("SLIPPI_MOCK_STREAMS") {
        let default_path = default_mock_streams_path();
        if default_path.is_file() {
            return load_mock_streams(&default_path);
        }
        return Ok(vec![
            SlippiStream {
                id: "mock-1".to_string(),
                window_title: Some("Mock Slippi Launcher".to_string()),
                p1_tag: Some("MANGO".to_string()),
                p2_tag: Some("ZAIN".to_string()),
                p1_code: Some("MANGO#777".to_string()),
                p2_code: Some("ZAIN#999".to_string()),
                startgg_entrant_id: None,
                replay_path: None,
                is_playing: Some(false),
                source: Some("mock".to_string()),
                startgg_set: None,
            },
            SlippiStream {
                id: "mock-2".to_string(),
                window_title: Some("Mock Slippi Launcher".to_string()),
                p1_tag: Some("ARMADA".to_string()),
                p2_tag: Some("HBOX".to_string()),
                p1_code: Some("ARMADA#321".to_string()),
                p2_code: Some("HBOX#888".to_string()),
                startgg_entrant_id: None,
                replay_path: None,
                is_playing: Some(false),
                source: Some("mock".to_string()),
                startgg_set: None,
            },
            SlippiStream {
                id: "mock-3".to_string(),
                window_title: Some("Mock Slippi Launcher".to_string()),
                p1_tag: Some("LEFFEN".to_string()),
                p2_tag: Some("PLUP".to_string()),
                p1_code: Some("LEFFEN#555".to_string()),
                p2_code: Some("PLUP#222".to_string()),
                startgg_entrant_id: None,
                replay_path: None,
                is_playing: Some(false),
                source: Some("mock".to_string()),
                startgg_set: None,
            },
        ]);
    }
    build_test_streams().map(|items| items.into_iter().map(|item| item.stream).collect())
}

pub fn build_test_streams() -> Result<Vec<TestStreamSpec>, String> {
    let folders = load_test_folder_paths()?;
    let mut out = Vec::new();

    for (idx, folder) in folders.iter().enumerate() {
        let replays = collect_slp_files(folder)?;
        if replays.is_empty() {
            return Err(format!("No .slp files found in {}", folder.display()));
        }

        let primary = most_common_connect_code(&replays)
            .map_err(|e| format!("{e} (folder: {})", folder.display()))?;
        let opponent = find_opponent_code(&primary, &replays);
        let folder_name = folder
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("folder-{}", idx + 1));

        let p1_tag = Some(tag_from_code(&primary));
        let p2_tag = opponent.as_ref().map(|code| tag_from_code(code));
        let replay_path = replays[0].clone();
        let stream = SlippiStream {
            id: format!("test-{}", folder_name),
            window_title: Some("Test Mode".to_string()),
            p1_tag,
            p2_tag,
            p1_code: Some(primary),
            p2_code: opponent,
            startgg_entrant_id: None,
            replay_path: Some(replay_path.to_string_lossy().to_string()),
            is_playing: Some(false),
            source: Some(format!("test:{}", folder_name)),
            startgg_set: None,
        };

        out.push(TestStreamSpec {
            stream,
            replay_path,
        });
    }

    if out.is_empty() {
        return Err("No test streams generated from configured folders.".to_string());
    }
    Ok(out)
}

pub fn build_test_replay_lookup() -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let items = match build_test_streams() {
        Ok(items) => items,
        Err(_) => return out,
    };
    for item in items {
        let TestStreamSpec { stream, replay_path } = item;
        if let Some(code) = stream.p1_code {
            let key = normalize_broadcast_key(&code);
            if !key.is_empty() {
                out.insert(key, replay_path);
            }
        }
    }
    out
}

// ── Broadcast / bracket stream helpers ──────────────────────────────────

pub fn test_mode_broadcast_streams(guard: &mut TestModeState) -> Result<Vec<SlippiStream>, String> {
    if guard.broadcast_players.is_empty() {
        guard.spoof_replays.clear();
        return Ok(Vec::new());
    }

    let now = now_ms();
    init_startgg_sim(guard, now)?;
    let sim = guard
        .startgg_sim
        .as_mut()
        .ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
    let state = sim.state(now);
    let event_name = state.event.name.clone();

    let config_path = guard
        .startgg_config_path
        .clone()
        .unwrap_or_else(startgg_sim_config_path);
    let replay_map = build_bracket_replay_map(&config_path);
    let test_replay_map = build_test_replay_lookup();
    let fallback_replay = replay_map
        .values()
        .next()
        .cloned()
        .or_else(|| test_replay_map.values().next().cloned());

    let mut streams = Vec::new();
    let mut replay_lookup = HashMap::new();

    for player in &guard.broadcast_players {
        let set = find_set_for_player(&state.sets, player, Some(&guard.active_replay_sets)).cloned();
        let mut p1_tag = player.name.trim().to_string();
        let mut p1_code = player.slippi_code.trim().to_string();
        if p1_tag.is_empty() || p1_code.is_empty() {
            if let Some(found) = set.as_ref() {
                if let Some(slot) = found.slots.iter().find(|slot| slot_matches_player(slot, player)) {
                    let (tag, code) = slot_label(Some(slot));
                    if p1_tag.is_empty() {
                        p1_tag = tag.unwrap_or_default();
                    }
                    if p1_code.is_empty() {
                        p1_code = code.unwrap_or_default();
                    }
                }
            }
        }

        let is_playing = set
            .as_ref()
            .map(|found| guard.active_replay_sets.contains(&found.id) || found.state == "inProgress")
            .unwrap_or(false);
        let expected_p2 = if let Some(found) = set.as_ref() {
            found
                .slots
                .iter()
                .find(|slot| !slot_matches_player(slot, player))
                .map(|slot| slot_label(Some(slot)))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };
        let replay_path = set
            .as_ref()
            .and_then(|found| guard.active_replay_paths.get(&found.id).cloned())
            .or_else(|| set.as_ref().and_then(|found| replay_map.get(&found.id).cloned()))
            .or_else(|| test_replay_map.get(&normalize_broadcast_key(&p1_code)).cloned())
            .or_else(|| fallback_replay.clone());
        let (p2_tag, p2_code) = if is_playing {
            let opponent_code = replay_path
                .as_ref()
                .and_then(|path| find_opponent_code_in_replay(&p1_code, path));
            let opponent_tag = opponent_code.as_ref().map(|code| tag_from_code(code));
            if opponent_tag.is_some() || opponent_code.is_some() {
                (opponent_tag, opponent_code)
            } else {
                expected_p2
            }
        } else {
            expected_p2
        };
        let title = set
            .as_ref()
            .map(|found| format!("{event_name} · {}", found.round_label))
            .unwrap_or_else(|| "Test Mode".to_string());
        let stream_id = format!("broadcast-{}", player.id);
        let stream = SlippiStream {
            id: stream_id.clone(),
            window_title: Some(title),
            p1_tag: if p1_tag.is_empty() { None } else { Some(p1_tag) },
            p1_code: if p1_code.is_empty() { None } else { Some(p1_code.clone()) },
            p2_tag,
            p2_code,
            startgg_entrant_id: Some(player.id),
            replay_path: replay_path.as_ref().map(|path| path.to_string_lossy().to_string()),
            is_playing: Some(is_playing),
            source: Some("broadcast".to_string()),
            startgg_set: set.clone(),
        };
        streams.push(stream);

        if let Some(path) = replay_path {
            replay_lookup.insert(stream_id, path);
        }
    }

    guard.spoof_replays = replay_lookup;
    Ok(streams)
}

pub fn test_mode_streams_from_replays(guard: &mut TestModeState) -> Result<Vec<SlippiStream>, String> {
    let items = build_test_streams()?;
    let mut replay_map = HashMap::new();
    let streams: Vec<SlippiStream> = items
        .into_iter()
        .map(|item| {
            replay_map.insert(item.stream.id.clone(), item.replay_path);
            item.stream
        })
        .collect();
    guard.spoof_replays = replay_map;
    Ok(streams)
}

pub fn test_mode_bracket_streams(guard: &mut TestModeState) -> Result<Vec<SlippiStream>, String> {
    let now = now_ms();
    init_startgg_sim(guard, now)?;
    let sim = guard
        .startgg_sim
        .as_mut()
        .ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
    let state = sim.state(now);
    let mut candidates: Vec<&crate::startgg_sim::StartggSimSet> =
        state.sets.iter().filter(|set| set.state == "inProgress").collect();
    if candidates.is_empty() {
        candidates = state.sets.iter().filter(|set| set.state == "pending").collect();
    }
    if candidates.is_empty() {
        candidates = state.sets.iter().collect();
    }
    candidates.sort_by(|a, b| a.id.cmp(&b.id));

    if guard.broadcast_filter_enabled {
        if guard.broadcast_codes.is_empty() && guard.broadcast_tags.is_empty() {
            candidates.clear();
        } else {
            candidates = candidates
                .into_iter()
                .filter(|set| set_matches_broadcast(set, guard))
                .collect();
        }
    }

    let config_path = guard
        .startgg_config_path
        .clone()
        .unwrap_or_else(startgg_sim_config_path);
    let replay_map = build_bracket_replay_map(&config_path);

    let mut streams = Vec::new();
    let mut replay_lookup = HashMap::new();
    let event_name = state.event.name.clone();
    for set in candidates.into_iter().take(TEST_STREAM_LIMIT) {
        let (p1_tag, p1_code) = slot_label(set.slots.get(0));
        let (p2_tag, p2_code) = slot_label(set.slots.get(1));
        if p1_tag.is_none() && p2_tag.is_none() && p1_code.is_none() && p2_code.is_none() {
            continue;
        }
        let stream_id = format!("set-{}", set.id);
        let title = format!("{} · {}", event_name, set.round_label);
        let is_playing = set.state == "inProgress" || guard.active_replay_sets.contains(&set.id);
        let replay_path = guard
            .active_replay_paths
            .get(&set.id)
            .cloned()
            .or_else(|| replay_map.get(&set.id).cloned());
        streams.push(SlippiStream {
            id: stream_id.clone(),
            window_title: Some(title),
            p1_tag,
            p2_tag,
            p1_code,
            p2_code,
            startgg_entrant_id: None,
            replay_path: replay_path.as_ref().map(|path| path.to_string_lossy().to_string()),
            is_playing: Some(is_playing),
            source: Some("test-bracket".to_string()),
            startgg_set: Some(set.clone()),
        });
        if let Some(path) = replay_path {
            replay_lookup.insert(stream_id, path);
        }
    }
    guard.spoof_replays = replay_lookup;
    Ok(streams)
}

// ── Tauri commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn spoof_live_games(test_state: State<'_, SharedTestState>) -> Result<Vec<SlippiStream>, String> {
    if !app_test_mode_enabled() {
        return Err("Test mode is disabled in settings.".to_string());
    }
    let config = load_config_inner()?;
    let spectate_raw = config.spectate_folder_path.trim();
    if spectate_raw.is_empty() {
        return Err("Spectate folder path is not set in settings.".to_string());
    }
    let spectate_dir = resolve_repo_path(spectate_raw);
    fs::create_dir_all(&spectate_dir)
        .map_err(|e| format!("create spectate folder {}: {e}", spectate_dir.display()))?;

    let items = build_test_streams()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let tasks_dir = repo_root().join("airlock").join("tmp");
    fs::create_dir_all(&tasks_dir)
        .map_err(|e| format!("create tasks folder {}: {e}", tasks_dir.display()))?;

    let fps = 60u32;
    let tasks: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            json!({
                "replayPath": item.replay_path.to_string_lossy(),
                "outputDir": spectate_dir.to_string_lossy(),
                "startTimeMs": now + ((idx as u64) * 1000),
                "fps": fps,
            })
        })
        .collect();

    let payload = json!({
        "fps": fps,
        "streams": tasks,
    });
    let tasks_path = tasks_dir.join(format!("spoof_tasks_{now}.json"));
    let tasks_json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(&tasks_path, tasks_json)
        .map_err(|e| format!("write tasks {}: {e}", tasks_path.display()))?;

    let script_path = repo_root().join("scripts").join("spoof_live_games.js");
    if !script_path.is_file() {
        return Err(format!("spoof script not found at {}", script_path.display()));
    }

    let node_path = build_node_path()?;
    let mut cmd = Command::new("node");
    cmd.arg(script_path)
        .arg("--tasks")
        .arg(&tasks_path)
        .env("NODE_PATH", node_path)
        .current_dir(repo_root());
    cmd.spawn().map_err(|e| format!("start spoof script: {e}"))?;

    let mut replay_map = HashMap::new();
    let streams: Vec<SlippiStream> = items
        .into_iter()
        .map(|item| {
            replay_map.insert(item.stream.id.clone(), item.replay_path.clone());
            item.stream
        })
        .collect();
    let mut guard = test_state.lock().map_err(|e| e.to_string())?;
    guard.spoof_streams = streams.clone();
    guard.spoof_replays = replay_map;
    if guard.broadcast_filter_enabled {
        return test_mode_broadcast_streams(&mut guard);
    }
    Ok(filter_broadcast_streams(&streams, &guard))
}

#[tauri::command]
pub fn spoof_bracket_set_replays(
    app_handle: tauri::AppHandle,
    config_path: String,
    set_id: u64,
    test_state: State<'_, SharedTestState>,
) -> Result<SpoofReplayResult, String> {
    if !app_test_mode_enabled() {
        return Err("Test mode is disabled in settings.".to_string());
    }
    if let Ok(mut guard) = test_state.lock() {
        guard.cancel_replay_sets.remove(&set_id);
    }
    let config = load_config_inner()?;
    let spectate_raw = config.spectate_folder_path.trim();
    if spectate_raw.is_empty() {
        return Err("Spectate folder path is not set in settings.".to_string());
    }
    let spectate_dir = resolve_repo_path(spectate_raw);
    fs::create_dir_all(&spectate_dir)
        .map_err(|e| format!("create spectate folder {}: {e}", spectate_dir.display()))?;

    let replay_paths = read_bracket_set_replay_paths(&config_path, set_id)?;
    let mut missing = 0usize;
    let mut valid_paths = Vec::new();
    for path in replay_paths {
        if path.is_file() {
            valid_paths.push(path);
        } else {
            missing += 1;
        }
    }
    if valid_paths.is_empty() {
        return Err(format!("No replay files found for set {set_id}."));
    }

    let valid_paths = sort_replay_paths_by_start_time(valid_paths);
    let replay_total = valid_paths.len();
    if replay_spoof_mode() == ReplaySpoofMode::Copy {
        let gap_ms = replay_spoof_gap_ms();
        {
            let mut guard = test_state.lock().map_err(|e| e.to_string())?;
            guard.active_replay_sets.insert(set_id);
        }
        let copy_result: Result<SpoofReplayResult, String> = (|| {
            let base_time: DateTime<Local> = SystemTime::now().into();
            for (idx, path) in valid_paths.iter().enumerate() {
                if let Ok(guard) = test_state.lock() {
                    if guard.cancel_replay_sets.contains(&set_id) {
                        return Err("Replay spoof cancelled.".to_string());
                    }
                }
                if let Ok(mut guard) = test_state.lock() {
                    guard.active_replay_paths.insert(set_id, path.clone());
                }
                let timestamp = base_time + ChronoDuration::seconds(idx as i64);
                let base_name = format_game_name(timestamp);
                let output_path = unique_spectate_path(&spectate_dir, &base_name, idx);
                let replay_index = idx + 1;
                let start_payload = json!({
                    "type": "start",
                    "setId": set_id,
                    "replayIndex": replay_index,
                    "replayTotal": replay_total,
                    "replayPath": path.to_string_lossy(),
                    "outputPath": output_path.to_string_lossy(),
                });
                let _ = app_handle.emit("spoof-replay-progress", start_payload);
                fs::copy(path, &output_path).map_err(|e| {
                    format!(
                        "copy replay {} -> {}: {e}",
                        path.display(),
                        output_path.display()
                    )
                })?;
                let event_type = if replay_index == replay_total {
                    "complete"
                } else {
                    "progress"
                };
                let payload = json!({
                    "type": event_type,
                    "setId": set_id,
                    "replayIndex": replay_index,
                    "replayTotal": replay_total,
                    "replayPath": path.to_string_lossy(),
                    "outputPath": output_path.to_string_lossy(),
                });
                let _ = app_handle.emit("spoof-replay-progress", payload);
                if replay_index == replay_total {
                    if let Ok(mut guard) = test_state.lock() {
                        guard.active_replay_sets.remove(&set_id);
                        guard.active_replay_paths.remove(&set_id);
                    }
                } else if gap_ms > 0 {
                    sleep(Duration::from_millis(gap_ms));
                }
            }
            Ok(SpoofReplayResult {
                started: replay_total,
                missing,
            })
        })();
        if copy_result.is_err() {
            if let Ok(mut guard) = test_state.lock() {
                guard.active_replay_sets.remove(&set_id);
                guard.active_replay_paths.remove(&set_id);
            }
        }
        return copy_result;
    }

    let mut tasks: Vec<Value> = Vec::new();
    for (idx, path) in valid_paths.into_iter().enumerate() {
        tasks.push(json!({
            "replayPath": path.to_string_lossy(),
            "outputDir": spectate_dir.to_string_lossy(),
            "fps": 60,
            "setId": set_id,
            "replayIndex": idx + 1,
            "replayTotal": replay_total,
        }));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let tasks_dir = repo_root().join("airlock").join("tmp");
    fs::create_dir_all(&tasks_dir)
        .map_err(|e| format!("create tasks folder {}: {e}", tasks_dir.display()))?;

    let payload = json!({
        "fps": 60,
        "gapMs": replay_spoof_gap_ms(),
        "sequential": true,
        "streams": tasks,
    });
    let tasks_path = tasks_dir.join(format!("spoof_set_{set_id}_{now}.json"));
    let tasks_json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(&tasks_path, tasks_json)
        .map_err(|e| format!("write tasks {}: {e}", tasks_path.display()))?;

    let script_path = repo_root().join("scripts").join("spoof_live_games.js");
    if !script_path.is_file() {
        return Err(format!("spoof script not found at {}", script_path.display()));
    }

    let node_path = build_node_path()?;
    let mut cmd = Command::new("node");
    cmd.arg(script_path)
        .arg("--tasks")
        .arg(&tasks_path)
        .env("NODE_PATH", node_path)
        .current_dir(repo_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("start spoof script: {e}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    {
        let mut guard = test_state.lock().map_err(|e| e.to_string())?;
        guard.active_replay_sets.insert(set_id);
        guard.active_replay_paths.remove(&set_id);
        guard.active_replay_children.insert(set_id, child);
    }

    if let Some(stdout) = stdout {
        let app = app_handle.clone();
        let set_id = set_id;
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                if let Ok(guard) = app.state::<SharedTestState>().lock() {
                    if guard.cancel_replay_sets.contains(&set_id) {
                        break;
                    }
                }
                if let Some(payload) = line.strip_prefix("SPOOF_PROGRESS:") {
                    if let Ok(value) = serde_json::from_str::<Value>(payload) {
                        if let Some(path) = value.get("replayPath").and_then(|v| v.as_str()) {
                            if let Ok(mut guard) = app.state::<SharedTestState>().lock() {
                                guard.active_replay_paths.insert(set_id, PathBuf::from(path));
                            }
                        }
                        let _ = app.emit("spoof-replay-progress", &value);
                        let is_done = value
                            .get("type")
                            .and_then(|v| v.as_str())
                            .map(|t| t == "complete")
                            .unwrap_or(false);
                        let replay_index = value.get("replayIndex").and_then(|v| v.as_u64());
                        let replay_total = value.get("replayTotal").and_then(|v| v.as_u64());
                        let payload_set_id = value.get("setId").and_then(|v| v.as_u64());
                        if is_done && replay_index == replay_total && payload_set_id == Some(set_id) {
                            let mut child = None;
                            if let Ok(mut guard) = app.state::<SharedTestState>().lock() {
                                guard.active_replay_sets.remove(&set_id);
                                guard.active_replay_paths.remove(&set_id);
                                guard.cancel_replay_sets.remove(&set_id);
                                child = guard.active_replay_children.remove(&set_id);
                            }
                            if let Some(mut child) = child {
                                let _ = child.wait();
                            }
                        }
                    }
                }
            }
        });
    }

    if let Some(stderr) = stderr {
        let app = app_handle.clone();
        let set_id = set_id;
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let payload = json!({
                    "type": "error",
                    "setId": set_id,
                    "message": trimmed,
                });
                let _ = app.emit("spoof-replay-progress", payload);
            }
        });
    }

    // stderr is already handled above

    Ok(SpoofReplayResult {
        started: tasks.len(),
        missing,
    })
}

#[tauri::command]
pub fn spoof_bracket_set_replay(
    app_handle: tauri::AppHandle,
    set_id: u64,
    replay_path: String,
    replay_index: u32,
    replay_total: u32,
    test_state: State<'_, SharedTestState>,
) -> Result<SpoofReplayResult, String> {
    if !app_test_mode_enabled() {
        return Err("Test mode is disabled in settings.".to_string());
    }
    if let Ok(mut guard) = test_state.lock() {
        guard.cancel_replay_sets.remove(&set_id);
    }
    let config = load_config_inner()?;
    let spectate_raw = config.spectate_folder_path.trim();
    if spectate_raw.is_empty() {
        return Err("Spectate folder path is not set in settings.".to_string());
    }
    let spectate_dir = resolve_repo_path(spectate_raw);
    fs::create_dir_all(&spectate_dir)
        .map_err(|e| format!("create spectate folder {}: {e}", spectate_dir.display()))?;

    let replay_path = replay_path.trim();
    if replay_path.is_empty() {
        return Err("Replay path is empty.".to_string());
    }
    let mut resolved = PathBuf::from(replay_path);
    if !resolved.is_absolute() {
        resolved = resolve_repo_path(replay_path);
    }
    if !resolved.is_file() {
        return Err(format!("Replay not found at {}", resolved.display()));
    }

    if replay_spoof_mode() == ReplaySpoofMode::Copy {
        {
            let mut guard = test_state.lock().map_err(|e| e.to_string())?;
            guard.active_replay_sets.insert(set_id);
            guard.active_replay_paths.insert(set_id, resolved.clone());
        }
        let timestamp: DateTime<Local> = SystemTime::now().into();
        let base_name = format_game_name(timestamp);
        let output_path = unique_spectate_path(&spectate_dir, &base_name, 0);
        let start_payload = json!({
            "type": "start",
            "setId": set_id,
            "replayIndex": replay_index,
            "replayTotal": replay_total,
            "replayPath": resolved.to_string_lossy(),
            "outputPath": output_path.to_string_lossy(),
        });
        let _ = app_handle.emit("spoof-replay-progress", start_payload);
        fs::copy(&resolved, &output_path).map_err(|e| {
            format!(
                "copy replay {} -> {}: {e}",
                resolved.display(),
                output_path.display()
            )
        })?;
        let payload = json!({
            "type": "complete",
            "setId": set_id,
            "replayIndex": replay_index,
            "replayTotal": replay_total,
            "replayPath": resolved.to_string_lossy(),
            "outputPath": output_path.to_string_lossy(),
        });
        let _ = app_handle.emit("spoof-replay-progress", payload);
        if let Ok(mut guard) = test_state.lock() {
            guard.active_replay_sets.remove(&set_id);
            guard.active_replay_paths.remove(&set_id);
        }
        return Ok(SpoofReplayResult { started: 1, missing: 0 });
    }

    let tasks_dir = repo_root().join("airlock").join("tmp");
    fs::create_dir_all(&tasks_dir)
        .map_err(|e| format!("create tasks folder {}: {e}", tasks_dir.display()))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let tasks_path = tasks_dir.join(format!("spoof_set_{set_id}_{now}.json"));
    let payload = json!({
        "fps": 60,
        "streams": [{
            "replayPath": resolved.to_string_lossy(),
            "outputDir": spectate_dir.to_string_lossy(),
            "fps": 60,
            "setId": set_id,
            "replayIndex": replay_index,
            "replayTotal": replay_total,
        }],
    });
    let tasks_json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(&tasks_path, tasks_json)
        .map_err(|e| format!("write tasks {}: {e}", tasks_path.display()))?;

    let script_path = repo_root().join("scripts").join("spoof_live_games.js");
    if !script_path.is_file() {
        return Err(format!("spoof script not found at {}", script_path.display()));
    }

    let node_path = build_node_path()?;
    let mut cmd = Command::new("node");
    cmd.arg(script_path)
        .arg("--tasks")
        .arg(&tasks_path)
        .env("NODE_PATH", node_path)
        .current_dir(repo_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("start spoof script: {e}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    {
        let mut guard = test_state.lock().map_err(|e| e.to_string())?;
        guard.active_replay_sets.insert(set_id);
        guard.active_replay_paths.insert(set_id, resolved.clone());
        guard.active_replay_children.insert(set_id, child);
    }

    if let Some(stdout) = stdout {
        let app = app_handle.clone();
        let set_id = set_id;
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                if let Ok(guard) = app.state::<SharedTestState>().lock() {
                    if guard.cancel_replay_sets.contains(&set_id) {
                        break;
                    }
                }
                if let Some(payload) = line.strip_prefix("SPOOF_PROGRESS:") {
                    if let Ok(value) = serde_json::from_str::<Value>(payload) {
                        if let Some(path) = value.get("replayPath").and_then(|v| v.as_str()) {
                            if let Ok(mut guard) = app.state::<SharedTestState>().lock() {
                                guard.active_replay_paths.insert(set_id, PathBuf::from(path));
                            }
                        }
                        let _ = app.emit("spoof-replay-progress", &value);
                        let is_done = value
                            .get("type")
                            .and_then(|v| v.as_str())
                            .map(|t| t == "complete")
                            .unwrap_or(false);
                        if is_done {
                            let mut child = None;
                            if let Ok(mut guard) = app.state::<SharedTestState>().lock() {
                                guard.active_replay_sets.remove(&set_id);
                                guard.active_replay_paths.remove(&set_id);
                                guard.cancel_replay_sets.remove(&set_id);
                                child = guard.active_replay_children.remove(&set_id);
                            }
                            if let Some(mut child) = child {
                                let _ = child.wait();
                            }
                        }
                    }
                }
            }
        });
    }

    if let Some(stderr) = stderr {
        let app = app_handle.clone();
        let set_id = set_id;
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let payload = json!({
                    "type": "error",
                    "setId": set_id,
                    "message": trimmed,
                });
                let _ = app.emit("spoof-replay-progress", payload);
            }
        });
    }

    Ok(SpoofReplayResult { started: 1, missing: 0 })
}

#[tauri::command]
pub fn cancel_spoof_bracket_set_replays(
    app_handle: tauri::AppHandle,
    set_id: Option<u64>,
    test_state: State<'_, SharedTestState>,
) -> Result<usize, String> {
    let mut children: Vec<Child> = Vec::new();
    let mut targets: Vec<u64> = Vec::new();
    {
        let mut guard = test_state.lock().map_err(|e| e.to_string())?;
        if let Some(id) = set_id {
            targets.push(id);
        } else {
            targets.extend(guard.active_replay_sets.iter().copied());
            targets.extend(guard.active_replay_children.keys().copied());
        }
        targets.sort_unstable();
        targets.dedup();
        for id in &targets {
            guard.cancel_replay_sets.insert(*id);
            guard.active_replay_sets.remove(id);
            guard.active_replay_paths.remove(id);
            if let Some(child) = guard.active_replay_children.remove(id) {
                children.push(child);
            }
        }
    }

    for child in children {
        let _ = stop_child_process(child);
    }

    for id in &targets {
        let payload = json!({
            "type": "error",
            "setId": id,
            "message": "Replay spoof cancelled.",
        });
        let _ = app_handle.emit("spoof-replay-progress", payload);
    }

    Ok(targets.len())
}

#[tauri::command]
pub fn set_broadcast_players(
    players: Vec<BroadcastPlayerSelection>,
    test_state: State<'_, SharedTestState>,
) -> Result<(), String> {
    let mut codes = HashSet::new();
    let mut tags = HashSet::new();
    for player in &players {
        let code = normalize_broadcast_key(&player.slippi_code);
        if !code.is_empty() {
            codes.insert(code);
        }
        let name = normalize_tag_key(&player.name);
        if !name.is_empty() {
            tags.insert(name);
        }
    }

    let mut guard = test_state.lock().map_err(|e| e.to_string())?;
    guard.broadcast_filter_enabled = true;
    guard.broadcast_players = players;
    guard.broadcast_codes = codes;
    guard.broadcast_tags = tags;
    Ok(())
}
