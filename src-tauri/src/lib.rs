use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
  collections::{HashMap, HashSet},
  env,
  fs,
  io::{BufRead, BufReader},
  path::{Path, PathBuf},
  process::{Child, Command, Stdio},
  sync::Mutex,
  thread::sleep,
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use tungstenite::Message;
use x11rb::{
  connection::Connection,
  protocol::xproto::{AtomEnum, ConnectionExt, Window},
  rust_connection::RustConnection,
};
use tauri::{Emitter, State};
mod startgg_sim;
use startgg_sim::{StartggSim, StartggSimConfig, StartggSimEntrantConfig, StartggSimEventConfig, StartggSimPhaseConfig, StartggSimSimulationConfig, StartggSimState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStub {
  pub id: u8,
  pub name: String,
  pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
  pub id: u32,
  pub name: String,
  pub assigned_stream: Option<SlippiStream>,
}

#[derive(Default)]
struct SetupStore {
  setups: Vec<Setup>,
  next_id: u32,
  processes: HashMap<u32, Child>,
}

impl SetupStore {
  fn bootstrap_from_existing() -> Self {
    SetupStore {
      setups: vec![Setup {
        id: 1,
        name: "Setup 1".to_string(),
        assigned_stream: None,
      }],
      next_id: 2,
      processes: HashMap::new(),
    }
  }
}

#[derive(Default)]
struct TestModeState {
  spoof_streams: Vec<SlippiStream>,
  spoof_replays: HashMap<String, PathBuf>,
  startgg_sim: Option<StartggSim>,
  startgg_config_path: Option<PathBuf>,
}

#[derive(Debug)]
struct DolphinConfig {
  dolphin_path: PathBuf,
  ssbm_iso_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupsPayload {
  pub setups: Vec<SetupStub>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlippiStream {
  pub id: String,
  pub window_title: Option<String>,
  pub p1_tag: Option<String>,
  pub p2_tag: Option<String>,
  pub p1_code: Option<String>,
  pub p2_code: Option<String>,
  pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlippiWindowInfo {
  pub id: u32,
  pub title: Option<String>,
  pub x: i32,
  pub y: i32,
  pub width: u32,
  pub height: u32,
  pub screen: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BracketConfigInfo {
  pub name: String,
  pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpoofReplayResult {
  pub started: usize,
  pub missing: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
  pub dolphin_path: String,
  pub ssbm_iso_path: String,
  pub slippi_launcher_path: String,
  pub spectate_folder_path: String,
  pub test_mode: bool,
  pub test_bracket_path: String,
  pub auto_complete_bracket: bool,
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      dolphin_path: String::new(),
      ssbm_iso_path: String::new(),
      slippi_launcher_path: String::new(),
      spectate_folder_path: String::new(),
      test_mode: false,
      test_bracket_path: "test_brackets/test_bracket_2.json".to_string(),
      auto_complete_bracket: true,
    }
  }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct CdpTarget {
  id: Option<String>,
  title: Option<String>,
  url: Option<String>,
  #[serde(rename = "type")]
  kind: Option<String>,
  #[serde(rename = "webSocketDebuggerUrl")]
  ws_url: Option<String>,
}

#[derive(Debug)]
struct TestStreamSpec {
  stream: SlippiStream,
  replay_path: PathBuf,
}

/// Temporary command so the frontend has a shape to integrate against.
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
fn list_setups(store: State<'_, Mutex<SetupStore>>) -> Result<Vec<Setup>, String> {
  let guard = store.lock().map_err(|e| e.to_string())?;
  Ok(guard.setups.clone())
}

#[tauri::command]
fn create_setup(store: State<'_, Mutex<SetupStore>>) -> Result<Setup, String> {
  let mut guard = store.lock().map_err(|e| e.to_string())?;
  let setup_id = guard.next_id;
  guard.next_id += 1;
  let setup = Setup {
    id: setup_id,
    name: format!("Setup {setup_id}"),
    assigned_stream: None,
  };
  guard.setups.push(setup.clone());
  Ok(setup)
}

#[tauri::command]
fn delete_setup(id: u32, store: State<'_, Mutex<SetupStore>>) -> Result<(), String> {
  let existing = {
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    guard.setups.retain(|s| s.id != id);
    guard.processes.remove(&id)
  };
  if let Some(child) = existing {
    stop_dolphin_child(child)?;
  }
  Ok(())
}

fn read_window_title(conn: &RustConnection, window: Window) -> Option<String> {
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

fn read_wm_class(conn: &RustConnection, window: Window) -> Option<Vec<String>> {
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

fn slippi_devtools_port() -> u16 {
  env::var("SLIPPI_DEVTOOLS_PORT")
    .ok()
    .and_then(|s| s.parse::<u16>().ok())
    .unwrap_or(9223)
}

fn env_flag_true(key: &str) -> bool {
  match env::var(key) {
    Ok(value) => {
      let value = value.trim().to_ascii_lowercase();
      matches!(value.as_str(), "1" | "true" | "yes" | "on")
    }
    Err(_) => false,
  }
}

fn env_flag_true_default(key: &str, default: bool) -> bool {
  match env::var(key) {
    Ok(value) => {
      let value = value.trim().to_ascii_lowercase();
      matches!(value.as_str(), "1" | "true" | "yes" | "on")
    }
    Err(_) => default,
  }
}

fn repo_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .map(|path| path.to_path_buf())
    .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn slippi_mock_streams_path() -> Option<PathBuf> {
  env::var("SLIPPI_MOCK_STREAMS_PATH")
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .map(PathBuf::from)
}

fn mock_streams_enabled() -> bool {
  env_flag_true("SLIPPI_MOCK_STREAMS") || slippi_mock_streams_path().is_some()
}

fn app_test_mode_enabled() -> bool {
  match load_config_inner() {
    Ok(config) => config.test_mode,
    Err(_) => false,
  }
}

fn default_mock_streams_path() -> PathBuf {
  repo_root().join("test_files").join("mock_streams.json")
}

fn load_mock_streams(path: &PathBuf) -> Result<Vec<SlippiStream>, String> {
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
    if stream.source.is_none() {
      stream.source = Some("mock".to_string());
    }
  }
  Ok(streams)
}

fn test_mode_streams() -> Result<Vec<SlippiStream>, String> {
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
        source: Some("mock".to_string()),
      },
      SlippiStream {
        id: "mock-2".to_string(),
        window_title: Some("Mock Slippi Launcher".to_string()),
        p1_tag: Some("ARMADA".to_string()),
        p2_tag: Some("HBOX".to_string()),
        p1_code: Some("ARMADA#321".to_string()),
        p2_code: Some("HBOX#888".to_string()),
        source: Some("mock".to_string()),
      },
      SlippiStream {
        id: "mock-3".to_string(),
        window_title: Some("Mock Slippi Launcher".to_string()),
        p1_tag: Some("LEFFEN".to_string()),
        p2_tag: Some("PLUP".to_string()),
        p1_code: Some("LEFFEN#555".to_string()),
        p2_code: Some("PLUP#222".to_string()),
        source: Some("mock".to_string()),
      },
    ]);
  }
  build_test_streams().map(|items| items.into_iter().map(|item| item.stream).collect())
}

fn config_path() -> PathBuf {
  repo_root().join("config.json")
}

fn load_config_inner() -> Result<AppConfig, String> {
  let path = config_path();
  if !path.is_file() {
    return Ok(AppConfig::default());
  }
  let data = fs::read_to_string(&path).map_err(|e| format!("read config {}: {e}", path.display()))?;
  serde_json::from_str::<AppConfig>(&data).map_err(|e| format!("parse config {}: {e}", path.display()))
}

fn save_config_inner(config: AppConfig) -> Result<AppConfig, String> {
  let path = config_path();
  let payload = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
  fs::write(&path, payload).map_err(|e| format!("write config {}: {e}", path.display()))?;
  Ok(config)
}

fn resolve_repo_path(raw: &str) -> PathBuf {
  let path = PathBuf::from(raw);
  if path.is_absolute() {
    path
  } else {
    repo_root().join(path)
  }
}

fn node_path_delimiter() -> char {
  if cfg!(windows) { ';' } else { ':' }
}

fn split_node_path(raw: &str) -> Vec<PathBuf> {
  raw
    .split(node_path_delimiter())
    .map(|part| part.trim())
    .filter(|part| !part.is_empty())
    .map(PathBuf::from)
    .collect()
}

fn contains_slippi_module(path: &Path) -> bool {
  path.join("@slippi").join("slippi-js").is_dir()
}

fn candidate_node_modules() -> Vec<PathBuf> {
  let mut out = Vec::new();
  let local = repo_root().join("node_modules");
  if local.is_dir() {
    out.push(local);
  }
  if let Some(parent) = repo_root().parent() {
    let alt = parent.join("replay_archiver").join("node_modules");
    if alt.is_dir() {
      out.push(alt);
    }
  }
  out
}

fn build_node_path() -> Result<String, String> {
  let mut entries: Vec<PathBuf> = Vec::new();
  let mut has_module = false;

  if let Ok(existing) = env::var("NODE_PATH") {
    for path in split_node_path(&existing) {
      if contains_slippi_module(&path) {
        has_module = true;
      }
      entries.push(path);
    }
  }

  for candidate in candidate_node_modules() {
    if contains_slippi_module(&candidate) {
      has_module = true;
    }
    entries.push(candidate);
  }

  if !has_module {
    return Err(
      "Unable to locate @slippi/slippi-js. Install it in this repo (node_modules), in ../replay_archiver, or set NODE_PATH to a node_modules folder that contains it.".to_string(),
    );
  }

  let mut seen = HashSet::new();
  let mut unique = Vec::new();
  for entry in entries {
    let key = entry.to_string_lossy().to_string();
    if seen.insert(key.clone()) {
      unique.push(key);
    }
  }

  Ok(unique.join(&node_path_delimiter().to_string()))
}

fn test_config_path() -> PathBuf {
  if let Ok(raw) = env::var("SLIPPI_TEST_CONFIG_PATH") {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
      return PathBuf::from(trimmed);
    }
  }
  repo_root().join("test_config.json")
}

fn default_test_folders() -> Vec<String> {
  vec![
    "test_files/replays/aklo".to_string(),
    "test_files/replays/axe".to_string(),
    "test_files/replays/nomad".to_string(),
    "test_files/replays/rookie".to_string(),
    "test_files/replays/shiz".to_string(),
  ]
}

fn load_test_folder_paths() -> Result<Vec<PathBuf>, String> {
  let config_path = test_config_path();
  let folders: Vec<String> = if config_path.is_file() {
    let data = fs::read_to_string(&config_path)
      .map_err(|e| format!("read test config {}: {e}", config_path.display()))?;
    let value: Value = serde_json::from_str(&data)
      .map_err(|e| format!("parse test config {}: {e}", config_path.display()))?;
    if let Some(arr) = value.as_array() {
      arr.iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect()
    } else if let Some(arr) = value.get("folders").and_then(|v| v.as_array()) {
      arr.iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect()
    } else {
      return Err(format!(
        "Test config {} must be an array of folder paths or an object with a \"folders\" array.",
        config_path.display()
      ));
    }
  } else {
    default_test_folders()
  };

  if folders.is_empty() {
    return Err(format!(
      "Test config {} contains no folders.",
      config_path.display()
    ));
  }

  let mut out = Vec::new();
  for raw in folders {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
      continue;
    }
    let abs = resolve_repo_path(trimmed);
    if !abs.is_dir() {
      return Err(format!("Test folder not found: {}", abs.display()));
    }
    out.push(abs);
  }

  if out.is_empty() {
    return Err(format!(
      "Test config {} did not resolve to any valid folders.",
      config_path.display()
    ));
  }
  Ok(out)
}

