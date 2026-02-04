use crate::config::*;
use crate::types::*;
use std::{
    collections::HashSet,
    env,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command},
    thread::sleep,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::State;

pub fn dolphin_config() -> Result<DolphinConfig, String> {
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

pub fn dolphin_exec_flag() -> String {
    env::var("DOLPHIN_EXEC_FLAG")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "-e".to_string())
}

pub fn dolphin_batch_enabled() -> bool {
    env_flag_true_default("DOLPHIN_BATCH", true)
}

pub fn obs_gamecapture_enabled() -> bool {
    env_flag_true_default("USE_OBS_GAMECAPTURE", true)
}

pub fn slippi_launches_dolphin() -> bool {
    env_flag_true_default("SLIPPI_LAUNCHES_DOLPHIN", true)
}

pub fn read_proc_cmdline(pid: u32) -> Result<Vec<String>, String> {
    let path = PathBuf::from("/proc").join(pid.to_string()).join("cmdline");
    let bytes = fs::read(&path).map_err(|e| format!("read cmdline {}: {e}", path.display()))?;
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in bytes.split(|b| *b == 0) {
        if part.is_empty() {
            continue;
        }
        out.push(String::from_utf8_lossy(part).to_string());
    }
    Ok(out)
}

pub fn cmdline_contains_dolphin(cmdline: &[String]) -> bool {
    cmdline
        .iter()
        .any(|arg| arg.to_lowercase().contains("dolphin"))
}

pub fn cmdline_matches_slippi(cmdline: &[String], slippi_path: &Path) -> bool {
    let exe = match cmdline.first() {
        Some(exe) => exe,
        None => return false,
    };
    let slippi_name = slippi_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if !slippi_name.is_empty() && exe.ends_with(slippi_name) {
        return true;
    }
    let full = slippi_path.to_string_lossy();
    exe == full.as_ref() || cmdline.iter().any(|arg| arg.contains(full.as_ref()))
}

pub fn list_dolphin_like_pids() -> HashSet<u32> {
    let mut out = HashSet::new();
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Ok(pid) = name.to_string_lossy().parse::<u32>() else {
            continue;
        };
        if let Ok(cmdline) = read_proc_cmdline(pid) {
            if cmdline_contains_dolphin(&cmdline) {
                out.insert(pid);
            }
        }
    }
    out
}

pub fn list_slippi_pids(slippi_path: &Path) -> HashSet<u32> {
    let mut out = HashSet::new();
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Ok(pid) = name.to_string_lossy().parse::<u32>() else {
            continue;
        };
        if let Ok(cmdline) = read_proc_cmdline(pid) {
            if cmdline_matches_slippi(&cmdline, slippi_path) {
                out.insert(pid);
            }
        }
    }
    out
}

pub fn find_new_dolphin_cmdline_any(
    before: &HashSet<u32>,
    timeout: Duration,
) -> Result<Option<(u32, Vec<String>)>, String> {
    let start = Instant::now();
    loop {
        let current = list_dolphin_like_pids();
        let mut new: Vec<u32> = current.difference(before).copied().collect();
        if !new.is_empty() {
            new.sort_unstable();
            let pid = *new.last().unwrap();
            let cmdline = read_proc_cmdline(pid)?;
            if !cmdline.is_empty() {
                return Ok(Some((pid, cmdline)));
            }
        }
        if start.elapsed() >= timeout {
            return Ok(None);
        }
        sleep(Duration::from_millis(200));
    }
}

pub fn stop_process_by_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .map_err(|e| format!("stop process {pid}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("stop process {pid}: kill exited with {status}"))
    }
}

pub fn stop_dolphin_child(mut child: Child) -> Result<(), String> {
    match child.try_wait() {
        Ok(Some(_)) => return Ok(()),
        Ok(None) => {}
        Err(e) => return Err(format!("check dolphin process: {e}")),
    }
    child.kill().map_err(|e| format!("stop dolphin process: {e}"))?;
    let _ = child.wait();
    Ok(())
}

