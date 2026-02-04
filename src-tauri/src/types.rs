use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Child,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use crate::startgg_sim::{StartggSim, StartggSimSet, StartggSimState};

// ── Constants ──────────────────────────────────────────────────────────

pub const TEST_STREAM_LIMIT: usize = 8;
pub const MAX_SETUP_COUNT: usize = 16;
pub const STARTGG_API_URL: &str = "https://api.start.gg/gql/alpha";
pub const STARTGG_ENTRANTS_PER_PAGE: i32 = 200;
pub const STARTGG_SETS_PER_PAGE: i32 = 200;
pub const STARTGG_POLL_INTERVAL_MS: u64 = 1000;
pub const STARTGG_IDLE_REFRESH_MS: u64 = 10_000;

// ── Shared state type aliases ──────────────────────────────────────────

pub type SharedSetupStore = Arc<Mutex<SetupStore>>;
pub type SharedTestState = Arc<Mutex<TestModeState>>;
pub type SharedOverlayCache = Arc<Mutex<OverlayReplayCache>>;
pub type SharedLiveStartgg = Arc<Mutex<LiveStartggState>>;

// ── App domain types ───────────────────────────────────────────────────

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignStreamResult {
    pub setups: Vec<Setup>,
    pub warning: Option<String>,
}

#[derive(Default)]
pub struct SetupStore {
    pub setups: Vec<Setup>,
    pub processes: HashMap<u32, Child>,
    pub process_pids: HashMap<u32, u32>,
}

impl SetupStore {
    pub fn bootstrap_from_existing() -> Self {
        SetupStore {
            setups: vec![
                Setup {
                    id: 1,
                    name: "Setup 1".to_string(),
                    assigned_stream: None,
                },
                Setup {
                    id: 2,
                    name: "Setup 2".to_string(),
                    assigned_stream: None,
                },
                Setup {
                    id: 3,
                    name: "Setup 3".to_string(),
                    assigned_stream: None,
                },
            ],
            processes: HashMap::new(),
            process_pids: HashMap::new(),
        }
    }
}

pub struct TestModeState {
    pub spoof_streams: Vec<SlippiStream>,
    pub spoof_replays: HashMap<String, PathBuf>,
    pub startgg_sim: Option<StartggSim>,
    pub startgg_config_path: Option<PathBuf>,
    pub broadcast_filter_enabled: bool,
    pub broadcast_codes: HashSet<String>,
    pub broadcast_tags: HashSet<String>,
    pub broadcast_players: Vec<BroadcastPlayerSelection>,
    pub active_replay_sets: HashSet<u64>,
    pub active_replay_paths: HashMap<u64, PathBuf>,
    pub active_replay_children: HashMap<u64, Child>,
    pub cancel_replay_sets: HashSet<u64>,
}

impl Default for TestModeState {
    fn default() -> Self {
        Self {
            spoof_streams: Vec::new(),
            spoof_replays: HashMap::new(),
            startgg_sim: None,
            startgg_config_path: None,
            broadcast_filter_enabled: true,
            broadcast_codes: HashSet::new(),
            broadcast_tags: HashSet::new(),
            broadcast_players: Vec::new(),
            active_replay_sets: HashSet::new(),
            active_replay_paths: HashMap::new(),
            active_replay_children: HashMap::new(),
            cancel_replay_sets: HashSet::new(),
        }
    }
}

#[derive(Default)]
pub struct LiveStartggState {
    pub state: Option<StartggSimState>,
    pub last_fetch: Option<SystemTime>,
    pub last_error: Option<String>,
    pub event_slug: Option<String>,
    pub startgg_link: Option<String>,
    pub fetch_in_flight: bool,
}

#[derive(Clone)]
pub struct OverlayServerState {
    pub setup_store: SharedSetupStore,
    pub test_state: SharedTestState,
    pub live_startgg: SharedLiveStartgg,
    pub replay_cache: SharedOverlayCache,
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
    pub startgg_entrant_id: Option<u32>,
    pub replay_path: Option<String>,
    pub is_playing: Option<bool>,
    pub source: Option<String>,
    pub startgg_set: Option<StartggSimSet>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastPlayerSelection {
    pub id: u32,
    pub name: String,
    pub slippi_code: String,
}

#[derive(Debug, Clone, Serialize)]
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

// ── Overlay types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub side: String,
    pub port: Option<u8>,
    pub tag: String,
    pub sponsor: Option<String>,
    pub handle: Option<String>,
    pub character: String,
    pub character_color: String,
    pub score: u32,
    pub country_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryState {
    pub name: String,
    pub handle: Option<String>,
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchMeta {
    pub tournament: Option<String>,
    pub round: String,
    pub best_of: u8,
    pub game_number: Option<u32>,
    pub stage: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayState {
    pub p1: PlayerState,
    pub p2: PlayerState,
    pub meta: MatchMeta,
    pub commentators: Vec<CommentaryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllSetupsState {
    pub setups: Vec<OverlayState>,
}

// ── Replay parsing types ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ParsedPlayerInfo {
    pub port: u8,
    pub tag: Option<String>,
    pub code: Option<String>,
    pub character: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedGameInfo {
    pub players: Vec<ParsedPlayerInfo>,
}

#[derive(Debug, Clone)]
pub struct ParsedReplay {
    pub info: ParsedGameInfo,
    pub modified: SystemTime,
}

#[derive(Debug, Default)]
pub struct OverlayReplayCache {
    pub last_scan: Option<SystemTime>,
    pub replay_mtimes: HashMap<String, SystemTime>,
    pub replay_codes: HashMap<String, Vec<String>>,
    pub code_index: HashMap<String, String>,
    pub parsed: HashMap<String, ParsedReplay>,
}

// ── Config types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub dolphin_path: String,
    pub ssbm_iso_path: String,
    pub slippi_launcher_path: String,
    pub spectate_folder_path: String,
    pub startgg_link: String,
    pub startgg_token: String,
    pub startgg_polling: bool,
    pub auto_stream: bool,
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
            startgg_link: String::new(),
            startgg_token: String::new(),
            startgg_polling: false,
            auto_stream: true,
            test_mode: false,
            test_bracket_path: "test_brackets/test_bracket_2.json".to_string(),
            auto_complete_bracket: true,
        }
    }
}

// ── Dolphin types ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct DolphinConfig {
    pub dolphin_path: PathBuf,
    pub ssbm_iso_path: PathBuf,
}

