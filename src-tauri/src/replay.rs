use crate::config::*;
use crate::types::*;
use crate::startgg_sim::{StartggSimSet, StartggSimSlot, StartggSimState};
use chrono::{DateTime, Datelike, Local, NaiveDateTime, Timelike, Utc};
use peppi::{game::{Game, Port}, io::slippi};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn collect_slp_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
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

pub fn extract_connect_codes(bytes: &[u8]) -> Vec<String> {
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

pub fn most_common_connect_code(files: &[PathBuf]) -> Result<String, String> {
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

pub fn find_opponent_code(primary: &str, files: &[PathBuf]) -> Option<String> {
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

pub fn find_opponent_code_in_replay(primary: &str, replay_path: &Path) -> Option<String> {
    let primary_norm = normalize_slippi_code(primary)?;
    let bytes = fs::read(replay_path).ok()?;
    let codes = extract_connect_codes(&bytes);
    for code in codes {
        let Some(norm) = normalize_slippi_code(&code) else {
            continue;
        };
        if norm != primary_norm {
            return Some(code);
        }
    }
    None
}

pub fn tag_from_code(code: &str) -> String {
    code.split('#').next().unwrap_or(code).to_string()
}

pub fn map_character(id: u8) -> Option<&'static str> {
    match id {
        0x00 => Some("Captain Falcon"),
        0x01 => Some("Donkey Kong"),
        0x02 => Some("Fox"),
        0x03 => Some("Mr Game & Watch"),
        0x04 => Some("Kirby"),
        0x05 => Some("Bowser"),
        0x06 => Some("Link"),
        0x07 => Some("Luigi"),
        0x08 => Some("Mario"),
        0x09 => Some("Marth"),
        0x0A => Some("Mewtwo"),
        0x0B => Some("Ness"),
        0x0C => Some("Peach"),
        0x0D => Some("Pikachu"),
        0x0E => Some("Ice Climbers"),
        0x0F => Some("Jigglypuff"),
        0x10 => Some("Samus"),
        0x11 => Some("Yoshi"),
        0x12 => Some("Zelda"),
        0x13 => Some("Sheik"),
        0x14 => Some("Falco"),
        0x15 => Some("Young Link"),
        0x16 => Some("Dr Mario"),
        0x17 => Some("Roy"),
        0x18 => Some("Pichu"),
        0x19 => Some("Ganondorf"),
        _ => None,
    }
}

pub fn map_color(char_name: &str, costume: u8) -> &'static str {
    match char_name {
        "Fox" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", _ => "Default" },
        "Falco" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", _ => "Default" },
        "Marth" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "White", 5 => "Black", _ => "Default" },
        "Sheik" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "Purple", _ => "Default" },
        "Zelda" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "Purple", _ => "Default" },
        "Jigglypuff" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "Yellow", _ => "Default" },
        "Captain Falcon" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "White", 5 => "Black", _ => "Default" },
        "Peach" => match costume { 1 => "Blue", 2 => "Green", 3 => "White", 4 => "Yellow", _ => "Default" },
        "Luigi" => match costume { 1 => "Blue", 2 => "Pink", 3 => "White", _ => "Default" },
        "Mario" => match costume { 1 => "Blue", 2 => "Brown", 3 => "Green", 4 => "Yellow", _ => "Default" },
        "Dr Mario" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "Black", _ => "Default" },
        "Pikachu" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", _ => "Default" },
        "Samus" => match costume { 1 => "Brown", 2 => "Green", 3 => "Pink", 4 => "Purple", _ => "Default" },
        "Ganondorf" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "Purple", _ => "Default" },
        "Roy" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "Yellow", _ => "Default" },
        "Young Link" => match costume { 1 => "Red", 2 => "Blue", 3 => "White", 4 => "Black", _ => "Default" },
        "Link" => match costume { 1 => "Red", 2 => "Blue", 3 => "White", 4 => "Black", _ => "Default" },
        "Yoshi" => match costume { 1 => "Red", 2 => "Blue", 3 => "Cyan", 4 => "Pink", 5 => "Yellow", _ => "Default" },
        "Ice Climbers" => match costume { 1 => "Red", 2 => "Green", 3 => "Orange", _ => "Default" },
        "Kirby" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "White", 5 => "Yellow", _ => "Default" },
        "Mewtwo" => match costume { 1 => "Blue", 2 => "Green", 3 => "Yellow", _ => "Default" },
        "Ness" => match costume { 1 => "Blue", 2 => "Green", 3 => "Yellow", _ => "Default" },
        "Bowser" => match costume { 1 => "Red", 2 => "Blue", 3 => "Black", _ => "Default" },
        "Pichu" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", _ => "Default" },
        "Mr Game & Watch" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", _ => "Default" },
        "Donkey Kong" => match costume { 1 => "Red", 2 => "Blue", 3 => "Green", 4 => "Purple", _ => "Default" },
        _ => "Default",
    }
}

