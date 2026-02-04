use crate::config::*;
use crate::types::*;
use crate::test_mode::{mock_streams_enabled, test_mode_streams, test_mode_broadcast_streams, test_mode_bracket_streams, test_mode_streams_from_replays};
use crate::dolphin::{
    launch_dolphin_for_setup_internal, launch_dolphin_playback_for_setup_internal,
    stop_dolphin_child, stop_process_by_pid, list_dolphin_like_pids,
    find_new_dolphin_cmdline_any, ensure_slippi_wrapper, ensure_slippi_playback_wrapper,
    write_slippi_watch_label, clear_slippi_watch_label, slippi_launches_dolphin, list_slippi_pids,
    target_display, slippi_appimage_path,
};
use crate::replay::{
    filter_broadcast_streams, find_opponent_code_in_replay, tag_from_code,
    update_replay_index, latest_replay_for_code,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    process::{Command, Stdio},
    thread::sleep,
    time::Duration,
};
use tauri::State;
use tungstenite::Message;
use x11rb::{
    connection::Connection,
    protocol::xproto::{AtomEnum, ConnectionExt, Window},
    rust_connection::RustConnection,
};

// ── X11 helpers ─────────────────────────────────────────────────────────

pub fn read_window_title(conn: &RustConnection, window: Window) -> Option<String> {
  // UTF8 title via _NET_WM_NAME
  let utf8_title = (|| {
    let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME").ok()?.reply().ok()?;
    let utf8_string = conn.intern_atom(false, b"UTF8_STRING").ok()?.reply().ok()?;
    let prop = conn
      .get_property(false, window, net_wm_name.atom, utf8_string.atom, 0, 1024)
      .ok()?
      .reply()
      .ok()?;
    let txt = String::from_utf8(prop.value).ok()?;
    let trimmed = txt.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
  })();
  if let Some(txt) = utf8_title {
    return Some(txt);
  }

  // Fallback to classic WM_NAME (STRING)
  let wm_name = (|| {
    let prop = conn
      .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)
      .ok()?
      .reply()
      .ok()?;
    let txt = String::from_utf8(prop.value).ok()?;
    let trimmed = txt.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
  })();
  if let Some(txt) = wm_name {
    return Some(txt);
  }

  None
}

pub fn read_wm_class(conn: &RustConnection, window: Window) -> Option<Vec<String>> {
  let prop = conn
    .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)
    .ok()?
    .reply()
    .ok()?;
  let txt = String::from_utf8(prop.value).ok()?;
  let parts: Vec<String> = txt
    .split('\0')
    .filter(|s| !s.trim().is_empty())
    .map(|s| s.trim().to_string())
    .collect();
  if parts.is_empty() { None } else { Some(parts) }
}

pub fn slippi_devtools_port() -> u16 {
  env::var("SLIPPI_DEVTOOLS_PORT")
    .ok()
    .and_then(|s| s.parse::<u16>().ok())
    .unwrap_or(9223)
}

pub fn slippi_x11_connect() -> Result<(RustConnection, usize), String> {
  let display = target_display().ok();
  x11rb::connect(display.as_deref()).map_err(|e| e.to_string())
}

// ── CDP automation ──────────────────────────────────────────────────────

pub fn cdp_targets(port: u16) -> Result<Vec<CdpTarget>, String> {
  let url = format!("http://127.0.0.1:{port}/json/list");
  let resp = reqwest::blocking::get(&url).map_err(|e| format!("fetch {url}: {e}"))?;
  if !resp.status().is_success() {
    return Err(format!("DevTools list {url} returned {}", resp.status()));
  }
  resp.json::<Vec<CdpTarget>>().map_err(|e| format!("parse DevTools list: {e}"))
}

pub fn pick_slippi_target(targets: Vec<CdpTarget>) -> Option<CdpTarget> {
  let mut fallback: Option<CdpTarget> = None;
  for t in targets {
    if fallback.is_none() && t.kind.as_deref() == Some("page") {
      fallback = Some(t.clone());
    }
    let title = t.title.as_deref().unwrap_or_default().to_lowercase();
    if title.contains("slippi") {
      return Some(t);
    }
  }
  fallback
}