// ── CDP types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CdpTarget {
    pub id: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(rename = "webSocketDebuggerUrl")]
    pub ws_url: Option<String>,
}

// ── Test stream types ──────────────────────────────────────────────────

#[derive(Debug)]
pub struct TestStreamSpec {
    pub stream: SlippiStream,
    pub replay_path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaySpoofMode {
    Stream,
    Copy,
}

// ── Start.gg link parsing ──────────────────────────────────────────────

#[derive(Default)]
pub struct StartggLinkInfo {
    pub tournament_slug: Option<String>,
    pub event_slug: Option<String>,
}

// ── Start.gg live snapshot ─────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggLiveSnapshot {
    pub state: Option<StartggSimState>,
    pub last_error: Option<String>,
    pub last_fetch_ms: Option<u64>,
}

// ── Overlay server dirs ────────────────────────────────────────────────

pub struct OverlayDirs {
    pub root: PathBuf,
    pub resources: PathBuf,
    pub upcoming: PathBuf,
    pub dual: PathBuf,
    pub quad: PathBuf,
}

// ── Start.gg GraphQL response types ────────────────────────────────────

#[derive(Deserialize)]
pub struct StartggGraphqlResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<StartggGraphqlError>>,
}

#[derive(Deserialize)]
pub struct StartggGraphqlError {
    pub message: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggEventInfoData {
    pub event: Option<StartggEventInfoNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggEventInfoNode {
    pub id: Option<Value>,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub phases: Option<Vec<StartggPhaseNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggPhaseNode {
    pub id: Option<Value>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggTournamentEventsData {
    pub tournament: Option<StartggTournamentNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggTournamentNode {
    pub events: Option<Vec<StartggTournamentEventNode>>,
}

#[derive(Deserialize)]
pub struct StartggTournamentEventsConnectionData {
    pub tournament: Option<StartggTournamentNodeConnection>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggTournamentNodeConnection {
    pub events: Option<StartggTournamentEventsConnection>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggTournamentEventsConnection {
    pub nodes: Option<Vec<StartggTournamentEventNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggTournamentEventNode {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub videogame: Option<StartggVideogameNode>,
    #[serde(rename = "type")]
    pub kind: Option<i32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggVideogameNode {
    pub id: Option<Value>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggEntrantsData {
    pub event: Option<StartggEntrantsEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggEntrantsEvent {
    pub entrants: Option<StartggEntrantConnection>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggEntrantConnection {
    pub nodes: Option<Vec<StartggEntrantNode>>,
    pub page_info: Option<StartggPageInfo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSetsData {
    pub event: Option<StartggSetsEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSetsEvent {
    pub sets: Option<StartggSetConnection>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSetConnection {
    pub nodes: Option<Vec<StartggSetNode>>,
    pub page_info: Option<StartggPageInfo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggPageInfo {
    pub total_pages: Option<i32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggEntrantNode {
    pub id: Option<Value>,
    pub name: Option<String>,
    pub seeds: Option<Vec<StartggSeedNode>>,
    pub seed: Option<i32>,
    pub slippi_code: Option<String>,
    pub custom_fields: Option<Vec<StartggCustomField>>,
    pub participants: Option<Vec<StartggParticipantNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSeedNode {
    pub seed_num: Option<i32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggCustomField {
    pub name: Option<String>,
    pub value: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggParticipantNode {
    pub gamer_tag: Option<String>,
    pub player: Option<StartggPlayerNode>,
    pub user: Option<StartggUserNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggPlayerNode {
    pub gamer_tag: Option<String>,
    pub slippi_code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggUserNode {
    pub authorizations: Option<Vec<StartggAuthorizationNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggAuthorizationNode {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub external_username: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSetNode {
    pub id: Option<Value>,
    pub round: Option<i32>,
    pub full_round_text: Option<String>,
    pub state: Option<Value>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub winner_id: Option<Value>,
    pub phase_group: Option<StartggPhaseGroupNode>,
    pub slots: Option<Vec<StartggSetSlotNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggPhaseGroupNode {
    pub phase: Option<StartggPhaseNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSetSlotNode {
    pub entrant: Option<StartggEntrantStub>,
    pub standing: Option<StartggStandingNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggEntrantStub {
    pub id: Option<Value>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggStandingNode {
    pub stats: Option<StartggStatsNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggStatsNode {
    pub score: Option<StartggScoreNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggScoreNode {
    pub value: Option<f64>,
    pub label: Option<String>,
}
