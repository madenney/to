use crate::types::*;
use chrono::Local;
use serde_json::Value;
use std::{
    collections::HashSet,
    env,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn repo_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .map(|path| path.to_path_buf())
    .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

pub fn resolve_repo_path(raw: &str) -> PathBuf {
  let path = PathBuf::from(raw);
  if path.is_absolute() {
    path
  } else {
    repo_root().join(path)
  }
}

pub fn config_path() -> PathBuf {
  repo_root().join("config.json")
}

pub fn env_default(key: &str) -> Option<String> {
  env::var(key)
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

pub fn env_flag_true(key: &str) -> bool {
  match env::var(key) {
    Ok(value) => {
      let value = value.trim().to_ascii_lowercase();
      matches!(value.as_str(), "1" | "true" | "yes" | "on")
    }
    Err(_) => false,
  }
}

pub fn env_flag_true_default(key: &str, default: bool) -> bool {
  match env::var(key) {
    Ok(value) => {
      let value = value.trim().to_ascii_lowercase();
      matches!(value.as_str(), "1" | "true" | "yes" | "on")
    }
    Err(_) => default,
  }
}

pub fn apply_env_defaults(mut config: AppConfig) -> AppConfig {
  if config.dolphin_path.trim().is_empty() {
    if let Some(value) = env_default("DOLPHIN_PATH") {
      config.dolphin_path = value;
    }
  }
  if config.ssbm_iso_path.trim().is_empty() {
    if let Some(value) = env_default("SSBM_ISO_PATH") {
      config.ssbm_iso_path = value;
    }
  }
  if config.slippi_launcher_path.trim().is_empty() {
    if let Some(value) = env_default("SLIPPI_APPIMAGE_PATH") {
      config.slippi_launcher_path = value;
    }
  }
  if config.spectate_folder_path.trim().is_empty() {
    if let Some(value) = env_default("SPECTATE_FOLDER_PATH") {
      config.spectate_folder_path = value;
    }
  }
  if config.startgg_link.trim().is_empty() {
    if let Some(value) = env_default("STARTGG_EVENT_LINK") {
      config.startgg_link = value;
    }
  }
  if config.startgg_token.trim().is_empty() {
    if let Some(value) = env_default("STARTGG_TOKEN") {
      config.startgg_token = value;
    }
  }
  config
}

pub fn load_config_inner() -> Result<AppConfig, String> {
  let path = config_path();
  if !path.is_file() {
    return Ok(apply_env_defaults(AppConfig::default()));
  }
  let data = fs::read_to_string(&path).map_err(|e| format!("read config {}: {e}", path.display()))?;
  let config =
    serde_json::from_str::<AppConfig>(&data).map_err(|e| format!("parse config {}: {e}", path.display()))?;
  Ok(apply_env_defaults(config))
}

pub fn save_config_inner(config: AppConfig) -> Result<AppConfig, String> {
  let path = config_path();
  let payload = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
  fs::write(&path, payload).map_err(|e| format!("write config {}: {e}", path.display()))?;
  Ok(config)
}

pub fn load_env_file() {
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

pub fn parse_env_line(line: &str) -> Option<(String, String)> {
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

pub fn required_env_var(key: &str) -> Result<String, String> {
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

pub fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_millis() as u64
}

pub fn startgg_log_path() -> PathBuf {
  repo_root().join("logs").join("startgg_api.log")
}

pub fn append_startgg_log(label: &str, payload: &str) {
  let dir = repo_root().join("logs");
  if fs::create_dir_all(&dir).is_err() {
    return;
  }
  let path = startgg_log_path();
  let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
  let entry = format!("[{timestamp}] {label}\n{payload}\n\n");
  if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
    let _ = file.write_all(entry.as_bytes());
  }
}

pub fn startgg_sim_config_path() -> PathBuf {
  if let Ok(raw) = env::var("STARTGG_SIM_CONFIG_PATH") {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
      return PathBuf::from(trimmed);
    }
  }
  startgg_sim_configs_dir().join("test_bracket_2.json")
}

pub fn startgg_sim_configs_dir() -> PathBuf {
  repo_root().join("test_brackets")
}

pub fn resolve_startgg_sim_config_path(raw: &str) -> PathBuf {
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

pub fn resolve_startgg_sim_path_from_config(config: &AppConfig) -> Option<PathBuf> {
  let trimmed = config.test_bracket_path.trim();
  if trimmed.is_empty() {
    return None;
  }
  Some(resolve_startgg_sim_config_path(trimmed))
}

pub fn sync_startgg_sim_path_from_config(guard: &mut TestModeState, config: &AppConfig) {
  if let Some(path) = resolve_startgg_sim_path_from_config(config) {
    guard.startgg_config_path = Some(path);
  }
}

pub fn sync_live_startgg_from_config(guard: &mut LiveStartggState, config: &AppConfig) {
  let link = config.startgg_link.trim();
  if link.is_empty() {
    guard.state = None;
    guard.event_slug = None;
    guard.startgg_link = None;
    guard.last_fetch = None;
    guard.last_error = None;
    guard.fetch_in_flight = false;
    return;
  }
  if guard.startgg_link.as_deref() != Some(link) {
    guard.state = None;
    guard.event_slug = None;
    guard.last_fetch = None;
    guard.last_error = None;
  }
  if !config.startgg_token.trim().is_empty() {
    guard.last_error = None;
  }
  guard.startgg_link = Some(link.to_string());
}

pub fn node_path_delimiter() -> char {
  if cfg!(windows) { ';' } else { ':' }
}

pub fn split_node_path(raw: &str) -> Vec<PathBuf> {
  raw
    .split(node_path_delimiter())
    .map(|part| part.trim())
    .filter(|part| !part.is_empty())
    .map(PathBuf::from)
    .collect()
}

pub fn contains_slippi_module(path: &Path) -> bool {
  path.join("@slippi").join("slippi-js").is_dir()
}

pub fn candidate_node_modules() -> Vec<PathBuf> {
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

pub fn build_node_path() -> Result<String, String> {
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

pub fn test_config_path() -> PathBuf {
  if let Ok(raw) = env::var("SLIPPI_TEST_CONFIG_PATH") {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
      return PathBuf::from(trimmed);
    }
  }
  repo_root().join("test_config.json")
}

pub fn default_test_folders() -> Vec<String> {
  vec![
    "test_files/replays/aklo".to_string(),
    "test_files/replays/axe".to_string(),
    "test_files/replays/nomad".to_string(),
    "test_files/replays/rookie".to_string(),
    "test_files/replays/shiz".to_string(),
  ]
}

pub fn load_test_folder_paths() -> Result<Vec<PathBuf>, String> {
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

pub fn normalize_slippi_code(raw: &str) -> Option<String> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return None;
  }
  Some(trimmed.to_ascii_uppercase())
}

pub fn replay_pair_key(a: &str, b: &str) -> String {
  if a <= b {
    format!("{a}|{b}")
  } else {
    format!("{b}|{a}")
  }
}

pub fn app_test_mode_enabled() -> bool {
  match load_config_inner() {
    Ok(config) => config.test_mode,
    Err(_) => false,
  }
}

pub fn log_env_warnings() {
  let config = load_config_inner().unwrap_or_else(|_| AppConfig::default());
  let mut warnings = Vec::new();

  if config.dolphin_path.trim().is_empty() && env_default("DOLPHIN_PATH").is_none() {
    warnings.push("DOLPHIN_PATH not set and no dolphin path in config — Dolphin launch will fail");
  }
  if config.ssbm_iso_path.trim().is_empty() && env_default("SSBM_ISO_PATH").is_none() {
    warnings.push("SSBM_ISO_PATH not set and no ISO path in config — Dolphin launch will fail");
  }
  if config.slippi_launcher_path.trim().is_empty() && env_default("SLIPPI_APPIMAGE_PATH").is_none() {
    warnings.push("SLIPPI_APPIMAGE_PATH not set and no Slippi path in config — Slippi launch may fail");
  }

  for msg in warnings {
    tracing::warn!("{}", msg);
  }
}

pub fn normalize_broadcast_key(raw: &str) -> String {
  raw.trim().to_lowercase()
}

pub fn normalize_tag_key(raw: &str) -> String {
  let trimmed = strip_sponsor_tag(raw).trim();
  if trimmed.is_empty() {
    return String::new();
  }
  let without_code = trimmed.split('#').next().unwrap_or(trimmed);
  without_code.trim().to_lowercase()
}

pub fn strip_sponsor_tag(raw: &str) -> &str {
  let trimmed = raw.trim();
  if let Some(idx) = trimmed.find('|') {
    trimmed[idx + 1..].trim()
  } else {
    trimmed
  }
}