pub fn stop_child_process(mut child: Child) -> Result<(), String> {
    match child.try_wait() {
        Ok(Some(_)) => return Ok(()),
        Ok(None) => {}
        Err(e) => return Err(format!("check process: {e}")),
    }
    child.kill().map_err(|e| format!("stop process: {e}"))?;
    let _ = child.wait();
    Ok(())
}

pub fn find_in_path(command: &str) -> Option<PathBuf> {
    let path = env::var("PATH").ok()?;
    for entry in path.split(node_path_delimiter()) {
        let candidate = PathBuf::from(entry).join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn obs_gamecapture_path() -> Option<PathBuf> {
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

pub fn exe_override_lib_path() -> Option<PathBuf> {
    let path = repo_root().join("scripts").join("vkcapture_exe_override.so");
    if path.is_file() { Some(path) } else { None }
}

pub fn apply_ld_preload(cmd: &mut Command, lib_path: &Path) {
    let lib = lib_path.to_string_lossy().to_string();
    let merged = match env::var("LD_PRELOAD") {
        Ok(existing) if !existing.trim().is_empty() => format!("{lib}:{existing}"),
        _ => lib,
    };
    cmd.env("LD_PRELOAD", merged);
}

pub fn dolphin_binary_path() -> Result<PathBuf, String> {
    let config = load_config_inner()?;
    let raw = config.dolphin_path.trim();
    if raw.is_empty() {
        return Err("Dolphin path is empty; set it in Settings or DOLPHIN_PATH.".to_string());
    }
    let path = resolve_repo_path(raw);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("Dolphin binary not found at {}", path.display()))
    }
}

pub fn detect_slippi_netplay_path() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    let netplay_dir = PathBuf::from(home).join(".config").join("Slippi Launcher").join("netplay");
    if !netplay_dir.is_dir() {
        return None;
    }
    let mut best: Option<(PathBuf, i32)> = None;
    let entries = fs::read_dir(&netplay_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if !name.ends_with(".appimage") {
            continue;
        }
        let score = if name.contains("online") {
            0
        } else if name.contains("slippi") {
            1
        } else {
            2
        };
        match &best {
            Some((_, best_score)) if *best_score <= score => {}
            _ => best = Some((path, score)),
        }
    }
    best.map(|(path, _)| path)
}

pub fn slippi_launcher_dir() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config").join("Slippi Launcher"))
}

pub fn detect_slippi_playback_path() -> Option<PathBuf> {
    let launcher_dir = slippi_launcher_dir()?;
    let playback_dir = launcher_dir.join("playback");
    if !playback_dir.is_dir() {
        return None;
    }
    let mut best: Option<(PathBuf, i32)> = None;
    let entries = fs::read_dir(&playback_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if !name.ends_with(".appimage") {
            continue;
        }
        let score = if name.contains("playback") {
            0
        } else if name.contains("slippi") {
            1
        } else {
            2
        };
        match &best {
            Some((_, best_score)) if *best_score <= score => {}
            _ => best = Some((path, score)),
        }
    }
    best.map(|(path, _)| path)
}

pub fn slippi_playback_appimage_path() -> Option<PathBuf> {
    let launcher_dir = slippi_launcher_dir()?;
    let default = launcher_dir
        .join("playback")
        .join("Slippi_Playback-x86_64.AppImage");
    if default.exists() {
        return Some(default);
    }
    detect_slippi_playback_path()
}