pub fn parse_game_start(path: &Path) -> Option<ParsedGameInfo> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    if slippi::de::parse_header(&mut reader, None).is_err() {
        return None;
    }

    let mut opts = slippi::de::Opts::default();
    opts.skip_frames = true;
    let state = slippi::de::parse_start(&mut reader, Some(&opts)).ok()?;
    let start = state.start();
    let mut players = Vec::new();

    for pl in start.players.iter() {
        let name = match map_character(pl.character) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let color = map_color(&name, pl.costume).to_string();
        let netplay = pl.netplay.as_ref().map(|n| (n.name.0.clone(), n.code.0.clone()));
        let tag = netplay
            .as_ref()
            .map(|(n, _)| n.clone())
            .or_else(|| pl.name_tag.as_ref().map(|s| s.0.clone()));
        let code = netplay.as_ref().map(|(_, c)| c.clone());
        let port = match pl.port {
            Port::P1 => 1,
            Port::P2 => 2,
            Port::P3 => 3,
            Port::P4 => 4,
        };

        players.push(ParsedPlayerInfo {
            port,
            tag,
            code,
            character: Some(name),
            color: Some(color),
        });
    }

    if players.is_empty() {
        return None;
    }
    Some(ParsedGameInfo { players })
}

pub fn parse_replay_cached(cache: &mut OverlayReplayCache, path: &Path) -> Option<ParsedGameInfo> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let key = path.to_string_lossy().to_string();
    if let Some(existing) = cache.parsed.get(&key) {
        if existing.modified == modified {
            return Some(existing.info.clone());
        }
    }
    let parsed = parse_game_start(path)?;
    cache.parsed.insert(
        key,
        ParsedReplay {
            info: parsed.clone(),
            modified,
        },
    );
    Some(parsed)
}

pub fn replay_winner_identity(replay_path: &Path) -> Result<(Option<String>, Option<String>), String> {
    let file = fs::File::open(replay_path)
        .map_err(|e| format!("open replay {}: {e}", replay_path.display()))?;
    let mut opts = slippi::de::Opts::default();
    opts.skip_frames = true;
    let game = slippi::de::read(file, Some(&opts))
        .map_err(|e| format!("parse replay {}: {e}", replay_path.display()))?;
    let end = game
        .end
        .as_ref()
        .ok_or_else(|| "Replay is missing end data.".to_string())?;
    let placements = end
        .players
        .as_ref()
        .ok_or_else(|| "Replay is missing placement data.".to_string())?;
    let mut winner_port: Option<Port> = None;
    let mut best = u8::MAX;
    for player in placements {
        if player.placement < best {
            best = player.placement;
            winner_port = Some(player.port);
        }
    }
    let winner_port = winner_port.ok_or_else(|| "Replay winner not found.".to_string())?;
    let start_player = game
        .start
        .players
        .iter()
        .find(|player| player.port == winner_port)
        .ok_or_else(|| "Replay winner missing from start data.".to_string())?;
    let code = start_player
        .netplay
        .as_ref()
        .map(|netplay| netplay.code.0.clone());
    let tag = start_player
        .netplay
        .as_ref()
        .map(|netplay| netplay.name.0.clone())
        .or_else(|| start_player.name_tag.as_ref().map(|tag| tag.0.clone()));
    Ok((code, tag))
}