fn collect_slp_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
  let mut files = Vec::new();
  let entries = fs::read_dir(dir).map_err(|e| format!("read dir {}: {e}", dir.display()))?;
  for entry in entries {
    let entry = entry.map_err(|e| format!("read dir entry {}: {e}", dir.display()))?;
    let path = entry.path();
    if !path.is_file() {
      continue;
    }
    let ext = path
      .extension()
      .and_then(|s| s.to_str())
      .unwrap_or("")
      .to_ascii_lowercase();
    if ext == "slp" || ext == "slippi" {
      files.push(path);
    }
  }
  files.sort();
  Ok(files)
}


fn extract_connect_codes(bytes: &[u8]) -> Vec<String> {
  let mut out = Vec::new();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'#' {
      let mut start = i;
      while start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
        start -= 1;
      }
      let mut end = i + 1;
      while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
      }
      let left_len = i - start;
      let right_len = end.saturating_sub(i + 1);
      if (2..=12).contains(&left_len) && (3..=4).contains(&right_len) {
        let left = &bytes[start..i];
        let right = &bytes[i + 1..end];
        if left.iter().all(|b| b.is_ascii_alphanumeric()) && right.iter().all(|b| b.is_ascii_digit()) {
          let code = format!(
            "{}#{}",
            String::from_utf8_lossy(left),
            String::from_utf8_lossy(right)
          );
          out.push(code);
        }
      }
      i = end;
    } else {
      i += 1;
    }
  }
  out
}