pub fn cdp_eval(ws_url: &str, expr: &str) -> Result<Value, String> {
  let (mut socket, _) = tungstenite::connect(ws_url).map_err(|e| format!("cdp connect {ws_url}: {e}"))?;
  let msg = json!({
    "id": 1,
    "method": "Runtime.evaluate",
    "params": {
      "expression": expr,
      "returnByValue": true,
      "awaitPromise": true,
    }
  });
  socket.send(Message::Text(msg.to_string())).map_err(|e| e.to_string())?;

  loop {
    let msg = socket.read().map_err(|e| e.to_string())?;
    if let Message::Text(txt) = msg {
      if let Ok(val) = serde_json::from_str::<Value>(&txt) {
        if val.get("id").and_then(|v| v.as_i64()) == Some(1) {
          if let Some(err) = val.get("error") {
            return Err(format!("cdp eval error: {err}"));
          }
          if let Some(result) = val
            .get("result")
            .and_then(|r| r.get("result"))
            .and_then(|r| r.get("value"))
          {
            return Ok(result.clone());
          }
        }
      }
    }
  }
}

pub fn scrape_slippi_via_cdp(port: u16) -> Result<Vec<SlippiStream>, String> {
  let targets = cdp_targets(port)?;
  let target = pick_slippi_target(targets).ok_or_else(|| "No DevTools targets found; is Slippi running with --remote-debugging-port?".to_string())?;
  let ws_url = target.ws_url.ok_or_else(|| "Target missing webSocketDebuggerUrl".to_string())?;

  let expr = r#"
    (() => {
      const cards = Array.from(document.querySelectorAll('.css-7xs1xn, [data-testid="spectate-card"], .css-o8b25d .MuiPaper-root'));
      return cards.map((c, idx) => {
        const text = (c.innerText || '').split('\n').map(t => t.trim()).filter(Boolean);
        const lower = text.map(t => t.toLowerCase());
        const playingTokens = [
          'in game',
          'playing',
          'in progress',
          'in-progress',
          'in match',
          'match in progress',
          'game in progress',
        ];
        const idleTokens = ['in lobby', 'lobby', 'waiting', 'idle', 'menu'];
        const hasPlaying = lower.some(line => playingTokens.some(token => line.includes(token)));
        const hasIdle = lower.some(line => idleTokens.some(token => line.includes(token)));
        const isPlaying = hasPlaying && !hasIdle;
        const name = text[0] || null;
        const code = text.find(t => t.includes('#')) || null;
        return {
          id: c.id || `card-${idx}`,
          name,
          code,
          isPlaying,
          text,
        };
      });
    })()
  "#;

  let value = cdp_eval(&ws_url, expr)?;
  let arr = value.as_array().ok_or_else(|| "Unexpected CDP eval result (not array)".to_string())?;

  let mut out = vec![];
  for (idx, item) in arr.iter().enumerate() {
    let name = item.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
    let code = item.get("code").and_then(|v| v.as_str()).map(|s| s.to_string());
    let is_playing = item.get("isPlaying").and_then(|v| v.as_bool());
    let id = item
      .get("id")
      .and_then(|v| v.as_str())
      .map(|s| s.to_string())
      .unwrap_or_else(|| format!("card-{idx}"));

    out.push(SlippiStream {
      id,
      window_title: target.title.clone(),
      p1_tag: name.clone(),
      p2_tag: None,
      p1_code: code.clone(),
      p2_code: None,
      startgg_entrant_id: None,
      replay_path: None,
      is_playing,
      source: Some(format!("cdp port {port}")),
      startgg_set: None,
    });
  }
  Ok(out)
}