pub fn set_slot_index_for_identity(
    set: &StartggSimSet,
    winner_code: Option<&str>,
    winner_tag: Option<&str>,
) -> Option<usize> {
    let code_key = winner_code
        .map(normalize_broadcast_key)
        .filter(|key| !key.is_empty());
    let tag_key = winner_tag
        .map(normalize_tag_key)
        .filter(|key| !key.is_empty());

    if let Some(code_key) = code_key.as_ref() {
        for (idx, slot) in set.slots.iter().enumerate() {
            if let Some(code) = slot.slippi_code.as_deref() {
                if normalize_broadcast_key(code) == *code_key {
                    return Some(idx);
                }
            }
        }
    }

    if let Some(tag_key) = tag_key.as_ref() {
        for (idx, slot) in set.slots.iter().enumerate() {
            if let Some(name) = slot.entrant_name.as_deref() {
                if normalize_tag_key(name) == *tag_key {
                    return Some(idx);
                }
            }
        }
    }

    None
}

pub fn update_replay_index(cache: &mut OverlayReplayCache, dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let now = SystemTime::now();
    if let Some(last) = cache.last_scan {
        if now
            .duration_since(last)
            .unwrap_or_else(|_| Duration::from_secs(0))
            < Duration::from_millis(700)
        {
            return Ok(());
        }
    }
    cache.last_scan = Some(now);

    let mut next_mtimes = HashMap::new();
    let mut next_codes = HashMap::new();
    let mut next_index = HashMap::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("read spectate dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read spectate entry {}: {e}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !is_replay_file_path(&path) {
            continue;
        }
        let meta = entry.metadata().map_err(|e| format!("read metadata {}: {e}", path.display()))?;
        let modified = match meta.modified() {
            Ok(modified) => modified,
            Err(_) => continue,
        };
        let key = path.to_string_lossy().to_string();
        let codes = if cache.replay_mtimes.get(&key) == Some(&modified) {
            cache.replay_codes.get(&key).cloned().unwrap_or_default()
        } else {
            let bytes = fs::read(&path).map_err(|e| format!("read replay {}: {e}", path.display()))?;
            extract_connect_codes(&bytes)
        };
        next_mtimes.insert(key.clone(), modified);
        next_codes.insert(key.clone(), codes.clone());

        for code in codes {
            let normalized = normalize_broadcast_key(&code);
            if normalized.is_empty() {
                continue;
            }
            let should_replace = match next_index.get(&normalized) {
                Some(existing_path) => {
                    let prev_time = next_mtimes.get(existing_path).copied().unwrap_or(SystemTime::UNIX_EPOCH);
                    modified > prev_time
                }
                None => true,
            };
            if should_replace {
                next_index.insert(normalized, key.clone());
            }
        }
    }

    cache.replay_mtimes = next_mtimes;
    cache.replay_codes = next_codes;
    cache.code_index = next_index;
    cache.parsed.retain(|path, _| cache.replay_mtimes.contains_key(path));
    Ok(())
}

pub fn latest_replay_for_code(cache: &OverlayReplayCache, code: &str) -> Option<PathBuf> {
    let key = normalize_broadcast_key(code);
    cache.code_index.get(&key).map(PathBuf::from)
}

pub fn select_parsed_players(
    parsed: &ParsedGameInfo,
    broadcaster_code: Option<&str>,
    broadcaster_tag: Option<&str>,
) -> (Option<ParsedPlayerInfo>, Option<ParsedPlayerInfo>) {
    if parsed.players.is_empty() {
        return (None, None);
    }
    let mut players = parsed.players.clone();
    let mut broadcaster_idx = None;
    if let Some(code) = broadcaster_code {
        let key = normalize_broadcast_key(code);
        broadcaster_idx = players
            .iter()
            .position(|player| {
                player
                    .code
                    .as_deref()
                    .map(normalize_broadcast_key)
                    .as_deref()
                    == Some(key.as_str())
            });
    }
    if broadcaster_idx.is_none() {
        if let Some(tag) = broadcaster_tag {
            let key = normalize_tag_key(tag);
            broadcaster_idx = players
                .iter()
                .position(|player| {
                    player
                        .tag
                        .as_deref()
                        .map(normalize_tag_key)
                        .as_deref()
                        == Some(key.as_str())
                });
        }
    }

    if let Some(idx) = broadcaster_idx {
        let broadcaster = players.remove(idx);
        let opponent = players.into_iter().next();
        return (Some(broadcaster), opponent);
    }

    let p1 = parsed.players.iter().find(|p| p.port == 1).cloned();
    let p2 = parsed.players.iter().find(|p| p.port == 2).cloned();
    (p1, p2)
}