fn most_common_connect_code(files: &[PathBuf]) -> Result<String, String> {
  let mut counts: HashMap<String, usize> = HashMap::new();
  for file in files {
    let bytes = fs::read(file)
      .map_err(|e| format!("read replay {}: {e}", file.display()))?;
    let codes = extract_connect_codes(&bytes);
    let mut seen: HashSet<String> = HashSet::new();
    for code in codes {
      if seen.insert(code.clone()) {
        *counts.entry(code).or_insert(0) += 1;
      }
    }
  }
  counts
    .into_iter()
    .max_by_key(|(_, count)| *count)
    .map(|(code, _)| code)
    .ok_or_else(|| "No connect codes found in replays.".to_string())
}

fn find_opponent_code(primary: &str, files: &[PathBuf]) -> Option<String> {
  for file in files {
    let bytes = fs::read(file).ok()?;
    let codes = extract_connect_codes(&bytes);
    for code in codes {
      if code != primary {
        return Some(code);
      }
    }
  }
  None
}

fn tag_from_code(code: &str) -> String {
  code.split('#').next().unwrap_or(code).to_string()
}

fn build_test_streams() -> Result<Vec<TestStreamSpec>, String> {
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
    let stream = SlippiStream {
      id: format!("test-{}", folder_name),
      window_title: Some("Test Mode".to_string()),
      p1_tag,
      p2_tag,
      p1_code: Some(primary),
      p2_code: opponent,
      source: Some(format!("test:{}", folder_name)),
    };

    out.push(TestStreamSpec {
      stream,
      replay_path: replays[0].clone(),
    });
  }

  if out.is_empty() {
    return Err("No test streams generated from configured folders.".to_string());
  }
  Ok(out)
}

fn startgg_sim_config_path() -> PathBuf {
  if let Ok(raw) = env::var("STARTGG_SIM_CONFIG_PATH") {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
      return PathBuf::from(trimmed);
    }
  }
  startgg_sim_configs_dir().join("test_bracket_2.json")
}

fn startgg_sim_configs_dir() -> PathBuf {
  repo_root().join("test_brackets")
}

fn resolve_startgg_sim_config_path(raw: &str) -> PathBuf {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return startgg_sim_config_path();
  }
  let path = PathBuf::from(trimmed);
  if path.is_absolute() {
    return path;
  }
  if trimmed.contains(std::path::MAIN_SEPARATOR) || trimmed.contains('/') {
    return repo_root().join(path);
  }
  startgg_sim_configs_dir().join(path)
}

fn build_default_startgg_sim_config() -> Result<StartggSimConfig, String> {
  let items = build_test_streams()?;
  let mut entrants = Vec::new();
  let mut seen_codes = HashSet::new();
  let mut next_id = 1u32;

  for item in items {
    let code = item
      .stream
      .p1_code
      .clone()
      .unwrap_or_else(|| format!("TEST#{}", next_id));
    if !seen_codes.insert(code.clone()) {
      continue;
    }
    let name = item
      .stream
      .p1_tag
      .clone()
      .unwrap_or_else(|| tag_from_code(&code));
    entrants.push(StartggSimEntrantConfig {
      id: next_id,
      name,
      slippi_code: code,
      seed: Some(next_id),
    });
    next_id += 1;
  }

  if entrants.is_empty() {
    return Err("No entrants available to build Start.gg sim config.".to_string());
  }

  Ok(StartggSimConfig {
    event: StartggSimEventConfig {
      id: "test-event-1".to_string(),
      name: "Test Melee Event".to_string(),
      slug: "test-melee-event".to_string(),
    },
    phases: vec![StartggSimPhaseConfig {
      id: "phase-1".to_string(),
      name: "Singles Bracket".to_string(),
      best_of: 3,
    }],
    entrants,
    simulation: StartggSimSimulationConfig::default(),
    reference_tournament_link: None,
    reference_sets: Vec::new(),
  })
}

fn load_startgg_sim_config() -> Result<StartggSimConfig, String> {
  let path = startgg_sim_config_path();
  if path.is_file() {
    return load_startgg_sim_config_from(&path);
  }

  let config = build_default_startgg_sim_config()?;
  let payload = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)
      .map_err(|e| format!("create startgg sim config dir {}: {e}", parent.display()))?;
  }
  fs::write(&path, payload)
    .map_err(|e| format!("write startgg sim config {}: {e}", path.display()))?;
  Ok(config)
}

fn load_startgg_sim_config_from(path: &Path) -> Result<StartggSimConfig, String> {
  if !path.is_file() {
    return Err(format!("Start.gg sim config not found at {}.", path.display()));
  }
  let data = fs::read_to_string(path)
    .map_err(|e| format!("read startgg sim config {}: {e}", path.display()))?;
  serde_json::from_str::<StartggSimConfig>(&data)
    .map_err(|e| format!("parse startgg sim config {}: {e}", path.display()))
}

fn init_startgg_sim(guard: &mut TestModeState, now: u64) -> Result<(), String> {
  if guard.startgg_sim.is_none() {
    let config = if let Some(path) = guard.startgg_config_path.clone() {
      load_startgg_sim_config_from(&path)?
    } else {
      load_startgg_sim_config()?
    };
    guard.startgg_sim = Some(StartggSim::new(config, now)?);
  }
  Ok(())
}

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_millis() as u64
}

fn load_env_file() {
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let env_path = manifest_dir.join("..").join(".env");
  if !env_path.is_file() {
    return;
  }
  let contents = match fs::read_to_string(&env_path) {
    Ok(data) => data,
    Err(_) => return,
  };
  for line in contents.lines() {
    if let Some((key, value)) = parse_env_line(line) {
      if env::var_os(&key).is_none() {
        env::set_var(key, value);
      }
    }
  }
}