pub fn click_slippi_refresh(port: u16) -> Result<(), String> {
  let targets = cdp_targets(port)?;
  let target = pick_slippi_target(targets).ok_or_else(|| "No DevTools targets found; is Slippi running with --remote-debugging-port?".to_string())?;
  let ws_url = target.ws_url.ok_or_else(|| "Target missing webSocketDebuggerUrl".to_string())?;

  fn try_click_refresh(ws_url: &str) -> Result<(bool, Option<String>), String> {
    let expr = r#"
      (() => {
        const buttons = Array.from(document.querySelectorAll('button'));
        const byTestId = buttons.find(btn => btn.querySelector('[data-testid="SyncIcon"]'));
        const byText = buttons.find(btn => (btn.innerText || '').toLowerCase().includes('refresh'));
        const target = byTestId || byText;
        if (target) {
          target.click();
          return { clicked: true, label: target.innerText || null };
        }
        return { clicked: false, reason: 'refresh button not found' };
      })()
    "#;

    let result = cdp_eval(ws_url, expr)?;
    let clicked = result.get("clicked").and_then(|v| v.as_bool()).unwrap_or(false);
    let reason = result.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string());
    Ok((clicked, reason))
  }

  let (clicked, reason) = try_click_refresh(&ws_url)?;
  if clicked {
    return Ok(());
  }

  // If refresh button wasn't present (e.g., not on Spectate tab), try to navigate first.
  let nav_expr = r#"
    (() => {
      const anchors = Array.from(document.querySelectorAll('a'));
      const byHref = anchors.find(a => (a.getAttribute('href') || '').includes('/spectate'));
      const byLabel = anchors.find(a => (a.getAttribute('aria-label') || '').toLowerCase().includes('spectate'));
      const target = byHref || byLabel;
      if (target) {
        target.click();
        return { clicked: true };
      }
      return { clicked: false, reason: 'spectate link not found' };
    })()
  "#;

  let nav_result = cdp_eval(&ws_url, nav_expr)?;
  let nav_clicked = nav_result.get("clicked").and_then(|v| v.as_bool()).unwrap_or(false);
  if !nav_clicked {
    let nav_reason = nav_result.get("reason").and_then(|v| v.as_str()).unwrap_or("unknown reason");
    let reason_txt = reason.unwrap_or_else(|| "refresh button missing".into());
    return Err(format!(
      "Failed to click Slippi refresh: {reason_txt}; also could not switch to Spectate: {nav_reason}"
    ));
  }

  // Let navigation settle, then try the refresh button again.
  sleep(Duration::from_millis(600));
  let (clicked_after_nav, reason_after_nav) = try_click_refresh(&ws_url)?;
  if clicked_after_nav {
    Ok(())
  } else {
    let reason_txt = reason_after_nav.unwrap_or_else(|| "refresh button still missing after Spectate click".into());
    Err(format!("Failed to click Slippi refresh after Spectate: {reason_txt}"))
  }
}

pub fn click_slippi_watch(port: u16, target_id: String, target_code: Option<String>, target_tag: Option<String>) -> Result<(), String> {
  let targets = cdp_targets(port)?;
  let target = pick_slippi_target(targets).ok_or_else(|| "No DevTools targets found; is Slippi running with --remote-debugging-port?".to_string())?;
  let ws_url = target.ws_url.ok_or_else(|| "Target missing webSocketDebuggerUrl".to_string())?;

  let id_json = serde_json::to_string(&target_id).map_err(|e| e.to_string())?;
  let code_json = serde_json::to_string(&target_code).map_err(|e| e.to_string())?;
  let tag_json = serde_json::to_string(&target_tag).map_err(|e| e.to_string())?;

  let expr = format!(
    r#"
      (() => {{
        const targetId = {id};
        const targetCode = {code};
        const targetTag = {tag};
        const cards = Array.from(document.querySelectorAll('.css-7xs1xn, [data-testid="spectate-card"], .css-o8b25d .MuiPaper-root'));
        const normalize = (txt) => (txt || '').toLowerCase().trim();

        let card = cards.find(c => c.id === targetId);
        if (!card && targetCode) {{
          card = cards.find(c => normalize(c.innerText).includes(normalize(targetCode)));
        }}
        if (!card && targetTag) {{
          card = cards.find(c => normalize(c.innerText).includes(normalize(targetTag)));
        }}
        if (!card) {{
          return {{ clicked: false, reason: 'card not found', count: cards.length }};
        }}

        const buttons = Array.from(card.querySelectorAll('button'));
        const byIcon = buttons.find(btn => btn.querySelector('[data-testid="PlayCircleOutlineIcon"]'));
        const byText = buttons.find(btn => normalize(btn.innerText).includes('watch'));
        const btn = byIcon || byText || buttons[0];
        if (!btn) {{
          return {{ clicked: false, reason: 'watch button not found in card' }};
        }}
        btn.click();
        return {{ clicked: true, label: btn.innerText || null, cardId: card.id || null }};
      }})()
    "#,
    id = id_json,
    code = code_json,
    tag = tag_json
  );

  let result = cdp_eval(&ws_url, &expr)?;
  let clicked = result.get("clicked").and_then(|v| v.as_bool()).unwrap_or(false);
  if clicked {
    Ok(())
  } else {
    let reason = result.get("reason").and_then(|v| v.as_str()).unwrap_or("unknown reason");
    Err(format!("Failed to click Slippi Watch: {reason}"))
  }
}