pub fn apply_parsed_player(target: &mut PlayerState, parsed: &ParsedPlayerInfo) {
    if let Some(tag) = parsed.tag.as_ref() {
        if !tag.trim().is_empty() {
            target.tag = tag.clone();
        }
    } else if let Some(code) = parsed.code.as_ref() {
        if target.tag.trim().is_empty() || target.tag == "Waiting" {
            target.tag = code.clone();
        }
    }
    if let Some(code) = parsed.code.as_ref() {
        if target.tag.trim().is_empty() {
            target.tag = code.clone();
        }
    }
    if let Some(character) = parsed.character.as_ref() {
        target.character = character.clone();
    }
    if let Some(color) = parsed.color.as_ref() {
        target.character_color = color.clone();
    }
    if parsed.port > 0 {
        target.port = Some(parsed.port);
    }
}

pub fn default_player(side: &str, port: u8, tag: &str, character: &str) -> PlayerState {
    PlayerState {
        side: side.to_string(),
        port: Some(port),
        tag: tag.to_string(),
        sponsor: None,
        handle: None,
        character: character.to_string(),
        character_color: "Default".to_string(),
        score: 0,
        country_code: None,
    }
}

pub fn default_overlay_state(setup_id: u32) -> OverlayState {
    OverlayState {
        p1: default_player("left", 1, "Player 1", "Falco"),
        p2: default_player("right", 2, "Player 2", "Marth"),
        meta: MatchMeta {
            tournament: None,
            round: format!("Setup {setup_id}"),
            best_of: 3,
            game_number: None,
            stage: None,
            notes: None,
        },
        commentators: Vec::new(),
    }
}

pub fn is_replay_file_path(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => matches!(ext.to_ascii_lowercase().as_str(), "slp" | "slippi"),
        None => false,
    }
}

pub fn replay_slots_from_file(path: &Path) -> Vec<Value> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };
    let codes = extract_connect_codes(&bytes);
    let mut unique: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for code in codes {
        let Some(normalized) = normalize_slippi_code(&code) else {
            continue;
        };
        if seen.insert(normalized.clone()) {
            unique.push(normalized);
        }
        if unique.len() >= 2 {
            break;
        }
    }
    unique
        .into_iter()
        .map(|code| json!({ "slippiCode": code }))
        .collect()
}

pub fn scores_from_set(set: &StartggSimSet, player: &BroadcastPlayerSelection) -> (u32, u32) {
    let mut p1_score = 0u32;
    let mut p2_score = 0u32;
    let mut matched = false;
    for slot in &set.slots {
        let score = slot.score.unwrap_or(0) as u32;
        if slot_matches_player(slot, player) {
            p1_score = score;
            matched = true;
        } else {
            p2_score = score;
        }
    }
    if !matched {
        if let Some(slot) = set.slots.get(0) {
            p1_score = slot.score.unwrap_or(0) as u32;
        }
        if let Some(slot) = set.slots.get(1) {
            p2_score = slot.score.unwrap_or(0) as u32;
        }
    }
    (p1_score, p2_score)
}

pub fn slot_label(slot: Option<&StartggSimSlot>) -> (Option<String>, Option<String>) {
    match slot {
        Some(slot) => {
            let code = slot.slippi_code.clone();
            let tag = slot
                .entrant_name
                .clone()
                .or_else(|| code.clone())
                .or_else(|| slot.source_label.clone());
            (tag, code)
        }
        None => (None, None),
    }
}