pub fn slippi_appimage_backup_path(target: &Path) -> PathBuf {
    let file_name = target
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let candidate = target.with_file_name(format!("{file_name}.real"));
    if !candidate.exists() {
        return candidate;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    target.with_file_name(format!("{file_name}.real.{ts}"))
}

pub fn ensure_slippi_wrapper_link(target_path: &Path, wrapper_path: &Path) -> Result<bool, String> {
    let target_parent = target_path
        .parent()
        .ok_or_else(|| format!("Invalid AppImage path {}", target_path.display()))?;
    let wrapper_real = fs::canonicalize(wrapper_path).unwrap_or_else(|_| wrapper_path.to_path_buf());

    if let Ok(meta) = fs::symlink_metadata(target_path) {
        if meta.file_type().is_symlink() {
            let link = fs::read_link(target_path)
                .map_err(|e| format!("read link {}: {e}", target_path.display()))?;
            let link_path = if link.is_absolute() {
                link
            } else {
                target_parent.join(link)
            };
            let link_real = fs::canonicalize(&link_path).unwrap_or(link_path);
            if link_real == wrapper_real {
                return Ok(false);
            }
            fs::remove_file(target_path)
                .map_err(|e| format!("remove old link {}: {e}", target_path.display()))?;
        } else {
            let backup = slippi_appimage_backup_path(target_path);
            fs::rename(target_path, &backup)
                .map_err(|e| format!("backup {} to {}: {e}", target_path.display(), backup.display()))?;
        }
    }

    symlink(wrapper_path, target_path)
        .map_err(|e| format!("link {} -> {}: {e}", target_path.display(), wrapper_path.display()))?;
    Ok(true)
}

pub fn ensure_slippi_playback_wrapper(wrapper_path: &Path) -> Result<(), String> {
    let Some(target_path) = slippi_playback_appimage_path() else {
        return Err("Slippi playback Dolphin not found; open Slippi once to install it.".to_string());
    };
    ensure_slippi_wrapper_link(&target_path, wrapper_path).map(|_| ())
}

pub fn slippi_netplay_dolphin_path() -> Result<PathBuf, String> {
    if let Some(value) = env_default("SLIPPI_DOLPHIN_PATH") {
        let path = resolve_repo_path(&value);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "SLIPPI_DOLPHIN_PATH not found at {}",
            path.display()
        ));
    }
    if let Some(path) = detect_slippi_netplay_path() {
        return Ok(path);
    }
    dolphin_binary_path()
}

pub fn ensure_slippi_wrapper() -> Result<PathBuf, String> {
    let dolphin_path = slippi_netplay_dolphin_path()?;
    let label_path = slippi_watch_label_path();
    let wrapper_path = slippi_wrapper_path();
    let exe_override = exe_override_lib_path();
    let obs_default = obs_gamecapture_path()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "obs-gamecapture".to_string());

    let dolphin_escaped = sh_escape(&dolphin_path.to_string_lossy());
    let label_escaped = sh_escape(&label_path.to_string_lossy());
    let override_escaped = exe_override
        .as_ref()
        .map(|path| sh_escape(&path.to_string_lossy()))
        .unwrap_or_default();
    let log_escaped = sh_escape(&slippi_wrapper_log_path().to_string_lossy());
    let obs_default_escaped = sh_escape(&obs_default);

    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

DEFAULT_DOLPHIN_PATH="{dolphin}"
REAL_DOLPHIN_PATH="${{SLIPPI_DOLPHIN_PATH:-}}"
LABEL_FILE="{label}"
EXE_OVERRIDE_LIB="{override_lib}"
USE_OBS_GAMECAPTURE="${{USE_OBS_GAMECAPTURE:-1}}"
OBS_GAMECAPTURE_BIN="${{OBS_GAMECAPTURE:-{obs_gamecapture}}}"
LOG_FILE="{log}"

log() {{
  if [[ -n "$LOG_FILE" ]]; then
    printf '%s %s\n' "$(date +'%Y-%m-%d %H:%M:%S')" "$*" >> "$LOG_FILE"
  fi
}}

log "wrapper start pid=$$ args=$*"