// ── Tauri commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn find_slippi_launcher_window() -> Result<Option<SlippiWindowInfo>, String> {
  if mock_streams_enabled() || app_test_mode_enabled() {
    return Ok(Some(SlippiWindowInfo {
      id: 0,
      title: Some("Mock Slippi Launcher".to_string()),
      x: 0,
      y: 0,
      width: 1280,
      height: 720,
      screen: 0,
    }));
  }

  let (conn, screen_num) = slippi_x11_connect()?;
  let root = conn.setup().roots[screen_num].root;
  let tree = conn
    .query_tree(root)
    .map_err(|e| e.to_string())?
    .reply()
    .map_err(|e| e.to_string())?;

  let mut best: Option<(SlippiWindowInfo, u32)> = None;

  for win in tree.children {
    let title = read_window_title(&conn, win).unwrap_or_default();
    let wm_class = read_wm_class(&conn, win).unwrap_or_default();
    let title_lower = title.to_lowercase();
    let class_lower: Vec<String> = wm_class.iter().map(|c| c.to_lowercase()).collect();

    let is_match = title_lower.contains("slippi launcher")
      || (title_lower.contains("slippi") && title_lower.contains("launcher"))
      || class_lower.iter().any(|c| c.contains("slippi-launcher") || c.contains("slippi launcher") || c.contains("slippi"));
    if !is_match {
      continue;
    }

    let geo = conn
      .get_geometry(win)
      .map_err(|e| e.to_string())?
      .reply()
      .map_err(|e| e.to_string())?;

    let area = (geo.width as u32) * (geo.height as u32);
    if geo.width < 200 || geo.height < 200 {
      // Likely a tiny helper window; skip unless no other candidates.
      if best.is_some() {
        continue;
      }
    }

    let info = SlippiWindowInfo {
      id: win,
      title: if title.is_empty() { None } else { Some(title) },
      x: geo.x.into(),
      y: geo.y.into(),
      width: geo.width.into(),
      height: geo.height.into(),
      screen: screen_num as u32,
    };

    match &best {
      Some((_, best_area)) if area <= *best_area => {}
      _ => best = Some((info, area)),
    }
  }

  Ok(best.map(|(info, _)| info))
}