pub fn slot_matches_player(slot: &StartggSimSlot, player: &BroadcastPlayerSelection) -> bool {
    if let Some(id) = slot.entrant_id {
        if id == player.id {
            return true;
        }
    }
    let code = normalize_broadcast_key(&player.slippi_code);
    if !code.is_empty() {
        if let Some(slot_code) = slot.slippi_code.as_deref() {
            if normalize_broadcast_key(slot_code) == code {
                return true;
            }
        }
    }
    let name = normalize_tag_key(&player.name);
    if !name.is_empty() {
        if let Some(slot_name) = slot.entrant_name.as_deref() {
            if normalize_tag_key(slot_name) == name {
                return true;
            }
        }
    }
    false
}

pub fn build_overlay_for_setup(
    setup_id: u32,
    setup: Option<&Setup>,
    startgg_state: Option<&StartggSimState>,
    active_sets: Option<&HashSet<u64>>,
    config: &AppConfig,
    replay_map: &HashMap<String, PathBuf>,
    replay_cache: &mut OverlayReplayCache,
) -> OverlayState {
    let mut state = default_overlay_state(setup_id);
    let Some(setup) = setup else {
        return state;
    };
    let Some(stream) = setup.assigned_stream.as_ref() else {
        state.meta.round = "Waiting for assignment".to_string();
        return state;
    };

    let mut p1_tag = stream
        .p1_tag
        .clone()
        .or_else(|| stream.p1_code.clone())
        .unwrap_or_else(|| "Player 1".to_string());
    if p1_tag.trim().is_empty() {
        p1_tag = "Player 1".to_string();
    }
    let p1_code = stream.p1_code.clone();
    let mut expected_p2_tag = stream.p2_tag.clone();
    let mut expected_p2_code = stream.p2_code.clone();
    let mut round_label = "Waiting".to_string();
    let mut best_of = 3u8;
    let mut game_number = None;
    let mut p1_score = 0u32;
    let mut p2_score = 0u32;
    let mut tournament = None;
    let mut set_state = None;

    let player = BroadcastPlayerSelection {
        id: stream.startgg_entrant_id.unwrap_or(0),
        name: stream.p1_tag.clone().unwrap_or_default(),
        slippi_code: stream.p1_code.clone().unwrap_or_default(),
    };

    let mut matched_set: Option<StartggSimSet> = None;
    if let Some(state_ref) = startgg_state {
        tournament = Some(state_ref.event.name.clone());
        if !player.name.trim().is_empty() || !player.slippi_code.trim().is_empty() {
            matched_set = find_set_for_player(&state_ref.sets, &player, active_sets).cloned();
        }
    }
    if matched_set.is_none() {
        if let Some(startgg_set) = stream.startgg_set.as_ref() {
            matched_set = Some(startgg_set.clone());
        }
    }

    if let Some(set) = matched_set.as_ref() {
        round_label = set.round_label.clone();
        if set.best_of > 0 {
            best_of = set.best_of;
        }
        set_state = Some(set.state.clone());
        let expected = set
            .slots
            .iter()
            .find(|slot| !slot_matches_player(slot, &player))
            .map(|slot| slot_label(Some(slot)))
            .unwrap_or((None, None));
        if expected.0.is_some() {
            expected_p2_tag = expected.0;
        }
        if expected.1.is_some() {
            expected_p2_code = expected.1;
        }
        let scores = scores_from_set(set, &player);
        p1_score = scores.0;
        p2_score = scores.1;
    }

    state.meta.tournament = tournament;
    state.meta.round = round_label;
    state.meta.best_of = best_of;

    state.p1.tag = p1_tag;
    state.p1.score = p1_score;
    let mut p2_tag = expected_p2_tag
        .or_else(|| expected_p2_code.clone())
        .unwrap_or_else(|| "Waiting".to_string());
    if p2_tag.trim().is_empty() {
        p2_tag = "Waiting".to_string();
    }
    state.p2.tag = p2_tag;
    state.p2.score = p2_score;

    let is_playing = stream.is_playing.unwrap_or(false)
        || matches!(set_state.as_deref(), Some("inProgress"));
    let replay_path = if config.test_mode {
        replay_map.get(&stream.id).cloned()
    } else {
        p1_code
            .as_deref()
            .and_then(|code| latest_replay_for_code(replay_cache, code))
    };
    if let Some(path) = replay_path {
        if let Some(parsed) = parse_replay_cached(replay_cache, &path) {
            let (parsed_p1, parsed_p2) =
                select_parsed_players(&parsed, p1_code.as_deref(), Some(&state.p1.tag));
            if let Some(parsed_player) = parsed_p1 {
                apply_parsed_player(&mut state.p1, &parsed_player);
            }
            if let Some(parsed_player) = parsed_p2 {
                apply_parsed_player(&mut state.p2, &parsed_player);
            }
        }
    }
    if is_playing {
        game_number = Some(p1_score + p2_score + 1);
    }

    state.meta.game_number = game_number;
    state
}

