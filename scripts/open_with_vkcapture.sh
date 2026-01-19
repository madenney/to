#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: open_with_vkcapture.sh [-n count] [replay.slp] [-- dolphin args...]

Launches Slippi/Dolphin with OBS_VKCAPTURE enabled for OBS Game Capture.
If no replay is provided, a random .slp from test_files/ is used.
With -n > 1 and no replay provided, each instance uses a random replay.
If a replay is provided with -n > 1, the same replay is used for all instances.

Options:
  -n count           Number of Dolphin instances to launch (default: 1)
  -h, --help         Show this help

Env:
  DOLPHIN_PATH       Path to your Slippi Dolphin binary (required)
  SSBM_ISO_PATH      Game/ISO path to boot (required; used with DOLPHIN_EXEC_FLAG)
  DOLPHIN_EXEC_FLAG  Exec flag for SSBM_ISO_PATH (default: -e)
  DOLPHIN_USER_DIR   Dolphin user dir (default: /tmp/slippi-playback-clean-<command_id>)
  DOLPHIN_GAMESETTINGS_ID  GameSettings file id (default: GALE01r2)
  PLAYBACK_OUTPUT_DIR  Directory for playback output + config JSON (default: airlock/tmp)
  SLIPPI_LAST_FRAME    Override replay last frame (auto-detected when possible)
  SLIPPI_NODE_PATH     node_modules path containing @slippi/slippi-js (default: repo or replay_archiver)
  DOLPHIN_BATCH        Set to 1 to use batch mode (-b) like replay_archiver (default: 1)
  OBS_VKCAPTURE      Set to 1 to enable Vulkan capture (default: 1)
  USE_OBS_GAMECAPTURE Use obs-gamecapture LD_PRELOAD (default: 1)
  OBS_GAMECAPTURE    Override obs-gamecapture binary (default: obs-gamecapture in PATH)
EOF
}

dolphin_count=1
force_random_replay=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    -n)
      shift
      if [[ $# -lt 1 ]]; then
        echo "Option -n requires a value." >&2
        exit 1
      fi
      dolphin_count="$1"
      shift
      ;;
    --)
      force_random_replay=1
      shift
      break
      ;;
    -* )
      echo "Unknown option: $1" >&2
      usage
      exit 1
      ;;
    *)
      break
      ;;
  esac
done