/// Scan the Slippi Launcher window, screenshot it, OCR the contents, and try to extract tags/connect codes.
#[tauri::command]
pub fn scan_slippi_streams(
  test_state: State<'_, SharedTestState>,
  replay_cache: State<'_, SharedOverlayCache>,
) -> Result<Vec<SlippiStream>, String> {
  if mock_streams_enabled() {
    return test_mode_streams();
  }
  if app_test_mode_enabled() {
    let mut guard = test_state.lock().map_err(|e| e.to_string())?;
    if guard.broadcast_filter_enabled {
      return test_mode_broadcast_streams(&mut guard);
    }
    let streams = if !guard.spoof_streams.is_empty() {
      guard.spoof_streams.clone()
    } else {
      match test_mode_bracket_streams(&mut guard) {
        Ok(streams) if !streams.is_empty() => streams,
        _ => test_mode_streams_from_replays(&mut guard)?,
      }
    };
    return Ok(filter_broadcast_streams(&streams, &guard));
  }
  let devtools_port = slippi_devtools_port();
  let mut streams = scrape_slippi_via_cdp(devtools_port)?;
  let config = load_config_inner()?;
  let spectate = config.spectate_folder_path.trim();
  if !spectate.is_empty() {
    let dir = resolve_repo_path(spectate);
    let mut cache = replay_cache.lock().map_err(|e| e.to_string())?;
    let _ = update_replay_index(&mut cache, &dir);
    for stream in &mut streams {
      let Some(code) = stream.p1_code.as_deref() else {
        continue;
      };
      if let Some(path) = latest_replay_for_code(&cache, code) {
        stream.replay_path = Some(path.to_string_lossy().to_string());
        if stream.is_playing == Some(true) {
          if let Some(opponent) = find_opponent_code_in_replay(code, &path) {
            stream.p2_code = Some(opponent.clone());
            stream.p2_tag = Some(tag_from_code(&opponent));
          }
        }
      }
    }
  }
  Ok(streams)
}

#[tauri::command]
pub fn refresh_slippi_launcher() -> Result<(), String> {
  if mock_streams_enabled() || app_test_mode_enabled() {
    return Ok(());
  }
  let devtools_port = slippi_devtools_port();
  click_slippi_refresh(devtools_port)
}

#[tauri::command]
pub fn watch_slippi_stream(stream_id: String, p1_code: Option<String>, p1_tag: Option<String>) -> Result<(), String> {
  if mock_streams_enabled() || app_test_mode_enabled() {
    return Ok(());
  }
  let devtools_port = slippi_devtools_port();
  click_slippi_watch(devtools_port, stream_id, p1_code, p1_tag)
}