pub fn build_overlay_state(
    setups: &[Setup],
    startgg_state: Option<&StartggSimState>,
    active_sets: Option<&HashSet<u64>>,
    config: &AppConfig,
    replay_map: &HashMap<String, PathBuf>,
    replay_cache: &mut OverlayReplayCache,
) -> AllSetupsState {
    if !config.test_mode {
        let spectate = config.spectate_folder_path.trim();
        if !spectate.is_empty() {
            let dir = resolve_repo_path(spectate);
            let _ = update_replay_index(replay_cache, &dir);
        }
    }
    let mut out = Vec::with_capacity(MAX_SETUP_COUNT);
    for id in 1..=MAX_SETUP_COUNT as u32 {
        let setup = setups.iter().find(|s| s.id == id);
        out.push(build_overlay_for_setup(
            id,
            setup,
            startgg_state,
            active_sets,
            config,
            replay_map,
            replay_cache,
        ));
    }
    AllSetupsState { setups: out }
}

pub fn normalize_timestamp_ms(value: i64) -> i64 {
    if value > 10_000_000_000 {
        value
    } else {
        value * 1000
    }
}

pub fn parse_metadata_timestamp_ms(value: &Value) -> Option<i64> {
    match value {
        Value::String(raw) => {
            if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
                return Some(parsed.timestamp_millis());
            }
            if let Ok(parsed) = raw.parse::<i64>() {
                return Some(normalize_timestamp_ms(parsed));
            }
            for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
                if let Ok(parsed) = NaiveDateTime::parse_from_str(raw, fmt) {
                    return Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc).timestamp_millis());
                }
            }
            None
        }
        Value::Number(num) => num.as_i64().map(normalize_timestamp_ms),
        _ => None,
    }
}

pub fn replay_metadata_timestamp_ms(path: &Path) -> Option<i64> {
    let file = fs::File::open(path).ok()?;
    let mut opts = slippi::de::Opts::default();
    opts.skip_frames = true;
    let game = slippi::de::read(file, Some(&opts)).ok()?;
    let metadata = game.metadata?;
    for key in ["startAt", "playedOn", "startTime", "date"] {
        if let Some(value) = metadata.get(key) {
            if let Some(timestamp) = parse_metadata_timestamp_ms(value) {
                return Some(timestamp);
            }
        }
    }
    None
}

pub fn replay_modified_timestamp_ms(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(duration.as_millis() as i64)
}