fn parse_env_line(line: &str) -> Option<(String, String)> {
  let trimmed = line.trim();
  if trimmed.is_empty() || trimmed.starts_with('#') {
    return None;
  }
  let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
  let (key, raw_value) = trimmed.split_once('=')?;
  let key = key.trim();
  if key.is_empty() {
    return None;
  }
  let mut value = raw_value.trim();
  if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
    value = &value[1..value.len() - 1];
  } else if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
    value = &value[1..value.len() - 1];
  } else if let Some(idx) = value.find('#') {
    value = value[..idx].trim_end();
  }
  Some((key.to_string(), value.to_string()))
}

fn required_env_var(key: &str) -> Result<String, String> {
  match env::var(key) {
    Ok(value) => {
      let trimmed = value.trim();
      if trimmed.is_empty() {
        Err(format!("{key} is empty; set it in .env or the shell environment."))
      } else {
        Ok(trimmed.to_string())
      }
    }
    Err(_) => Err(format!("{key} is not set; set it in .env or the shell environment.")),
  }
}

fn dolphin_config() -> Result<DolphinConfig, String> {
  if let Ok(config) = load_config_inner() {
    let dolphin_raw = config.dolphin_path.trim();
    let iso_raw = config.ssbm_iso_path.trim();
    if !dolphin_raw.is_empty() && !iso_raw.is_empty() {
      let dolphin_path = resolve_repo_path(dolphin_raw);
      if !dolphin_path.is_file() {
        return Err(format!(
          "Dolphin binary not found at {}. Update Dolphin path in settings.",
          dolphin_path.display()
        ));
      }
      let ssbm_iso_path = resolve_repo_path(iso_raw);
      if !ssbm_iso_path.is_file() {
        return Err(format!(
          "SSBM ISO not found at {}. Update Melee ISO path in settings.",
          ssbm_iso_path.display()
        ));
      }
      return Ok(DolphinConfig { dolphin_path, ssbm_iso_path });
    }
  }

  let dolphin_path = PathBuf::from(required_env_var("DOLPHIN_PATH")?);
  if !dolphin_path.is_file() {
    return Err(format!(
      "Dolphin binary not found at {}. Set DOLPHIN_PATH to the file.",
      dolphin_path.display()
    ));
  }
  let ssbm_iso_path = PathBuf::from(required_env_var("SSBM_ISO_PATH")?);
  if !ssbm_iso_path.is_file() {
    return Err(format!(
      "SSBM ISO not found at {}. Set SSBM_ISO_PATH to the file.",
      ssbm_iso_path.display()
    ));
  }
  Ok(DolphinConfig { dolphin_path, ssbm_iso_path })
}

fn dolphin_exec_flag() -> String {
  env::var("DOLPHIN_EXEC_FLAG")
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "-e".to_string())
}

fn dolphin_batch_enabled() -> bool {
  env_flag_true_default("DOLPHIN_BATCH", true)
}

fn obs_gamecapture_enabled() -> bool {
  env_flag_true_default("USE_OBS_GAMECAPTURE", true)
}

fn find_in_path(command: &str) -> Option<PathBuf> {
  let path = env::var("PATH").ok()?;
  for entry in path.split(node_path_delimiter()) {
    let candidate = PathBuf::from(entry).join(command);
    if candidate.is_file() {
      return Some(candidate);
    }
  }
  None
}

fn obs_gamecapture_path() -> Option<PathBuf> {
  if let Ok(raw) = env::var("OBS_GAMECAPTURE") {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
      let path = PathBuf::from(trimmed);
      if path.is_file() {
        return Some(path);
      }
    }
  }
  find_in_path("obs-gamecapture")
}

fn exe_override_lib_path() -> Option<PathBuf> {
  let path = repo_root().join("scripts").join("vkcapture_exe_override.so");
  if path.is_file() { Some(path) } else { None }
}

fn apply_ld_preload(cmd: &mut Command, lib_path: &Path) {
  let lib = lib_path.to_string_lossy().to_string();
  let merged = match env::var("LD_PRELOAD") {
    Ok(existing) if !existing.trim().is_empty() => format!("{lib}:{existing}"),
    _ => lib,
  };
  cmd.env("LD_PRELOAD", merged);
}

fn setup_user_dir(setup_id: u32) -> Result<PathBuf, String> {
  let dir = env::temp_dir().join(format!("slippi-setup-{setup_id}"));
  fs::create_dir_all(&dir)
    .map_err(|e| format!("create Dolphin user dir {}: {e}", dir.display()))?;
  Ok(dir)
}

fn write_gamesettings(user_dir: &Path) -> Result<(), String> {
  let settings_id = env::var("DOLPHIN_GAMESETTINGS_ID")
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "GALE01r2".to_string());
  let settings_dir = user_dir.join("GameSettings");
  fs::create_dir_all(&settings_dir)
    .map_err(|e| format!("create GameSettings dir {}: {e}", settings_dir.display()))?;
  let content = "[Gecko]\n\n[Gecko_Enabled]\n$Optional: Game Music OFF\n$Optional: Widescreen 16:9\n";
  let settings_path = settings_dir.join(format!("{settings_id}.ini"));
  fs::write(&settings_path, content)
    .map_err(|e| format!("write GameSettings {}: {e}", settings_path.display()))?;
  Ok(())
}

fn ini_set(path: &Path, section: &str, key: &str, value: &str) -> Result<(), String> {
  if !path.is_file() {
    let payload = format!("[{section}]\n{key} = {value}\n");
    fs::write(path, payload).map_err(|e| format!("write ini {}: {e}", path.display()))?;
    return Ok(());
  }

  let data = fs::read_to_string(path).map_err(|e| format!("read ini {}: {e}", path.display()))?;
  let mut output: Vec<String> = Vec::new();
  let mut in_section = false;
  let mut seen_section = false;
  let mut done = false;

  for line in data.lines() {
    let trimmed = line.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
      if in_section && !done {
        output.push(format!("{key} = {value}"));
        done = true;
      }
      in_section = trimmed == format!("[{section}]");
      if in_section {
        seen_section = true;
      }
      output.push(line.to_string());
      continue;
    }

    if in_section {
      let key_prefix = format!("{key} ");
      if trimmed.starts_with(&key_prefix) || trimmed.starts_with(&format!("{key}=")) {
        if !done {
          output.push(format!("{key} = {value}"));
          done = true;
        }
        continue;
      }
    }

    output.push(line.to_string());
  }

  if !seen_section {
    output.push(format!("[{section}]"));
  }
  if !done {
    output.push(format!("{key} = {value}"));
  }

  fs::write(path, output.join("\n") + "\n")
    .map_err(|e| format!("write ini {}: {e}", path.display()))?;
  Ok(())
}