#[tauri::command]
pub fn assign_stream_to_setup(
  setup_id: u32,
  stream: SlippiStream,
  launch: Option<bool>,
  store: State<'_, SharedSetupStore>,
  test_state: State<'_, SharedTestState>,
) -> Result<AssignStreamResult, String> {
  let should_launch = launch.unwrap_or(true);
  let test_mode = app_test_mode_enabled();
  let (changed_assignments, processes_to_stop, pids_to_stop, updated_setups) = {
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    if !guard.setups.iter().any(|s| s.id == setup_id) {
      return Err("Setup not found.".to_string());
    }

    let target_prev_stream = guard
      .setups
      .iter()
      .find(|s| s.id == setup_id)
      .and_then(|s| s.assigned_stream.clone());
    let can_swap = target_prev_stream
      .as_ref()
      .map(|assigned| assigned.id != stream.id)
      .unwrap_or(false);

    let mut source_ids: Vec<u32> = guard
      .setups
      .iter()
      .filter_map(|s| {
        s.assigned_stream
          .as_ref()
          .filter(|assigned| assigned.id == stream.id)
          .map(|_| s.id)
      })
      .collect();
    source_ids.sort_unstable();
    let swap_source_id = if can_swap {
      source_ids.iter().copied().find(|id| *id != setup_id)
    } else {
      None
    };

    let mut assignments: HashMap<u32, Option<SlippiStream>> = HashMap::new();
    assignments.insert(setup_id, Some(stream.clone()));
    if let Some(source_id) = swap_source_id {
      assignments.insert(source_id, target_prev_stream.clone());
    }
    for id in source_ids {
      if id != setup_id && Some(id) != swap_source_id {
        assignments.insert(id, None);
      }
    }

    let mut changed_assignments = Vec::new();
    for (id, new_assignment) in assignments.iter() {
      let setup = guard
        .setups
        .iter_mut()
        .find(|s| s.id == *id)
        .ok_or_else(|| "Setup not found.".to_string())?;
      let prev_id = setup.assigned_stream.as_ref().map(|s| s.id.clone());
      let prev_playing = setup.assigned_stream.as_ref().and_then(|s| s.is_playing);
      let prev_replay = setup.assigned_stream.as_ref().and_then(|s| s.replay_path.clone());
      let next_id = new_assignment.as_ref().map(|s| s.id.clone());
      let next_playing = new_assignment.as_ref().and_then(|s| s.is_playing);
      let next_replay = new_assignment.as_ref().and_then(|s| s.replay_path.clone());
      let replay_changed = test_mode && prev_replay != next_replay;
      if prev_id != next_id || prev_playing != next_playing || replay_changed {
        changed_assignments.push((*id, new_assignment.clone()));
      }
      setup.assigned_stream = new_assignment.clone();
    }

    if should_launch {
      let has_target_change = changed_assignments.iter().any(|(id, _)| *id == setup_id);
      if !has_target_change {
        if let Some(assigned) = guard
          .setups
          .iter()
          .find(|s| s.id == setup_id)
          .and_then(|s| s.assigned_stream.clone())
        {
          changed_assignments.push((setup_id, Some(assigned)));
        }
      }
    }

    let mut processes_to_stop = Vec::new();
    let mut pids_to_stop = Vec::new();
    for (id, _) in &changed_assignments {
      if should_launch {
        if let Some(child) = guard.processes.remove(id) {
          processes_to_stop.push(child);
        }
        if let Some(pid) = guard.process_pids.remove(id) {
          pids_to_stop.push(pid);
        }
      }
    }

    let updated_setups = guard.setups.clone();
    (changed_assignments, processes_to_stop, pids_to_stop, updated_setups)
  };

  if should_launch {
    for child in processes_to_stop {
      stop_dolphin_child(child)?;
    }
    for pid in pids_to_stop {
      stop_process_by_pid(pid)?;
    }
  }

  let replay_map = if should_launch && test_mode {
    let guard = test_state.lock().map_err(|e| e.to_string())?;
    guard.spoof_replays.clone()
  } else {
    HashMap::new()
  };

  let mut warning_messages = Vec::new();
  let mut new_children: Vec<(u32, std::process::Child)> = Vec::new();
  let mut new_pids: Vec<(u32, u32)> = Vec::new();

  if should_launch {
    for (id, assignment) in changed_assignments {
      let Some(assigned_stream) = assignment else { continue; };
      if test_mode {
        if assigned_stream.is_playing == Some(true) {
          let replay = assigned_stream
            .replay_path
            .as_deref()
            .map(resolve_repo_path)
            .or_else(|| replay_map.get(&assigned_stream.id).cloned());
          let Some(replay) = replay else {
            warning_messages.push(format!(
              "No test replay mapped for {} (setup {}).",
              assigned_stream.id, id
            ));
            continue;
          };
          match launch_dolphin_playback_for_setup_internal(id, &replay) {
            Ok(child) => new_children.push((id, child)),
            Err(err) => warning_messages.push(format!("Setup {id}: {err}")),
          }
        } else {
          match launch_dolphin_for_setup_internal(id) {
            Ok(child) => new_children.push((id, child)),
            Err(err) => warning_messages.push(format!("Setup {id}: {err}")),
          }
        }
      } else {
        let slippi_auto = slippi_launches_dolphin();
        let existing_pids = if slippi_auto {
          Some(list_dolphin_like_pids())
        } else {
          None
        };
        let mut wrapper_path: Option<PathBuf> = None;
        let mut label_path: Option<PathBuf> = None;
        if slippi_auto {
      match ensure_slippi_wrapper() {
            Ok(path) => wrapper_path = Some(path),
            Err(err) => warning_messages.push(format!("Setup {id}: {err}")),
          }
          if let Some(wrapper) = wrapper_path.as_ref() {
            if let Err(err) = ensure_slippi_playback_wrapper(wrapper) {
              warning_messages.push(format!("Setup {id}: {err}"));
            }
          }
          match write_slippi_watch_label(id) {
            Ok(path) => label_path = Some(path),
            Err(err) => warning_messages.push(format!("Setup {id}: {err}")),
          }
        }

        if let Err(err) = watch_slippi_stream(
          assigned_stream.id.clone(),
          assigned_stream.p1_code.clone(),
          assigned_stream.p1_tag.clone(),
        ) {
          warning_messages.push(format!("Setup {id}: {err}"));
          if let Some(path) = label_path.as_ref() {
            clear_slippi_watch_label(path);
          }
          continue;
        }
        if slippi_auto {
          let Some(before) = existing_pids else {
            continue;
          };
          let mut found_dolphin = false;
          match find_new_dolphin_cmdline_any(&before, Duration::from_secs(10)) {
            Ok(Some((pid, _cmdline))) => {
              new_pids.push((id, pid));
              found_dolphin = true;
            }
            Ok(None) => {
              warning_messages.push(format!(
                "Setup {id}: Slippi watch launched no Dolphin process."
              ));
            }
            Err(err) => warning_messages.push(format!("Setup {id}: {err}")),
          }
          if found_dolphin {
            if let Some(path) = label_path.as_ref() {
              if path.is_file() {
                clear_slippi_watch_label(path);
                if let Some(wrapper) = wrapper_path.as_ref() {
                  warning_messages.push(format!(
                    "Setup {id}: Slippi Dolphin wrapper not detected. Set the Slippi Dolphin path to {}.",
                    wrapper.display()
                  ));
                } else {
                  warning_messages.push(format!(
                    "Setup {id}: Slippi Dolphin wrapper not detected. Set the Slippi Dolphin path to the NMST wrapper."
                  ));
                }
              }
            }
          }
          continue;
        }

        match launch_dolphin_for_setup_internal(id) {
          Ok(child) => new_children.push((id, child)),
          Err(err) => warning_messages.push(format!("Setup {id}: {err}")),
        }
      }
    }
  }

  if !new_children.is_empty() || !new_pids.is_empty() {
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    for (id, child) in new_children {
      guard.processes.insert(id, child);
    }
    for (id, pid) in new_pids {
      guard.process_pids.insert(id, pid);
    }
  }

  let warning = if !should_launch || warning_messages.is_empty() {
    None
  } else {
    Some(warning_messages.join(" "))
  };

  Ok(AssignStreamResult {
    setups: updated_setups,
    warning,
  })
}