pub fn sort_replay_paths_by_start_time(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut entries: Vec<(i64, usize, PathBuf)> = paths
        .into_iter()
        .enumerate()
        .map(|(idx, path)| {
            let key = replay_metadata_timestamp_ms(&path)
                .or_else(|| replay_modified_timestamp_ms(&path))
                .unwrap_or(i64::MAX);
            (key, idx, path)
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    entries.into_iter().map(|(_, _, path)| path).collect()
}

pub fn slippi_last_frame(replay_path: &Path) -> Result<i32, String> {
    let node_path = build_node_path()?;
    let script = r#"
const { SlippiGame } = require('@slippi/slippi-js/node');
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

pub fn write_playback_config(replay_path: &Path, output_dir: &Path, command_id: &str) -> Result<(PathBuf, String), String> {
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

pub fn format_game_name(now: DateTime<Local>) -> String {
    format!(
        "Game_{:04}{:02}{:02}T{:02}{:02}{:02}.slp",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

pub fn unique_spectate_path(output_dir: &Path, base_name: &str, index: usize) -> PathBuf {
    let mut candidate = output_dir.join(base_name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = base_name.trim_end_matches(".slp");
    let mut suffix = index + 1;
    loop {
        let name = format!("{stem}_{suffix}.slp");
        candidate = output_dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
        suffix += 1;
    }
}

pub fn next_reference_step_scores(
    current: [u8; 2],
    target: [u8; 2],
    winner_slot: usize,
) -> Option<[u8; 2]> {
    if winner_slot > 1 {
        return None;
    }
    let loser_slot = if winner_slot == 0 { 1 } else { 0 };
    let mut next = current;
    let current_w = current[winner_slot];
    let current_l = current[loser_slot];
    let target_w = target[winner_slot];
    let target_l = target[loser_slot];
    if current_w >= target_w && current_l >= target_l {
        return None;
    }
    if current_l >= target_l {
        if current_w < target_w {
            next[winner_slot] = current_w.saturating_add(1);
        } else {
            return None;
        }
    } else if current_w >= target_w {
        next[loser_slot] = current_l.saturating_add(1);
    } else if current_w.saturating_add(1) >= target_w {
        next[loser_slot] = current_l.saturating_add(1);
    } else if current_w <= current_l {
        next[winner_slot] = current_w.saturating_add(1);
    } else {
        next[loser_slot] = current_l.saturating_add(1);
    }
    next[winner_slot] = next[winner_slot].min(target_w);
    next[loser_slot] = next[loser_slot].min(target_l);
    Some(next)
}

pub fn find_set_for_player<'a>(
    sets: &'a [StartggSimSet],
    player: &BroadcastPlayerSelection,
    active_sets: Option<&HashSet<u64>>,
) -> Option<&'a StartggSimSet> {
    let mut best: Option<&StartggSimSet> = None;
    let mut best_key = (u8::MAX, u8::MAX, u64::MAX);
    for set in sets {
        if !set.slots.iter().any(|slot| slot_matches_player(slot, player)) {
            continue;
        }
        let is_active = active_sets.map(|active| active.contains(&set.id)).unwrap_or(false);
        let active_rank = if is_active { 0 } else { 1 };
        let key = (active_rank, broadcast_state_rank(&set.state), set.id);
        if key < best_key {
            best = Some(set);
            best_key = key;
        }
    }
    best
}

pub fn broadcast_state_rank(state: &str) -> u8 {
    match state {
        "inProgress" => 0,
        "pending" => 1,
        "completed" => 2,
        "skipped" => 3,
        _ => 4,
    }
}

pub fn stream_matches_broadcast(stream: &SlippiStream, guard: &TestModeState) -> bool {
    let codes = [stream.p1_code.as_deref(), stream.p2_code.as_deref()];
    for code in codes.into_iter().flatten() {
        let key = normalize_broadcast_key(code);
        if !key.is_empty() && guard.broadcast_codes.contains(&key) {
            return true;
        }
    }

    let tags = [stream.p1_tag.as_deref(), stream.p2_tag.as_deref()];
    for tag in tags.into_iter().flatten() {
        let key = normalize_tag_key(tag);
        if !key.is_empty() && guard.broadcast_tags.contains(&key) {
            return true;
        }
    }

    false
}

pub fn filter_broadcast_streams(streams: &[SlippiStream], guard: &TestModeState) -> Vec<SlippiStream> {
    if !guard.broadcast_filter_enabled {
        return streams.to_vec();
    }
    if guard.broadcast_codes.is_empty() && guard.broadcast_tags.is_empty() {
        return Vec::new();
    }

    streams
        .iter()
        .filter(|stream| stream_matches_broadcast(stream, guard))
        .cloned()
        .collect()
}

pub fn set_matches_broadcast(set: &StartggSimSet, guard: &TestModeState) -> bool {
    if guard.broadcast_codes.is_empty() && guard.broadcast_tags.is_empty() {
        return false;
    }
    for slot in &set.slots {
        if let Some(code) = slot.slippi_code.as_deref() {
            let key = normalize_broadcast_key(code);
            if !key.is_empty() && guard.broadcast_codes.contains(&key) {
                return true;
            }
        }
        if let Some(name) = slot.entrant_name.as_deref() {
            let key = normalize_tag_key(name);
            if !key.is_empty() && guard.broadcast_tags.contains(&key) {
                return true;
            }
        }
    }
    false
}