fn write_dolphin_config(user_dir: &Path) -> Result<(), String> {
  let config_dir = user_dir.join("Config");
  fs::create_dir_all(&config_dir)
    .map_err(|e| format!("create Dolphin config dir {}: {e}", config_dir.display()))?;
  let path = config_dir.join("Dolphin.ini");
  ini_set(&path, "Display", "Fullscreen", "True")
}

fn stop_dolphin_child(mut child: Child) -> Result<(), String> {
  match child.try_wait() {
    Ok(Some(_)) => return Ok(()),
    Ok(None) => {}
    Err(e) => return Err(format!("check dolphin process: {e}")),
  }
  child.kill().map_err(|e| format!("stop dolphin process: {e}"))?;
  let _ = child.wait();
  Ok(())
}

fn playback_output_dir() -> PathBuf {
  if let Ok(raw) = env::var("PLAYBACK_OUTPUT_DIR") {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
      return resolve_repo_path(trimmed);
    }
  }
  repo_root().join("airlock").join("tmp")
}

fn slippi_last_frame(replay_path: &Path) -> Result<i32, String> {
  let node_path = build_node_path()?;
  let script = r#"
const { SlippiGame } = require('@slippi/slippi-js');
const input = process.argv[1];
if (!input) process.exit(2);
const game = new SlippiGame(input);
const meta = game.getMetadata() || {};
let last = typeof meta.lastFrame === 'number' ? meta.lastFrame : null;
if (last === null) {
  const frames = game.getFrames() || {};
  for (const key of Object.keys(frames)) {
    const num = Number(key);
    if (Number.isFinite(num)) {
      if (last === null || num > last) last = num;
    }
  }
}
if (last === null) process.exit(2);
console.log(last);
"#;
  let output = Command::new("node")
    .env("NODE_PATH", node_path)
    .arg("-e")
    .arg(script)
    .arg(replay_path)
    .output()
    .map_err(|e| format!("run node for replay length: {e}"))?;
  if !output.status.success() {
    return Err(format!(
      "node failed to read replay length: {}",
      String::from_utf8_lossy(&output.stderr)
    ));
  }
  let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
  raw
    .parse::<i32>()
    .map_err(|e| format!("parse replay length from node output '{raw}': {e}"))
}

fn write_playback_config(replay_path: &Path, output_dir: &Path, command_id: &str) -> Result<(PathBuf, String), String> {
  let last_frame = slippi_last_frame(replay_path)?;
  let start_frame = -123i32;
  let mut end_frame = last_frame.saturating_sub(1);
  if end_frame <= start_frame {
    end_frame = start_frame + 1;
  }

  let file_basename = format!("playback_{command_id}");
  let config_path = output_dir.join(format!("{file_basename}.json"));
  let payload = json!({
    "mode": "normal",
    "replay": replay_path.to_string_lossy(),
    "startFrame": start_frame,
    "endFrame": end_frame,
    "isRealTimeMode": false,
    "commandId": command_id,
  });
  let contents = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
  fs::write(&config_path, contents)
    .map_err(|e| format!("write playback config {}: {e}", config_path.display()))?;
  Ok((config_path, file_basename))
}

fn slippi_appimage_path() -> Result<PathBuf, String> {
  let raw = env::var("SLIPPI_APPIMAGE_PATH")
    .unwrap_or_else(|_| "slippi.AppImage".to_string());
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return Err("SLIPPI_APPIMAGE_PATH is empty; set it to your slippi.AppImage path.".into());
  }

  let path = resolve_repo_path(trimmed);
  if path.is_file() {
    Ok(path)
  } else {
    Err(format!(
      "Slippi AppImage not found at {}. Set SLIPPI_APPIMAGE_PATH to the file.",
      path.display()
    ))
  }
}