#[tauri::command]
pub fn clear_setup_assignment(
  setup_id: u32,
  stop: Option<bool>,
  store: State<'_, SharedSetupStore>,
) -> Result<Setup, String> {
  let should_stop = stop.unwrap_or(true);
  let (setup, existing, existing_pid) = {
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    let setup = guard
      .setups
      .iter_mut()
      .find(|s| s.id == setup_id)
      .ok_or_else(|| "Setup not found.".to_string())?;
    setup.assigned_stream = None;
    let cloned = setup.clone();
    let (existing, existing_pid) = if should_stop {
      (
        guard.processes.remove(&setup_id),
        guard.process_pids.remove(&setup_id),
      )
    } else {
      (None, None)
    };
    (cloned, existing, existing_pid)
  };

  if should_stop {
    if let Some(child) = existing {
      stop_dolphin_child(child)?;
    }
    if let Some(pid) = existing_pid {
      stop_process_by_pid(pid)?;
    }
  }

  Ok(setup)
}

#[tauri::command]
pub fn launch_slippi_app() -> Result<(), String> {
  let appimage = slippi_appimage_path()?;
  let devtools_port = slippi_devtools_port();

  let mut cmd = Command::new(&appimage);
  cmd.arg("--no-sandbox")
    .arg("--disable-setuid-sandbox")
    .arg(format!("--remote-debugging-port={devtools_port}"));
  cmd.stdin(Stdio::null());
  cmd.stdout(Stdio::null());
  cmd.stderr(Stdio::null());

  if let Some(dir) = appimage.parent() {
    cmd.current_dir(dir);
  }

  cmd.spawn().map_err(|e| format!("launch Slippi: {e}"))?;
  Ok(())
}

#[tauri::command]
pub fn relaunch_slippi_app() -> Result<(), String> {
  let appimage = slippi_appimage_path()?;
  let existing = list_slippi_pids(&appimage);
  let mut errors = Vec::new();
  for pid in existing {
    if let Err(err) = stop_process_by_pid(pid) {
      errors.push(err);
    }
  }
  if !errors.is_empty() {
    return Err(errors.join(" "));
  }
  sleep(Duration::from_millis(400));
  launch_slippi_app()
}