if ! [[ "$dolphin_count" =~ ^[0-9]+$ ]] || (( dolphin_count < 1 )); then
  echo "Invalid -n value: $dolphin_count" >&2
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
env_file="$repo_root/.env"
if [[ -f "$env_file" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$env_file"
  set +a
fi

require_env_file() {
  local key="$1"
  local value="${!key:-}"
  if [[ -z "$value" ]]; then
    echo "$key is required. Set it in $env_file or export it." >&2
    exit 1
  fi
  if [[ ! -f "$value" ]]; then
    echo "$key not found at: $value" >&2
    exit 1
  fi
}

require_env_file "DOLPHIN_PATH"
require_env_file "SSBM_ISO_PATH"
if [[ ! -x "$DOLPHIN_PATH" ]]; then
  echo "DOLPHIN_PATH is not executable: $DOLPHIN_PATH (try: chmod +x \"$DOLPHIN_PATH\")" >&2
  exit 1
fi

OBS_VKCAPTURE="${OBS_VKCAPTURE:-1}"
USE_OBS_GAMECAPTURE="${USE_OBS_GAMECAPTURE:-1}"
export OBS_VKCAPTURE

print_cmd() {
  local -a cmd=( "$@" )
  printf 'Running: OBS_VKCAPTURE=%q ' "$OBS_VKCAPTURE"
  printf '%q ' "${cmd[@]}"
  printf '\n'
}

json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\r'/\\r}"
  s="${s//$'\t'/\\t}"
  printf '%s' "$s"
}

resolve_slippi_node_path() {
  if [[ -n "${SLIPPI_NODE_PATH:-}" ]]; then
    echo "$SLIPPI_NODE_PATH"
    return 0
  fi
  local candidate
  for candidate in "$repo_root/node_modules" "$repo_root/../replay_archiver/node_modules"; do
    if [[ -d "$candidate/@slippi/slippi-js" ]]; then
      echo "$candidate"
      return 0
    fi
  done
  return 1
}

get_last_frame() {
  local slp_file="$1"
  local node_path
  node_path="$(resolve_slippi_node_path)" || return 1
  NODE_PATH="$node_path" node -e "const { SlippiGame } = require('@slippi/slippi-js'); const input = process.argv[1]; if (!input) process.exit(2); const game = new SlippiGame(input); const meta = game.getMetadata() || {}; let last = typeof meta.lastFrame === 'number' ? meta.lastFrame : null; if (last === null) { const frames = game.getFrames() || {}; for (const key of Object.keys(frames)) { const num = Number(key); if (Number.isFinite(num)) { if (last === null || num > last) last = num; } } } if (last === null) process.exit(2); console.log(last);" "$slp_file"
}

make_command_id() {
  hexdump -n 12 -e '/1 "%02x"' /dev/urandom 2>/dev/null || date +%s%N
}

make_abs_path() {
  local path="$1"
  if [[ "$path" != /* ]]; then
    path="$(cd "$(dirname "$path")" && pwd)/$(basename "$path")"
  fi
  printf '%s' "$path"
}

shuffle_candidates() {
  shuffled_replays=("${candidates[@]}")
  local i j tmp
  for ((i=${#shuffled_replays[@]}-1; i>0; i--)); do
    j=$((RANDOM % (i + 1)))
    tmp="${shuffled_replays[i]}"
    shuffled_replays[i]="${shuffled_replays[j]}"
    shuffled_replays[j]="$tmp"
  done
  replay_index=0
}

picked_replay=""

pick_random_replay() {
  if (( ${#shuffled_replays[@]} == 0 )); then
    return 1
  fi
  if (( replay_index >= ${#shuffled_replays[@]} )); then
    return 1
  fi
  local pick="${shuffled_replays[$replay_index]}"
  replay_index=$((replay_index + 1))
  picked_replay="$pick"
}

make_instance_binary() {
  local instance_label="$1"

  if [[ -z "${dolphin_bin_root:-}" ]]; then
    printf '%s' "$DOLPHIN_PATH"
    return 0
  fi

  local dest="$dolphin_bin_root/$instance_label"
  if ! ln "$DOLPHIN_PATH" "$dest" 2>/dev/null; then
    if ! cp -f "$DOLPHIN_PATH" "$dest" 2>/dev/null; then
      echo "Warning: unable to create Dolphin instance binary at $dest; using original binary." >&2
      printf '%s' "$DOLPHIN_PATH"
      return 0
    fi
  fi
  chmod +x "$dest" 2>/dev/null || true
  printf '%s' "$dest"
}

write_gamesettings() {
  local user_dir="$1"
  local settings_id="${DOLPHIN_GAMESETTINGS_ID:-GALE01r2}"
  local settings_dir="$user_dir/GameSettings"

  mkdir -p "$settings_dir"
  cat >"$settings_dir/${settings_id}.ini" <<'EOF'
[Gecko]

[Gecko_Enabled]
$Optional: Game Music OFF
$Optional: Widescreen 16:9
EOF
}

ini_set() {
  local file="$1"
  local section="$2"
  local key="$3"
  local value="$4"

  if [[ ! -f "$file" ]]; then
    printf '[%s]\n%s = %s\n' "$section" "$key" "$value" >"$file"
    return 0
  fi

  local tmp="${file}.tmp.$$"
  awk -v section="$section" -v key="$key" -v value="$value" '
    BEGIN { in_section=0; seen_section=0; done=0 }
    /^[[:space:]]*\[/ {
      if (in_section && !done) { print key " = " value; done=1 }
      if ($0 == "[" section "]") { in_section=1; seen_section=1 } else { in_section=0 }
      print $0
      next
    }
    {
      if (in_section && $0 ~ "^[[:space:]]*" key "[[:space:]]*=") {
        if (!done) { print key " = " value; done=1 }
        next
      }
      print $0
    }
    END {
      if (!seen_section) { print "[" section "]" }
      if (!done) { print key " = " value }
    }
  ' "$file" >"$tmp"
  mv "$tmp" "$file"
}

write_dolphin_config() {
  local user_dir="$1"
  local config_dir="$user_dir/Config"
  mkdir -p "$config_dir"
  ini_set "$config_dir/Dolphin.ini" "Display" "Fullscreen" "True"
}

exe_override_lib=""
build_exe_override_lib() {
  local src="$repo_root/scripts/vkcapture_exe_override.c"
  local out="$repo_root/scripts/vkcapture_exe_override.so"
  local cc_bin=""

  if [[ -f "$out" && ( ! -f "$src" || "$src" -ot "$out" ) ]]; then
    exe_override_lib="$out"
    return 0
  fi

  if [[ -f "$src" ]]; then
    for candidate in cc gcc clang; do
      if command -v "$candidate" >/dev/null 2>&1; then
        cc_bin="$candidate"
        break
      fi
    done
  fi

  if [[ -z "$cc_bin" || ! -f "$src" ]]; then
    if [[ -f "$out" ]]; then
      echo "Warning: using existing $out; rebuild unavailable." >&2
      exe_override_lib="$out"
      return 0
    fi
    echo "Warning: cannot build $out; OBS may not distinguish instances." >&2
    return 1
  fi

  if ! "$cc_bin" -shared -fPIC -O2 -o "$out" "$src" -ldl; then
    if [[ -f "$out" ]]; then
      echo "Warning: build failed; using existing $out." >&2
      exe_override_lib="$out"
      return 0
    fi
    echo "Warning: failed to build $out; OBS may not distinguish instances." >&2
    return 1
  fi

  exe_override_lib="$out"
}

explicit_replay=0
slp_arg=""
dolphin_args=()

if (( force_random_replay == 1 )); then
  dolphin_args=("$@")
else
  if [[ $# -gt 0 ]]; then
    slp_arg="$1"
    shift
    explicit_replay=1
    if [[ ! -f "$slp_arg" ]]; then
      echo "Slippi file not found: $slp_arg" >&2
      exit 1
    fi
  fi
  dolphin_args=("$@")
fi

candidates=()
shuffled_replays=()
replay_index=0

if (( explicit_replay == 0 )); then
  test_dir="$repo_root/test_files"
  if [[ ! -d "$test_dir" ]]; then
    echo "test_files directory not found: $test_dir" >&2
    exit 1
  fi

  mapfile -t candidates < <(find "$test_dir" -type f \( -iname "*.slp" -o -iname "*.slippi" \) 2>/dev/null)
  if (( ${#candidates[@]} == 0 )); then
    echo "No .slp files found in $test_dir" >&2
    exit 1
  fi
  shuffle_candidates
  if (( dolphin_count > ${#shuffled_replays[@]} )); then
    echo "Requested $dolphin_count instances but only ${#shuffled_replays[@]} replays found in $test_dir." >&2
    exit 1
  fi
fi

if [[ -n "${SLIPPI_LAST_FRAME:-}" && ! "${SLIPPI_LAST_FRAME}" =~ ^-?[0-9]+$ ]]; then
  echo "Invalid SLIPPI_LAST_FRAME: $SLIPPI_LAST_FRAME" >&2
  exit 1
fi
if [[ -z "${SLIPPI_LAST_FRAME:-}" ]]; then
  if ! command -v node >/dev/null 2>&1; then
    echo "node is required to auto-detect replay length; set SLIPPI_LAST_FRAME or install node." >&2
    exit 1
  fi
fi

playback_output_dir="${PLAYBACK_OUTPUT_DIR:-$repo_root/airlock/tmp}"
mkdir -p "$playback_output_dir"
playback_output_dir="$(cd "$playback_output_dir" && pwd)"
echo "Output dir: ${playback_output_dir#"$repo_root"/}"

if [[ "$USE_OBS_GAMECAPTURE" == "1" ]]; then
  obs_gamecapture="${OBS_GAMECAPTURE:-obs-gamecapture}"
  if ! command -v "$obs_gamecapture" >/dev/null 2>&1; then
    echo "obs-gamecapture not found. Install obs-vkcapture or set OBS_GAMECAPTURE." >&2
    exit 1
  fi
fi

ssbm_iso_path="$SSBM_ISO_PATH"
dolphin_exec_flag="${DOLPHIN_EXEC_FLAG:--e}"
DOLPHIN_BATCH="${DOLPHIN_BATCH:-1}"

run_id="$(make_command_id)"

if (( dolphin_count > 1 )); then
  build_exe_override_lib || true
fi

dolphin_bin_root=""
if (( dolphin_count > 1 )); then
  dolphin_bin_root="$(mktemp -d "${TMPDIR:-/tmp}/slippi-dolphin-bin-XXXXXX" 2>/dev/null || true)"
  if [[ -z "$dolphin_bin_root" ]]; then
    echo "Warning: could not create temp dir for unique Dolphin names; OBS may not distinguish instances." >&2
  else
    trap 'rm -rf "$dolphin_bin_root"' EXIT
  fi
fi

if (( explicit_replay == 1 )); then
  slp_arg="$(make_abs_path "$slp_arg")"
  rel_path="${slp_arg#"$repo_root"/}"
  if (( dolphin_count > 1 )); then
    echo "Using replay (all instances): $rel_path"
  else
    echo "Using replay: $rel_path"
  fi
fi

launch_pids=()

for ((i=1; i<=dolphin_count; i++)); do
  if (( explicit_replay == 1 )); then
    slp="$slp_arg"
  else
    if ! pick_random_replay; then
      echo "Failed to select a replay from test_files." >&2
      exit 1
    fi
    slp="$picked_replay"
    slp="$(make_abs_path "$slp")"
    rel_path="${slp#"$repo_root"/}"
    if (( dolphin_count > 1 )); then
      echo "Using replay ($i/$dolphin_count): $rel_path"
    else
      echo "Using replay: $rel_path"
    fi
  fi

  slippi_last_frame="${SLIPPI_LAST_FRAME:-}"
  if [[ -z "$slippi_last_frame" ]]; then
    if ! slippi_last_frame="$(get_last_frame "$slp")"; then
      echo "Failed to read replay length via @slippi/slippi-js. Set SLIPPI_NODE_PATH or SLIPPI_LAST_FRAME." >&2
      exit 1
    fi
  fi
  if ! [[ "$slippi_last_frame" =~ ^-?[0-9]+$ ]]; then
    echo "Invalid SLIPPI_LAST_FRAME: $slippi_last_frame" >&2
    exit 1
  fi

  start_frame=-123
  end_frame=$((slippi_last_frame - 1))
  if (( end_frame <= start_frame )); then
    end_frame=$((start_frame + 1))
  fi

  command_id="$(make_command_id)"
  file_basename="playback_${command_id}"
  playback_config_path="$playback_output_dir/${file_basename}.json"
  replay_json="$(json_escape "$slp")"
  cat >"$playback_config_path" <<EOF
{
  "mode": "normal",
  "replay": "$replay_json",
  "startFrame": $start_frame,
  "endFrame": $end_frame,
  "isRealTimeMode": false,
  "commandId": "$command_id"
}
EOF

  playback_config_rel="${playback_config_path#"$repo_root"/}"
  if (( dolphin_count > 1 )); then
    echo "Using playback config ($i/$dolphin_count): $playback_config_rel"
  else
    echo "Using playback config: $playback_config_rel"
  fi

  if [[ -n "${DOLPHIN_USER_DIR:-}" && $dolphin_count -gt 1 ]]; then
    dolphin_user_dir="${DOLPHIN_USER_DIR}-${run_id}-${i}"
  else
    dolphin_user_dir="${DOLPHIN_USER_DIR:-/tmp/slippi-playback-clean-$command_id}"
  fi
  mkdir -p "$dolphin_user_dir"
  write_gamesettings "$dolphin_user_dir"
  write_dolphin_config "$dolphin_user_dir"

  instance_label="dolphin-$i"
  dolphin_path="$(make_instance_binary "$instance_label")"

  env_prefix=()
  if (( dolphin_count > 1 )); then
    env_prefix+=( "OBS_VKCAPTURE_EXE_NAME=$instance_label" )
    if [[ -n "$exe_override_lib" ]]; then
      if [[ -n "${LD_PRELOAD:-}" ]]; then
        env_prefix+=( "LD_PRELOAD=$exe_override_lib:$LD_PRELOAD" )
      else
        env_prefix+=( "LD_PRELOAD=$exe_override_lib" )
      fi
    fi
  fi
  if (( ${#env_prefix[@]} > 0 )); then
    env_prefix=( env "${env_prefix[@]}" )
  fi

  launch_args=( "--user" "$dolphin_user_dir" "-i" "$playback_config_path" "-o" "${file_basename}-unmerged" "--output-directory=$playback_output_dir" )
  if [[ "$DOLPHIN_BATCH" == "1" ]]; then
    launch_args+=( "-b" )
  fi
  launch_args+=( "$dolphin_exec_flag" "$ssbm_iso_path" )
  launch_args+=( "${dolphin_args[@]}" )

  if [[ "$USE_OBS_GAMECAPTURE" == "1" ]]; then
    cmd=( "${env_prefix[@]}" "$obs_gamecapture" "$dolphin_path" "${launch_args[@]}" )
  else
    cmd=( "${env_prefix[@]}" "$dolphin_path" "${launch_args[@]}" )
  fi

  print_cmd "${cmd[@]}"

  if (( dolphin_count == 1 )); then
    exec "${cmd[@]}"
  else
    "${cmd[@]}" &
    launch_pids+=("$!")
  fi
done

if (( dolphin_count > 1 )); then
  wait "${launch_pids[@]}"
fi