fn slippi_display_override() -> Option<String> {
  env::var("SLIPPI_DISPLAY").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn target_display() -> Result<String, String> {
  if let Some(d) = slippi_display_override() {
    return Ok(d);
  }
  env::var("DISPLAY").map_err(|_| "DISPLAY is not set; set DISPLAY or SLIPPI_DISPLAY".to_string())
}

fn slippi_x11_connect() -> Result<(RustConnection, usize), String> {
  let display = target_display().ok();
  x11rb::connect(display.as_deref()).map_err(|e| e.to_string())
}

fn launch_dolphin_for_setup_internal(setup_id: u32) -> Result<Child, String> {
  let config = dolphin_config()?;
  let user_dir = setup_user_dir(setup_id)?;
  write_gamesettings(&user_dir)?;
  write_dolphin_config(&user_dir)?;

  let label = format!("dolphin-{setup_id}");
  let use_obs = obs_gamecapture_enabled();
  let obs_gamecapture = if use_obs {
    obs_gamecapture_path().ok_or_else(|| {
      "obs-gamecapture not found. Install obs-vkcapture or set OBS_GAMECAPTURE.".to_string()
    })?
  } else {
    PathBuf::new()
  };

  let mut cmd = if use_obs {
    let mut cmd = Command::new(obs_gamecapture);
    cmd.arg(&config.dolphin_path);
    cmd
  } else {
    Command::new(&config.dolphin_path)
  };

  cmd.arg("--user").arg(&user_dir);
  if dolphin_batch_enabled() {
    cmd.arg("-b");
  }
  cmd.arg(dolphin_exec_flag()).arg(&config.ssbm_iso_path);

  cmd.env("OBS_VKCAPTURE", "1");
  cmd.env("OBS_VKCAPTURE_EXE_NAME", &label);
  if let Some(lib_path) = exe_override_lib_path() {
    apply_ld_preload(&mut cmd, &lib_path);
  }

  if let Some(dir) = config.dolphin_path.parent() {
    cmd.current_dir(dir);
  }

  cmd.spawn()
    .map_err(|e| format!("launch Dolphin for setup {setup_id}: {e}"))
}

fn launch_dolphin_playback_for_setup_internal(setup_id: u32, replay_path: &Path) -> Result<Child, String> {
  let config = dolphin_config()?;
  let user_dir = setup_user_dir(setup_id)?;
  write_gamesettings(&user_dir)?;
  write_dolphin_config(&user_dir)?;

  let output_dir = playback_output_dir();
  fs::create_dir_all(&output_dir)
    .map_err(|e| format!("create playback output dir {}: {e}", output_dir.display()))?;
  let command_id = format!(
    "{}-{}",
    setup_id,
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
  );
  let (playback_config, file_basename) = write_playback_config(replay_path, &output_dir, &command_id)?;

  let label = format!("dolphin-{setup_id}");
  let use_obs = obs_gamecapture_enabled();
  let obs_gamecapture = if use_obs {
    obs_gamecapture_path().ok_or_else(|| {
      "obs-gamecapture not found. Install obs-vkcapture or set OBS_GAMECAPTURE.".to_string()
    })?
  } else {
    PathBuf::new()
  };

  let mut cmd = if use_obs {
    let mut cmd = Command::new(obs_gamecapture);
    cmd.arg(&config.dolphin_path);
    cmd
  } else {
    Command::new(&config.dolphin_path)
  };

  cmd.arg("--user")
    .arg(&user_dir)
    .arg("-i")
    .arg(&playback_config)
    .arg("-o")
    .arg(format!("{file_basename}-unmerged"))
    .arg(format!("--output-directory={}", output_dir.to_string_lossy()));
  if dolphin_batch_enabled() {
    cmd.arg("-b");
  }
  cmd.arg(dolphin_exec_flag()).arg(&config.ssbm_iso_path);

  cmd.env("OBS_VKCAPTURE", "1");
  cmd.env("OBS_VKCAPTURE_EXE_NAME", &label);
  if let Some(lib_path) = exe_override_lib_path() {
    apply_ld_preload(&mut cmd, &lib_path);
  }

  if let Some(dir) = config.dolphin_path.parent() {
    cmd.current_dir(dir);
  }

  cmd.spawn()
    .map_err(|e| format!("launch Dolphin playback for setup {setup_id}: {e}"))
}

#[tauri::command]
fn launch_dolphin_for_setup(setup_id: u32, store: State<'_, Mutex<SetupStore>>) -> Result<(), String> {
  let existing = {
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    if !guard.setups.iter().any(|s| s.id == setup_id) {
      return Err("Setup not found".to_string());
    }
    guard.processes.remove(&setup_id)
  };

  if let Some(child) = existing {
    stop_dolphin_child(child)?;
  }

  let child = launch_dolphin_for_setup_internal(setup_id)?;
  let mut guard = store.lock().map_err(|e| e.to_string())?;
  guard.processes.insert(setup_id, child);
  Ok(())
}

#[tauri::command]
fn assign_stream_to_setup(
  setup_id: u32,
  stream: SlippiStream,
  store: State<'_, Mutex<SetupStore>>,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Setup, String> {
  let test_mode = app_test_mode_enabled();
  let existing = {
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    if !guard.setups.iter().any(|s| s.id == setup_id) {
      return Err("Setup not found".to_string());
    }
    guard.processes.remove(&setup_id)
  };

  if let Some(child) = existing {
    stop_dolphin_child(child)?;
  }

  let child = if test_mode {
    let replay = {
      let guard = test_state.lock().map_err(|e| e.to_string())?;
      guard.spoof_replays.get(&stream.id).cloned()
    }
    .ok_or_else(|| {
      "No test replay found for this stream. Click \"Spoof live games\" first.".to_string()
    })?;
    launch_dolphin_playback_for_setup_internal(setup_id, &replay)?
  } else {
    watch_slippi_stream(stream.id.clone(), stream.p1_code.clone(), stream.p1_tag.clone())?;
    launch_dolphin_for_setup_internal(setup_id)?
  };

  let mut guard = store.lock().map_err(|e| e.to_string())?;
  let setup_clone = {
    let setup = guard
      .setups
      .iter_mut()
      .find(|s| s.id == setup_id)
      .ok_or_else(|| "Setup not found".to_string())?;
    setup.assigned_stream = Some(stream);
    setup.clone()
  };
  guard.processes.insert(setup_id, child);
  Ok(setup_clone)
}

#[tauri::command]
fn clear_setup_assignment(setup_id: u32, store: State<'_, Mutex<SetupStore>>) -> Result<Setup, String> {
  let (setup, existing) = {
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    let setup = guard
      .setups
      .iter_mut()
      .find(|s| s.id == setup_id)
      .ok_or_else(|| "Setup not found".to_string())?;
    setup.assigned_stream = None;
    let cloned = setup.clone();
    let existing = guard.processes.remove(&setup_id);
    (cloned, existing)
  };

  if let Some(child) = existing {
    stop_dolphin_child(child)?;
  }

  Ok(setup)
}

#[tauri::command]
fn find_slippi_launcher_window() -> Result<Option<SlippiWindowInfo>, String> {
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

fn cdp_targets(port: u16) -> Result<Vec<CdpTarget>, String> {
  let url = format!("http://127.0.0.1:{port}/json/list");
  let resp = reqwest::blocking::get(&url).map_err(|e| format!("fetch {url}: {e}"))?;
  if !resp.status().is_success() {
    return Err(format!("DevTools list {url} returned {}", resp.status()));
  }
  resp.json::<Vec<CdpTarget>>().map_err(|e| format!("parse DevTools list: {e}"))
}

fn pick_slippi_target(targets: Vec<CdpTarget>) -> Option<CdpTarget> {
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

fn cdp_eval(ws_url: &str, expr: &str) -> Result<Value, String> {
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

fn scrape_slippi_via_cdp(port: u16) -> Result<Vec<SlippiStream>, String> {
  let targets = cdp_targets(port)?;
  let target = pick_slippi_target(targets).ok_or_else(|| "No DevTools targets found; is Slippi running with --remote-debugging-port?".to_string())?;
  let ws_url = target.ws_url.ok_or_else(|| "Target missing webSocketDebuggerUrl".to_string())?;

  let expr = r#"
    (() => {
      const cards = Array.from(document.querySelectorAll('.css-7xs1xn, [data-testid="spectate-card"], .css-o8b25d .MuiPaper-root'));
      return cards.map((c, idx) => {
        const text = (c.innerText || '').split('\n').map(t => t.trim()).filter(Boolean);
        const name = text[0] || null;
        const code = text.find(t => t.includes('#')) || null;
        return {
          id: c.id || `card-${idx}`,
          name,
          code,
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
      source: Some(format!("cdp port {port}")),
    });
  }
  Ok(out)
}

fn click_slippi_refresh(port: u16) -> Result<(), String> {
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

fn click_slippi_watch(port: u16, target_id: String, target_code: Option<String>, target_tag: Option<String>) -> Result<(), String> {
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
        const byIcon = buttons.find(btn => btn.querySelector('[data-testid=\"PlayCircleOutlineIcon\"]'));
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

/// Scan the Slippi Launcher window, screenshot it, OCR the contents, and try to extract tags/connect codes.
#[tauri::command]
fn scan_slippi_streams(test_state: State<'_, Mutex<TestModeState>>) -> Result<Vec<SlippiStream>, String> {
  if mock_streams_enabled() {
    return test_mode_streams();
  }
  if app_test_mode_enabled() {
    let guard = test_state.lock().map_err(|e| e.to_string())?;
    return Ok(guard.spoof_streams.clone());
  }
  let devtools_port = slippi_devtools_port();
  scrape_slippi_via_cdp(devtools_port)
}

#[tauri::command]
fn refresh_slippi_launcher() -> Result<(), String> {
  if mock_streams_enabled() || app_test_mode_enabled() {
    return Ok(());
  }
  let devtools_port = slippi_devtools_port();
  click_slippi_refresh(devtools_port)
}

#[tauri::command]
fn watch_slippi_stream(stream_id: String, p1_code: Option<String>, p1_tag: Option<String>) -> Result<(), String> {
  if mock_streams_enabled() || app_test_mode_enabled() {
    return Ok(());
  }
  let devtools_port = slippi_devtools_port();
  click_slippi_watch(devtools_port, stream_id, p1_code, p1_tag)
}

#[tauri::command]
fn launch_slippi_app() -> Result<(), String> {
  let appimage = slippi_appimage_path()?;
  let devtools_port = slippi_devtools_port();

  let mut cmd = Command::new(&appimage);
  cmd.arg("--no-sandbox")
    .arg("--disable-setuid-sandbox")
    .arg(format!("--remote-debugging-port={devtools_port}"));

  if let Some(dir) = appimage.parent() {
    cmd.current_dir(dir);
  }

  cmd.spawn().map_err(|e| format!("launch Slippi: {e}"))?;
  Ok(())
}

#[tauri::command]
fn launch_dolphin_cli(extra_args: Option<Vec<String>>) -> Result<(), String> {
  let config = dolphin_config()?;
  let mut cmd = Command::new(&config.dolphin_path);
  cmd.arg("-e")
    .arg(&config.ssbm_iso_path)
    .arg("--cout");
  if let Some(args) = extra_args {
    cmd.args(args);
  }
  if let Some(dir) = config.dolphin_path.parent() {
    cmd.current_dir(dir);
  }
  cmd.spawn().map_err(|e| format!("launch Dolphin: {e}"))?;
  Ok(())
}

#[tauri::command]
fn spoof_live_games(test_state: State<'_, Mutex<TestModeState>>) -> Result<Vec<SlippiStream>, String> {
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
  Ok(streams)
}

#[tauri::command]
fn spoof_bracket_set_replays(
  app_handle: tauri::AppHandle,
  config_path: String,
  set_id: u64,
) -> Result<SpoofReplayResult, String> {
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

  let replay_paths = read_bracket_set_replay_paths(&config_path, set_id)?;
  let mut tasks: Vec<Value> = Vec::new();
  let mut missing = 0usize;
  let mut valid_paths = Vec::new();
  for path in replay_paths {
    if path.is_file() {
      valid_paths.push(path);
    } else {
      missing += 1;
    }
  }
  let replay_total = valid_paths.len();
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

  if tasks.is_empty() {
    return Err(format!("No replay files found for set {set_id}."));
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

  if let Some(stdout) = child.stdout.take() {
    let app = app_handle.clone();
    std::thread::spawn(move || {
      let reader = BufReader::new(stdout);
      for line in reader.lines().flatten() {
        if let Some(payload) = line.strip_prefix("SPOOF_PROGRESS:") {
          if let Ok(value) = serde_json::from_str::<Value>(payload) {
            let _ = app.emit("spoof-replay-progress", value);
          }
        }
      }
    });
  }

  if let Some(stderr) = child.stderr.take() {
    let app = app_handle.clone();
    std::thread::spawn(move || {
      let reader = BufReader::new(stderr);
      for line in reader.lines().flatten() {
        let payload = json!({
          "type": "error",
          "message": line,
        });
        let _ = app.emit("spoof-replay-progress", payload);
      }
    });
  }

  Ok(SpoofReplayResult {
    started: tasks.len(),
    missing,
  })
}

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

fn read_bracket_set_replay_paths(config_path: &str, set_id: u64) -> Result<Vec<PathBuf>, String> {
  let resolved = resolve_startgg_sim_config_path(config_path);
  if !resolved.is_file() {
    return Err(format!("Bracket config not found at {}", resolved.display()));
  }
  let data = fs::read_to_string(&resolved)
    .map_err(|e| format!("read bracket config {}: {e}", resolved.display()))?;
  let value: Value = serde_json::from_str(&data)
    .map_err(|e| format!("parse bracket config {}: {e}", resolved.display()))?;

  let replay_map = value
    .get("referenceReplayMap")
    .ok_or_else(|| "referenceReplayMap missing from bracket config.".to_string())?;
  let base_dir = replay_map
    .get("replaysDir")
    .and_then(|v| v.as_str())
    .map(resolve_repo_path);
  let sets = replay_map
    .get("sets")
    .and_then(|sets| sets.as_array())
    .ok_or_else(|| "referenceReplayMap sets missing from bracket config.".to_string())?;

  let mut out: Vec<PathBuf> = Vec::new();
  let mut seen: HashSet<PathBuf> = HashSet::new();

  for set in sets {
    let id = set.get("id").and_then(|v| v.as_u64());
    if id != Some(set_id) {
      continue;
    }
    let replays = match set.get("replays").and_then(|v| v.as_array()) {
      Some(replays) => replays,
      None => break,
    };
    for replay in replays {
      let raw = replay.get("path").and_then(|v| v.as_str()).unwrap_or("").trim();
      if raw.is_empty() {
        continue;
      }
      let mut path = PathBuf::from(raw);
      if !path.is_absolute() {
        if let Some(base) = &base_dir {
          path = base.join(&path);
        } else {
          path = resolve_repo_path(raw);
        }
      }
      if seen.insert(path.clone()) {
        out.push(path);
      }
    }
    break;
  }

  if out.is_empty() {
    return Err(format!("No replay paths found for set {set_id}."));
  }
  Ok(out)
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

fn normalize_slippi_code(raw: &str) -> Option<String> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return None;
  }
  Some(trimmed.to_ascii_uppercase())
}

fn replay_pair_key(a: &str, b: &str) -> String {
  if a <= b {
    format!("{a}|{b}")
  } else {
    format!("{b}|{a}")
  }
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
      for replay in replays {
        let path = replay.get("path").and_then(|v| v.as_str()).unwrap_or("").trim();
        if path.is_empty() {
          continue;
        }
        let mut unique: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        if let Some(slots) = replay.get("slots").and_then(|v| v.as_array()) {
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
        if unique.len() < 2 {
          continue;
        }
        let key = replay_pair_key(&unique[0], &unique[1]);
        pairs.insert(key);
      }
    }
  }
  let mut out: Vec<String> = pairs.into_iter().collect();
  out.sort();
  Ok(out)
}

#[tauri::command]
fn startgg_sim_state(
  since_ms: Option<u64>,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<StartggSimState, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  Ok(sim.state_since(now, since_ms))
}

#[tauri::command]
fn startgg_sim_reset(
  config_path: Option<String>,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<StartggSimState, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
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
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  Ok(sim.state(now))
}

#[tauri::command]
fn startgg_sim_advance_set(set_id: u64, test_state: State<'_, Mutex<TestModeState>>) -> Result<StartggSimState, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.advance_set(set_id, now)?;
  Ok(sim.state(now))
}

#[tauri::command]
fn startgg_sim_force_winner(
  set_id: u64,
  winner_slot: u8,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<StartggSimState, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.force_winner(set_id, winner_slot as usize, now)?;
  Ok(sim.state(now))
}

#[tauri::command]
fn startgg_sim_mark_dq(
  set_id: u64,
  dq_slot: u8,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<StartggSimState, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.mark_dq(set_id, dq_slot as usize, now)?;
  Ok(sim.state(now))
}

#[tauri::command]
fn startgg_sim_raw_state(
  since_ms: Option<u64>,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  Ok(sim.raw_response(now, since_ms))
}

#[tauri::command]
fn startgg_sim_raw_reset(
  config_path: Option<String>,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
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
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  Ok(sim.raw_response(now, None))
}

#[tauri::command]
fn startgg_sim_raw_advance_set(
  set_id: u64,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.advance_set(set_id, now)?;
  Ok(sim.raw_response(now, None))
}

#[tauri::command]
fn startgg_sim_raw_start_set(
  set_id: u64,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.start_set_manual(set_id, now)?;
  Ok(sim.raw_response(now, None))
}

#[tauri::command]
fn startgg_sim_raw_finish_set(
  set_id: u64,
  winner_slot: u8,
  scores: Vec<u8>,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  if scores.len() != 2 {
    return Err("Scores must include exactly two values.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.finish_set_manual(set_id, winner_slot as usize, [scores[0], scores[1]], now)?;
  Ok(sim.raw_response(now, None))
}

#[tauri::command]
fn startgg_sim_raw_complete_bracket(
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard
    .startgg_sim
    .as_mut()
    .ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  if sim.has_reference_sets() {
    sim.complete_from_reference(now)?;
  } else {
    sim.complete_all_sets(now)?;
  }
  Ok(sim.raw_response(now, None))
}

#[tauri::command]
fn startgg_sim_raw_force_winner(
  set_id: u64,
  winner_slot: u8,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.force_winner(set_id, winner_slot as usize, now)?;
  Ok(sim.raw_response(now, None))
}

#[tauri::command]
fn startgg_sim_raw_mark_dq(
  set_id: u64,
  dq_slot: u8,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard.startgg_sim.as_mut().ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.mark_dq(set_id, dq_slot as usize, now)?;
  Ok(sim.raw_response(now, None))
}

#[tauri::command]
fn startgg_sim_raw_reset_set(
  set_id: u64,
  test_state: State<'_, Mutex<TestModeState>>,
) -> Result<Value, String> {
  if !app_test_mode_enabled() {
    return Err("Test mode is disabled in settings.".to_string());
  }
  let now = now_ms();
  let mut guard = test_state.lock().map_err(|e| e.to_string())?;
  init_startgg_sim(&mut guard, now)?;
  let sim = guard
    .startgg_sim
    .as_mut()
    .ok_or_else(|| "Start.gg sim failed to initialize.".to_string())?;
  sim.reset_set_and_dependents(set_id, now)?;
  Ok(sim.raw_response(now, None))
}

#[tauri::command]
fn load_config() -> Result<AppConfig, String> {
  load_config_inner()
}

#[tauri::command]
fn save_config(config: AppConfig) -> Result<AppConfig, String> {
  save_config_inner(config)
}

/// Shared entry point for both the binary (main.rs) and the library target Tauri expects.
pub fn run() {
  load_env_file();
  let setup_store = Mutex::new(SetupStore::bootstrap_from_existing());
  let test_state = Mutex::new(TestModeState::default());
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .manage(setup_store)
    .manage(test_state)
    .invoke_handler(tauri::generate_handler![
      list_setups_stub,
      list_setups,
      create_setup,
      delete_setup,
      find_slippi_launcher_window,
      scan_slippi_streams,
      refresh_slippi_launcher,
      watch_slippi_stream,
      launch_dolphin_for_setup,
      assign_stream_to_setup,
      clear_setup_assignment,
      launch_slippi_app,
      launch_dolphin_cli,
      spoof_live_games,
      spoof_bracket_set_replays,
      list_bracket_configs,
      list_bracket_replay_sets,
      list_bracket_replay_pairs,
      startgg_sim_state,
      startgg_sim_reset,
      startgg_sim_advance_set,
      startgg_sim_force_winner,
      startgg_sim_mark_dq,
      startgg_sim_raw_state,
      startgg_sim_raw_reset,
      startgg_sim_raw_advance_set,
      startgg_sim_raw_start_set,
      startgg_sim_raw_finish_set,
      startgg_sim_raw_complete_bracket,
      startgg_sim_raw_force_winner,
      startgg_sim_raw_mark_dq,
      startgg_sim_raw_reset_set,
      load_config,
      save_config
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri app");
}