wrapper_self="$0"
resolve_realpath() {{
  local path="$1"
  local resolved=""
  if command -v readlink >/dev/null 2>&1; then
    resolved="$(readlink -f "$path" 2>/dev/null || true)"
  fi
  if [[ -z "$resolved" ]] && command -v realpath >/dev/null 2>&1; then
    resolved="$(realpath "$path" 2>/dev/null || true)"
  fi
  if [[ -z "$resolved" ]]; then
    resolved="$path"
  fi
  printf '%s' "$resolved"
}}
pick_real() {{
  local dir="$1"
  local candidate
  for candidate in "$dir/Slippi_Playback-x86_64.AppImage.real" "$dir/Slippi_Online-x86_64.AppImage.real" "$dir/Slippi_Playback-x86_64.AppImage.bak" "$dir/Slippi_Online-x86_64.AppImage.bak" "$dir"/*.AppImage.real; do
    if [[ -x "$candidate" ]]; then
      printf '%s' "$candidate"
      return 0
    fi
  done
  return 0
}}

link_dir="$(cd "$(dirname "$wrapper_self")" && pwd)"
if [[ -z "$REAL_DOLPHIN_PATH" ]]; then
  REAL_DOLPHIN_PATH="$(pick_real "$link_dir")"
fi

wrapper_self_real="$(resolve_realpath "$wrapper_self")"
dolphin_real="$(resolve_realpath "$REAL_DOLPHIN_PATH")"
if [[ -z "$REAL_DOLPHIN_PATH" || "$dolphin_real" == "$wrapper_self_real" || ! -x "$REAL_DOLPHIN_PATH" ]]; then
  REAL_DOLPHIN_PATH="$(pick_real "$link_dir")"
fi
if [[ -z "$REAL_DOLPHIN_PATH" ]]; then
  REAL_DOLPHIN_PATH="$DEFAULT_DOLPHIN_PATH"
fi
if [[ ! -x "$REAL_DOLPHIN_PATH" ]]; then
  log "Dolphin binary not found: $REAL_DOLPHIN_PATH"
  echo "Dolphin binary not found: $REAL_DOLPHIN_PATH" >&2
  exit 1
fi
log "resolved dolphin=$REAL_DOLPHIN_PATH wrapper=$wrapper_self link_dir=$link_dir default=$DEFAULT_DOLPHIN_PATH"

label=""
skip_label=0
for arg in "$@"; do
  case "$arg" in
    --version|--appimage-version|--help|-h)
      skip_label=1
      break
      ;;
  esac
done

if [[ "$skip_label" -eq 0 ]]; then
  if [[ -n "${{NMST_VKCAPTURE_LABEL:-}}" ]]; then
    label="${{NMST_VKCAPTURE_LABEL}}"
  elif [[ -f "$LABEL_FILE" ]]; then
    label="$(head -n 1 "$LABEL_FILE" | tr -d '\r\n')"
    rm -f "$LABEL_FILE" || true
    log "label file consumed label=$label"
  else
    log "label file missing"
  fi
else
  log "skipping label for args=$*"
fi

export OBS_VKCAPTURE=1
if [[ -n "$label" ]]; then
  export OBS_VKCAPTURE_EXE_NAME="$label"
  if [[ -n "$EXE_OVERRIDE_LIB" && -f "$EXE_OVERRIDE_LIB" ]]; then
    if [[ -n "${{LD_PRELOAD:-}}" ]]; then
      export LD_PRELOAD="$EXE_OVERRIDE_LIB:$LD_PRELOAD"
    else
      export LD_PRELOAD="$EXE_OVERRIDE_LIB"
    fi
  fi
fi
log "obs_gamecapture=$USE_OBS_GAMECAPTURE exe_name=${{OBS_VKCAPTURE_EXE_NAME:-}}"

user_dir=""
args=( "$@" )
for ((i=0; i<${{#args[@]}}; i++)); do
  arg="${{args[i]}}"
  if [[ "$arg" == "--user" ]]; then
    if (( i + 1 < ${{#args[@]}} )); then
      user_dir="${{args[i+1]}}"
    fi
    break
  fi
  if [[ "$arg" == --user=* ]]; then
    user_dir="${{arg#--user=}}"
    break
  fi
done

if [[ -z "$user_dir" && -n "${{HOME:-}}" ]]; then
  lower_path="$(printf '%s' "$REAL_DOLPHIN_PATH" | tr '[:upper:]' '[:lower:]')"
  if [[ "$lower_path" == *"playback"* ]]; then
    user_dir="${{HOME}}/.config/SlippiPlayback"
  else
    user_dir="${{HOME}}/.config/SlippiOnline"
  fi
fi
log "user_dir=${{user_dir:-}}"

ini_set() {{
  local file="$1"
  local section="$2"
  local key="$3"
  local value="$4"

  if [[ ! -f "$file" ]]; then
    printf '[%s]\n%s = %s\n' "$section" "$key" "$value" >"$file"
    return 0
  fi

  local tmp="${{file}}.tmp.$$"
  awk -v section="$section" -v key="$key" -v value="$value" '
    BEGIN {{ in_section=0; seen_section=0; done=0 }}
    /^[[:space:]]*\[/ {{
      if (in_section && !done) {{ print key " = " value; done=1 }}
      if ($0 == "[" section "]") {{ in_section=1; seen_section=1 }} else {{ in_section=0 }}
      print $0
      next
    }}
    {{
      if (in_section && $0 ~ "^[[:space:]]*" key "[[:space:]]*=") {{
        if (!done) {{ print key " = " value; done=1 }}
        next
      }}
      print $0
    }}
    END {{
      if (!seen_section) {{ print "[" section "]" }}
      if (!done) {{ print key " = " value }}
    }}
  ' "$file" >"$tmp"
  mv "$tmp" "$file"
}}

if [[ -n "$user_dir" ]]; then
  cfg_dir="$user_dir/Config"
  mkdir -p "$cfg_dir"
  ini_set "$cfg_dir/Dolphin.ini" "Display" "Fullscreen" "True"
  log "fullscreen set in $cfg_dir/Dolphin.ini"
fi

if [[ "$USE_OBS_GAMECAPTURE" == "1" ]]; then
  if ! command -v "$OBS_GAMECAPTURE_BIN" >/dev/null 2>&1; then
    log "obs-gamecapture not found at $OBS_GAMECAPTURE_BIN"
    echo "obs-gamecapture not found. Install obs-vkcapture or set OBS_GAMECAPTURE." >&2
    exit 1
  fi
  log "exec obs-gamecapture $OBS_GAMECAPTURE_BIN $REAL_DOLPHIN_PATH"
  exec "$OBS_GAMECAPTURE_BIN" "$REAL_DOLPHIN_PATH" "$@"
else
  log "exec dolphin direct $REAL_DOLPHIN_PATH"
  exec "$REAL_DOLPHIN_PATH" "$@"
fi
"#,
        dolphin = dolphin_escaped,
        label = label_escaped,
        override_lib = override_escaped,
        log = log_escaped,
        obs_gamecapture = obs_default_escaped
    );

    if let Some(parent) = wrapper_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create wrapper dir {}: {e}", parent.display()))?;
    }

    let mut should_write = true;
    if wrapper_path.is_file() {
        if let Ok(existing) = fs::read_to_string(&wrapper_path) {
            if existing == script {
                should_write = false;
            }
        }
    }

    if should_write {
        fs::write(&wrapper_path, script)
            .map_err(|e| format!("write Slippi wrapper {}: {e}", wrapper_path.display()))?;
    }

    let perms = fs::metadata(&wrapper_path)
        .map_err(|e| format!("read wrapper permissions {}: {e}", wrapper_path.display()))?
        .permissions();
    let mut next = perms;
    next.set_mode(0o755);
    fs::set_permissions(&wrapper_path, next)
        .map_err(|e| format!("chmod wrapper {}: {e}", wrapper_path.display()))?;

    Ok(wrapper_path)
}

pub fn slippi_watch_label_path() -> PathBuf {
    repo_root().join("airlock").join("slippi_watch_label.txt")
}

pub fn slippi_wrapper_path() -> PathBuf {
    repo_root().join("airlock").join("slippi_dolphin_wrapper.sh")
}

pub fn slippi_wrapper_log_path() -> PathBuf {
    repo_root().join("airlock").join("slippi_wrapper.log")
}

pub fn sh_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

pub fn write_slippi_watch_label(setup_id: u32) -> Result<PathBuf, String> {
    let path = slippi_watch_label_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create label dir {}: {e}", parent.display()))?;
    }
    fs::write(&path, format!("dolphin-{setup_id}\n"))
        .map_err(|e| format!("write Slippi label {}: {e}", path.display()))?;
    Ok(path)
}

pub fn clear_slippi_watch_label(path: &Path) {
    let _ = fs::remove_file(path);
}

pub fn setup_user_dir(setup_id: u32) -> Result<PathBuf, String> {
    let dir = env::temp_dir().join(format!("slippi-setup-{setup_id}"));
    fs::create_dir_all(&dir)
        .map_err(|e| format!("create Dolphin user dir {}: {e}", dir.display()))?;
    Ok(dir)
}

pub fn write_gamesettings(user_dir: &Path) -> Result<(), String> {
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

pub fn ini_set(path: &Path, section: &str, key: &str, value: &str) -> Result<(), String> {
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

pub fn write_dolphin_config(user_dir: &Path) -> Result<(), String> {
    let config_dir = user_dir.join("Config");
    fs::create_dir_all(&config_dir)
        .map_err(|e| format!("create Dolphin config dir {}: {e}", config_dir.display()))?;
    let path = config_dir.join("Dolphin.ini");
    ini_set(&path, "Display", "Fullscreen", "True")
}

pub fn playback_output_dir() -> PathBuf {
    if let Ok(raw) = env::var("PLAYBACK_OUTPUT_DIR") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return resolve_repo_path(trimmed);
        }
    }
    repo_root().join("airlock").join("tmp")
}

pub fn slippi_appimage_path() -> Result<PathBuf, String> {
    let config = load_config_inner()?;
    let trimmed = config.slippi_launcher_path.trim();
    if trimmed.is_empty() {
        return Err("Slippi launcher path is empty; set it in Settings or SLIPPI_APPIMAGE_PATH.".into());
    }

    let path = resolve_repo_path(trimmed);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "Slippi launcher not found at {}. Update Settings or SLIPPI_APPIMAGE_PATH.",
            path.display()
        ))
    }
}

pub fn slippi_display_override() -> Option<String> {
    env::var("SLIPPI_DISPLAY").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn target_display() -> Result<String, String> {
    if let Some(d) = slippi_display_override() {
        return Ok(d);
    }
    env::var("DISPLAY").map_err(|_| "DISPLAY is not set; set DISPLAY or SLIPPI_DISPLAY".to_string())
}

pub fn launch_dolphin_for_setup_internal(setup_id: u32) -> Result<Child, String> {
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

pub fn launch_dolphin_playback_for_setup_internal(setup_id: u32, replay_path: &Path) -> Result<Child, String> {
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
    let (playback_config, file_basename) = crate::replay::write_playback_config(replay_path, &output_dir, &command_id)?;

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
pub fn launch_dolphin_for_setup(setup_id: u32, store: State<'_, SharedSetupStore>) -> Result<(), String> {
    let (existing, existing_pid) = {
        let mut guard = store.lock().map_err(|e| e.to_string())?;
        if !guard.setups.iter().any(|s| s.id == setup_id) {
            return Err("Setup not found".to_string());
        }
        (
            guard.processes.remove(&setup_id),
            guard.process_pids.remove(&setup_id),
        )
    };

    if let Some(child) = existing {
        stop_dolphin_child(child)?;
    }
    if let Some(pid) = existing_pid {
        stop_process_by_pid(pid)?;
    }

    let child = launch_dolphin_for_setup_internal(setup_id)?;
    let mut guard = store.lock().map_err(|e| e.to_string())?;
    guard.processes.insert(setup_id, child);
    Ok(())
}

#[tauri::command]
pub fn launch_dolphin_cli(extra_args: Option<Vec<String>>) -> Result<(), String> {
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
