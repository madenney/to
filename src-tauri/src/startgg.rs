use crate::config::*;
use crate::types::*;
use crate::startgg_sim::{
    StartggSim, StartggSimConfig, StartggSimEntrant, StartggSimEntrantConfig, StartggSimEventConfig,
    StartggSimPhaseConfig, StartggSimSet, StartggSimSlot, StartggSimSimulationConfig, StartggSimState,
};
use crate::test_mode::build_test_streams;
use crate::replay::tag_from_code;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    thread::sleep,
    time::{Duration, SystemTime},
};

// ── GraphQL query constants ────────────────────────────────────────────

pub const STARTGG_EVENT_INFO_QUERY: &str = r#"
query EventInfo($slug: String!) {
  event(slug: $slug) {
    id
    name
    slug
    phases {
      id
      name
    }
  }
}
"#;

pub const STARTGG_TOURNAMENT_EVENTS_QUERY: &str = r#"
query TournamentEvents($slug: String!) {
  tournament(slug: $slug) {
    events {
      name
      slug
      type
      videogame { id name }
    }
  }
}
"#;

pub const STARTGG_TOURNAMENT_EVENTS_QUERY_NODES: &str = r#"
query TournamentEvents($slug: String!) {
  tournament(slug: $slug) {
    events {
      nodes {
        name
        slug
        type
        videogame { id name }
      }
    }
  }
}
"#;

pub const STARTGG_EVENT_ENTRANTS_QUERY: &str = r#"
query EventEntrants($slug: String!, $page: Int!, $perPage: Int!) {
  event(slug: $slug) {
    entrants(query: { page: $page, perPage: $perPage }) {
      pageInfo {
        totalPages
      }
      nodes {
        id
        name
        seeds { seedNum }
        seed
        slippiCode
        customFields { name value }
        participants {
          gamerTag
          player { gamerTag slippiCode }
          user { authorizations { type externalUsername } }
        }
      }
    }
  }
}
"#;

pub const STARTGG_EVENT_ENTRANTS_QUERY_FALLBACK: &str = r#"
query EventEntrants($slug: String!, $page: Int!, $perPage: Int!) {
  event(slug: $slug) {
    entrants(query: { page: $page, perPage: $perPage }) {
      pageInfo {
        totalPages
      }
      nodes {
        id
        name
        seeds { seedNum }
        participants {
          gamerTag
          player { gamerTag }
          user { authorizations { type externalUsername } }
        }
      }
    }
  }
}
"#;

pub const STARTGG_EVENT_SETS_QUERY: &str = r#"
query EventSets($slug: String!, $page: Int!, $perPage: Int!) {
  event(slug: $slug) {
    sets(page: $page, perPage: $perPage) {
      pageInfo {
        totalPages
      }
      nodes {
        id
        round
        fullRoundText
        state
        startedAt
        completedAt
        updatedAt
        winnerId
        phaseGroup {
          phase { id name }
        }
        slots {
          entrant { id name }
          standing { stats { score { value label } } }
        }
      }
    }
  }
}
"#;

// ── Functions ──────────────────────────────────────────────────────────

pub fn startgg_token_from_config(config: &AppConfig) -> Result<String, String> {
  let trimmed = config.startgg_token.trim();
  if !trimmed.is_empty() {
    return Ok(trimmed.to_string());
  }
  env_default("STARTGG_TOKEN")
    .ok_or_else(|| "Start.gg API token is not set (Settings or STARTGG_TOKEN).".to_string())
}

pub fn parse_startgg_link_info(link: &str) -> StartggLinkInfo {
  let trimmed = link.trim();
  if trimmed.is_empty() {
    return StartggLinkInfo::default();
  }
  let without_hash = trimmed.split('#').next().unwrap_or(trimmed);
  let without_query = without_hash.split('?').next().unwrap_or(without_hash);
  let mut path = without_query;
  if let Some(idx) = path.find("start.gg") {
    path = &path[idx + "start.gg".len()..];
  }
  let path = path.trim_matches('/');
  if path.is_empty() {
    return StartggLinkInfo::default();
  }
  let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
  if segments.is_empty() {
    return StartggLinkInfo::default();
  }

  let tournament_slug = if let Some(idx) = segments.iter().position(|s| *s == "tournament") {
    segments.get(idx + 1).map(|slug| slug.to_string())
  } else if segments.len() == 1 {
    Some(segments[0].to_string())
  } else {
    None
  };

  let event_slug = if let (Some(tournament_slug), Some(idx)) = (
    tournament_slug.as_ref(),
    segments.iter().position(|s| *s == "event"),
  ) {
    segments
      .get(idx + 1)
      .map(|event| format!("tournament/{}/event/{}", tournament_slug, event))
  } else {
    None
  };

  StartggLinkInfo {
    tournament_slug,
    event_slug,
  }
}

pub fn startgg_graphql_request<T: DeserializeOwned>(
  config: &AppConfig,
  query: &str,
  variables: Value,
) -> Result<T, String> {
  let token = startgg_token_from_config(config)?;
  let client = reqwest::blocking::Client::new();
  let request_log = {
    let vars = serde_json::to_string_pretty(&variables).unwrap_or_else(|_| variables.to_string());
    format!(
      "url: {STARTGG_API_URL}\nAuthorization: Bearer [redacted]\nUser-Agent: new-melee-stream-tool\nquery:\n{query}\nvariables:\n{vars}"
    )
  };
  append_startgg_log("Start.gg request", &request_log);
  let body_json = json!({ "query": query, "variables": variables });
  let mut last_send_err = String::new();
  let mut resp = None;
  for attempt in 0..3u32 {
    if attempt > 0 {
      sleep(Duration::from_millis(500 * u64::from(attempt)));
    }
    match client
      .post(STARTGG_API_URL)
      .header("Authorization", format!("Bearer {token}"))
      .header("User-Agent", "new-melee-stream-tool")
      .json(&body_json)
      .send()
    {
      Ok(r) => { resp = Some(r); break; }
      Err(e) => {
        last_send_err = format!("Start.gg request failed (attempt {}): {e}", attempt + 1);
        append_startgg_log("Start.gg error", &last_send_err);
      }
    }
  }
  let resp = resp.ok_or_else(|| last_send_err.clone())?;
  let status = resp.status();
  let body = resp.text().map_err(|e| {
    append_startgg_log("Start.gg error", &format!("read failed: {e}"));
    format!("Start.gg read failed: {e}")
  })?;
  append_startgg_log("Start.gg response", &format!("status: {status}\nbody:\n{body}"));
  if !status.is_success() {
    return Err(format!("Start.gg error {status}: {body}"));
  }
  let parsed: StartggGraphqlResponse<T> =
    serde_json::from_str(&body).map_err(|e| {
      append_startgg_log("Start.gg error", &format!("parse failed: {e}"));
      format!("Start.gg parse failed: {e}")
    })?;
  if let Some(errors) = parsed.errors {
    let message = errors
      .into_iter()
      .filter_map(|err| err.message)
      .collect::<Vec<_>>()
      .join(", ");
    if !message.is_empty() {
      append_startgg_log("Start.gg error", &format!("graphql error: {message}"));
      return Err(format!("Start.gg error: {message}"));
    }
  }
  parsed
    .data
    .ok_or_else(|| "Start.gg response missing data.".to_string())
}

pub fn fetch_startgg_event_info(config: &AppConfig, slug: &str) -> Result<StartggEventInfoNode, String> {
  let data: StartggEventInfoData =
    startgg_graphql_request(config, STARTGG_EVENT_INFO_QUERY, json!({ "slug": slug }))?;
  data
    .event
    .ok_or_else(|| "Start.gg event not found.".to_string())
}

pub fn fetch_startgg_entrants(config: &AppConfig, slug: &str) -> Result<Vec<StartggEntrantNode>, String> {
  let mut out = Vec::new();
  let mut page = 1;
  loop {
    let variables = json!({ "slug": slug, "page": page, "perPage": STARTGG_ENTRANTS_PER_PAGE });
    let data: StartggEntrantsData = match startgg_graphql_request(
      config,
      STARTGG_EVENT_ENTRANTS_QUERY,
      variables.clone(),
    ) {
      Ok(data) => data,
      Err(primary_err) => {
        let fallback = startgg_graphql_request(
          config,
          STARTGG_EVENT_ENTRANTS_QUERY_FALLBACK,
          variables,
        );
        match fallback {
          Ok(data) => data,
          Err(fallback_err) => {
            return Err(format!("{primary_err} | {fallback_err}"));
          }
        }
      }
    };
    let Some(event) = data.event else {
      break;
    };
    let Some(entrants) = event.entrants else {
      break;
    };
    if let Some(nodes) = entrants.nodes {
      out.extend(nodes);
    }
    let total_pages = entrants
      .page_info
      .as_ref()
      .and_then(|info| info.total_pages)
      .unwrap_or(page);
    if page >= total_pages {
      break;
    }
    page += 1;
  }
  Ok(out)
}

pub fn fetch_startgg_sets(config: &AppConfig, slug: &str) -> Result<Vec<StartggSetNode>, String> {
  let mut out = Vec::new();
  let mut page = 1;
  loop {
    let data: StartggSetsData = startgg_graphql_request(
      config,
      STARTGG_EVENT_SETS_QUERY,
      json!({ "slug": slug, "page": page, "perPage": STARTGG_SETS_PER_PAGE }),
    )?;
    let Some(event) = data.event else {
      break;
    };
    let Some(sets) = event.sets else {
      break;
    };
    if let Some(nodes) = sets.nodes {
      out.extend(nodes);
    }
    let total_pages = sets
      .page_info
      .as_ref()
      .and_then(|info| info.total_pages)
      .unwrap_or(page);
    if page >= total_pages {
      break;
    }
    page += 1;
  }
  Ok(out)
}

pub fn fetch_startgg_tournament_events(
  config: &AppConfig,
  tournament_slug: &str,
) -> Result<Vec<StartggTournamentEventNode>, String> {
  let primary = startgg_graphql_request::<StartggTournamentEventsData>(
    config,
    STARTGG_TOURNAMENT_EVENTS_QUERY,
    json!({ "slug": tournament_slug }),
  );
  match primary {
    Ok(data) => Ok(data
      .tournament
      .and_then(|tournament| tournament.events)
      .unwrap_or_default()),
    Err(primary_err) => {
      let fallback = startgg_graphql_request::<StartggTournamentEventsConnectionData>(
        config,
        STARTGG_TOURNAMENT_EVENTS_QUERY_NODES,
        json!({ "slug": tournament_slug }),
      );
      match fallback {
        Ok(data) => Ok(data
          .tournament
          .and_then(|tournament| tournament.events)
          .and_then(|events| events.nodes)
          .unwrap_or_default()),
        Err(fallback_err) => Err(format!("{primary_err} | {fallback_err}")),
      }
    }
  }
}

pub fn is_melee_event(event: &StartggTournamentEventNode) -> bool {
  if let Some(videogame) = event.videogame.as_ref() {
    if let Some(id) = videogame.id.as_ref().and_then(value_to_i64) {
      if id == 1 {
        return true;
      }
    }
    if let Some(name) = videogame.name.as_ref() {
      if name.to_lowercase().contains("melee") {
        return true;
      }
    }
  }
  let name = event.name.as_deref().unwrap_or("").to_lowercase();
  let slug = event.slug.as_deref().unwrap_or("").to_lowercase();
  name.contains("melee") || slug.contains("melee")
}

pub fn event_score(event: &StartggTournamentEventNode) -> i32 {
  let name = event.name.as_deref().unwrap_or("").to_lowercase();
  let slug = event.slug.as_deref().unwrap_or("").to_lowercase();
  let is_melee = is_melee_event(event);
  let mut score = if is_melee { 0 } else { 100 };
  let singles = name.contains("singles") || slug.contains("singles");
  let melee_singles = name.contains("melee singles") || slug.contains("melee-singles");
  if melee_singles {
    score -= 20;
  } else if singles {
    score -= 10;
  } else {
    score += 10;
  }
  if let Some(kind) = event.kind {
    if kind == 1 {
      score -= 5;
    } else {
      score += 5;
    }
  }
  score
}

pub fn select_melee_singles_event_slug(
  tournament_slug: &str,
  events: &[StartggTournamentEventNode],
) -> Option<String> {
  let mut candidates: Vec<(i32, String)> = events
    .iter()
    .filter_map(|event| {
      let slug = event.slug.as_ref()?.trim();
      if slug.is_empty() {
        return None;
      }
      Some((event_score(event), slug.to_string()))
    })
    .collect();
  candidates.sort_by(|a, b| a.0.cmp(&b.0));
  candidates
    .first()
    .and_then(|(_, slug)| normalize_event_slug(tournament_slug, slug))
}

pub fn normalize_event_slug(tournament_slug: &str, raw_slug: &str) -> Option<String> {
  let trimmed = raw_slug.trim().trim_start_matches('/');
  if trimmed.is_empty() {
    return None;
  }
  if trimmed.starts_with("tournament/") && trimmed.contains("/event/") {
    return Some(trimmed.to_string());
  }
  if trimmed.starts_with("event/") {
    return Some(format!("tournament/{}/{}", tournament_slug, trimmed));
  }
  if trimmed.contains("/event/") {
    return Some(trimmed.to_string());
  }
  Some(format!("tournament/{}/event/{}", tournament_slug, trimmed))
}

pub fn resolve_startgg_event_slug(
  config: &AppConfig,
  live_state: &SharedLiveStartgg,
) -> Result<String, String> {
  let link = config.startgg_link.trim();
  if link.is_empty() {
    return Err("Start.gg link is empty.".to_string());
  }
  let info = parse_startgg_link_info(link);
  if let Ok(guard) = live_state.lock() {
    if guard.startgg_link.as_deref() == Some(link) {
      if let Some(event_slug) = guard.event_slug.as_ref() {
        return Ok(event_slug.clone());
      }
    }
  }
  if let Some(tournament_slug) = info.tournament_slug.as_ref() {
    let events = fetch_startgg_tournament_events(config, tournament_slug)?;
    if let Some(event_slug) = select_melee_singles_event_slug(tournament_slug, &events) {
      return Ok(event_slug);
    }
    return Err(format!(
      "No Melee Singles event found for tournament {tournament_slug}."
    ));
  }
  if let Some(event_slug) = info.event_slug.as_ref() {
    return Ok(event_slug.clone());
  }
  Err("Start.gg link must include a tournament slug.".to_string())
}

pub fn value_to_i64(value: &Value) -> Option<i64> {
  match value {
    Value::Number(num) => num.as_i64(),
    Value::String(raw) => raw.parse::<i64>().ok(),
    _ => None,
  }
}

pub fn value_to_u32(value: &Value) -> Option<u32> {
  value_to_i64(value).and_then(|num| u32::try_from(num).ok())
}

pub fn value_to_u64(value: &Value) -> Option<u64> {
  value_to_i64(value).and_then(|num| u64::try_from(num).ok())
}

pub fn value_to_string(value: &Value) -> Option<String> {
  match value {
    Value::String(raw) => Some(raw.clone()),
    Value::Number(num) => Some(num.to_string()),
    _ => None,
  }
}

pub fn parse_time_ms(value: Option<i64>) -> Option<u64> {
  let value = value?;
  if value > 1_000_000_000_000 {
    Some(value as u64)
  } else if value > 0 {
    Some((value as u64) * 1000)
  } else {
    None
  }
}

pub fn map_startgg_set_state(value: Option<&Value>) -> String {
  if let Some(raw) = value {
    if let Some(text) = raw.as_str() {
      let lower = text.to_lowercase();
      if lower.contains("progress") {
        return "inProgress".to_string();
      }
      if lower.contains("complete") {
        return "completed".to_string();
      }
      if lower.contains("skip") {
        return "skipped".to_string();
      }
      return "pending".to_string();
    }
    if let Some(num) = value_to_i64(raw) {
      return match num {
        2 => "inProgress",
        3 => "completed",
        4 => "skipped",
        6 => "skipped",
        _ => "pending",
      }
      .to_string();
    }
  }
  "pending".to_string()
}

pub fn resolve_live_round_label(full_round_text: Option<&String>, round: i32) -> String {
  if let Some(text) = full_round_text {
    if !text.trim().is_empty() {
      return text.clone();
    }
  }
  if round > 0 {
    return format!("Winners Round {round}");
  }
  if round < 0 {
    return format!("Losers Round {}", round.abs());
  }
  "Grand Finals".to_string()
}

pub fn extract_slippi_code(entrant: &StartggEntrantNode) -> Option<String> {
  if let Some(code) = entrant.slippi_code.as_ref().map(|c| c.trim()).filter(|c| !c.is_empty()) {
    return Some(code.to_string());
  }
  for field in entrant.custom_fields.as_ref().into_iter().flatten() {
    let name = field.name.as_deref().unwrap_or("").to_lowercase();
    if name.contains("slippi") || name.contains("connect") {
      if let Some(value) = field.value.as_ref().map(|v| v.trim()).filter(|v| !v.is_empty()) {
        return Some(value.to_string());
      }
    }
  }
  for participant in entrant.participants.as_ref().into_iter().flatten() {
    if let Some(player) = &participant.player {
      if let Some(code) = player.slippi_code.as_ref().map(|c| c.trim()).filter(|c| !c.is_empty()) {
        return Some(code.to_string());
      }
    }
    if let Some(user) = &participant.user {
      for auth in user.authorizations.as_ref().into_iter().flatten() {
        let auth_type = auth.kind.as_deref().unwrap_or("").to_lowercase();
        if auth_type.contains("slippi") || auth_type.contains("connect") {
          if let Some(code) = auth
            .external_username
            .as_ref()
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
          {
            return Some(code.to_string());
          }
        }
      }
    }
    let tags = [
      participant.gamer_tag.as_deref(),
      participant.player.as_ref().and_then(|p| p.gamer_tag.as_deref()),
    ];
    for tag in tags {
      if let Some(tag) = tag {
        if tag.contains('#') {
          return Some(tag.to_string());
        }
      }
    }
  }
  None
}

pub fn build_live_startgg_state(
  event: StartggEventInfoNode,
  entrants_raw: Vec<StartggEntrantNode>,
  sets_raw: Vec<StartggSetNode>,
  event_link: Option<String>,
) -> StartggSimState {
  let now_ms = now_ms();
  let event_id = event
    .id
    .as_ref()
    .and_then(value_to_string)
    .unwrap_or_else(|| "event".to_string());
  let event_name = event.name.unwrap_or_else(|| "Start.gg Event".to_string());
  let event_slug = event.slug.unwrap_or_else(|| "event".to_string());

  let mut phases = Vec::new();
  if let Some(raw_phases) = event.phases {
    for (idx, phase) in raw_phases.into_iter().enumerate() {
      let id = phase
        .id
        .as_ref()
        .and_then(value_to_string)
        .unwrap_or_else(|| format!("phase-{}", idx + 1));
      let name = phase.name.unwrap_or_else(|| format!("Phase {}", idx + 1));
      phases.push(StartggSimPhaseConfig { id, name, best_of: 3 });
    }
  }
  if phases.is_empty() {
    phases.push(StartggSimPhaseConfig {
      id: "phase-1".to_string(),
      name: "Bracket".to_string(),
      best_of: 3,
    });
  }
  let phase_lookup: HashMap<String, StartggSimPhaseConfig> =
    phases.iter().map(|phase| (phase.id.clone(), phase.clone())).collect();

  let mut entrants = Vec::new();
  for (idx, entrant) in entrants_raw.iter().enumerate() {
    let id = entrant
      .id
      .as_ref()
      .and_then(value_to_u32)
      .unwrap_or((idx + 1) as u32);
    let name = entrant
      .name
      .clone()
      .or_else(|| entrant.participants.as_ref().and_then(|p| p.first()).and_then(|p| p.gamer_tag.clone()))
      .unwrap_or_else(|| format!("Entrant {id}"));
    let seed = entrant
      .seeds
      .as_ref()
      .and_then(|seeds| seeds.first().and_then(|seed| seed.seed_num))
      .or(entrant.seed)
      .unwrap_or((idx + 1) as i32)
      .max(1) as u32;
    let slippi_code = extract_slippi_code(entrant).unwrap_or_default();
    entrants.push(StartggSimEntrant { id, name, seed, slippi_code });
  }

  let entrants_by_id: HashMap<u32, StartggSimEntrant> =
    entrants.iter().map(|entrant| (entrant.id, entrant.clone())).collect();

  let mut sets = Vec::new();
  for (idx, set) in sets_raw.iter().enumerate() {
    let id = set
      .id
      .as_ref()
      .and_then(value_to_u64)
      .unwrap_or((idx + 1) as u64);
    let round = set.round.unwrap_or(0);
    let round_label = resolve_live_round_label(set.full_round_text.as_ref(), round);
    let state = map_startgg_set_state(set.state.as_ref());
    let winner_id = set.winner_id.as_ref().and_then(value_to_u32);
    let started_at_ms = parse_time_ms(set.started_at);
    let completed_at_ms = parse_time_ms(set.completed_at);
    let updated_at_ms = parse_time_ms(set.updated_at).unwrap_or(now_ms);
    let (phase_id, phase_name) = set
      .phase_group
      .as_ref()
      .and_then(|group| group.phase.as_ref())
      .and_then(|phase| {
        let id = phase.id.as_ref().and_then(value_to_string);
        let name = phase.name.clone();
        match (id, name) {
          (Some(id), Some(name)) => Some((id, name)),
          _ => None,
        }
      })
      .or_else(|| phases.first().map(|phase| (phase.id.clone(), phase.name.clone())))
      .unwrap_or_else(|| ("phase-1".to_string(), "Bracket".to_string()));
    let best_of = phase_lookup
      .get(&phase_id)
      .map(|phase| phase.best_of)
      .unwrap_or(3);

    let slots = set
      .slots
      .as_ref()
      .map(|raw_slots| {
        raw_slots
          .iter()
          .map(|slot| {
            let entrant_id = slot
              .entrant
              .as_ref()
              .and_then(|entrant| entrant.id.as_ref().and_then(value_to_u32));
            let entrant = entrant_id.and_then(|id| entrants_by_id.get(&id));
            let entrant_name = entrant
              .map(|e| e.name.clone())
              .or_else(|| slot.entrant.as_ref().and_then(|ent| ent.name.clone()));
            let slippi_code = entrant
              .map(|e| e.slippi_code.clone())
              .filter(|code| !code.trim().is_empty());
            let seed = entrant.map(|e| e.seed);
            let score_value = slot
              .standing
              .as_ref()
              .and_then(|standing| standing.stats.as_ref())
              .and_then(|stats| stats.score.as_ref())
              .and_then(|score| score.value);
            let score = score_value.and_then(|value| {
              if value < 0.0 {
                None
              } else {
                Some(value.round().clamp(0.0, 9.0) as u8)
              }
            });
            let label = slot
              .standing
              .as_ref()
              .and_then(|standing| standing.stats.as_ref())
              .and_then(|stats| stats.score.as_ref())
              .and_then(|score| score.label.as_ref())
              .map(|label| label.to_lowercase());
            let mut result = None;
            if label.as_deref().map(|l| l.contains("dq")).unwrap_or(false) {
              result = Some("dq".to_string());
            } else if let (Some(winner), Some(entrant_id)) = (winner_id, entrant_id) {
              result = Some(if winner == entrant_id { "win" } else { "loss" }.to_string());
            } else if state == "completed" && entrant_id.is_some() {
              result = Some("loss".to_string());
            }

            StartggSimSlot {
              entrant_id,
              entrant_name,
              slippi_code,
              seed,
              score,
              result,
              source_type: None,
              source_set_id: None,
              source_label: None,
            }
          })
          .collect::<Vec<_>>()
      })
      .unwrap_or_else(Vec::new);

    sets.push(StartggSimSet {
      id,
      phase_id,
      phase_name,
      round,
      round_label,
      best_of,
      state,
      started_at_ms,
      completed_at_ms,
      updated_at_ms,
      winner_id,
      slots,
    });
  }

  StartggSimState {
    event: StartggSimEventConfig {
      id: event_id,
      name: event_name,
      slug: event_slug,
    },
    phases,
    entrants,
    sets,
    started_at_ms: now_ms,
    now_ms,
    reference_tournament_link: event_link,
  }
}

pub fn fetch_live_startgg_state(
  config: &AppConfig,
  event_slug: &str,
) -> Result<StartggSimState, String> {
  let event = fetch_startgg_event_info(config, event_slug)?;
  let entrants = fetch_startgg_entrants(config, event_slug)?;
  let sets = fetch_startgg_sets(config, event_slug)?;
  let event_link = format!("https://start.gg/{}", event_slug.trim_start_matches('/'));
  Ok(build_live_startgg_state(
    event,
    entrants,
    sets,
    Some(event_link),
  ))
}

pub fn maybe_refresh_live_startgg(
  config: &AppConfig,
  live_state: &SharedLiveStartgg,
  force: bool,
) -> Option<StartggSimState> {
  if config.test_mode {
    return None;
  }
  let link = config.startgg_link.trim();
  if link.is_empty() {
    return None;
  }
  let (should_fetch, cached_state, cached_link, cached_slug, fetch_in_flight, last_fetch) = {
    let guard = live_state.lock().unwrap_or_else(|e| e.into_inner());
    (
      guard.state.is_none(),
      guard.state.clone(),
      guard.startgg_link.clone(),
      guard.event_slug.clone(),
      guard.fetch_in_flight,
      guard.last_fetch,
    )
  };

  let resolved_slug = match resolve_startgg_event_slug(config, live_state) {
    Ok(slug) => slug,
    Err(err) => {
      let mut guard = live_state.lock().unwrap_or_else(|e| e.into_inner());
      guard.last_error = Some(err);
      return cached_state;
    }
  };

  let mut needs_refresh = force || should_fetch;
  if cached_link.as_deref() != Some(link) {
    needs_refresh = true;
  }
  if cached_slug.as_deref() != Some(&resolved_slug) {
    needs_refresh = true;
  }
  if !config.startgg_polling {
    if let Some(last) = last_fetch {
      if last.elapsed().map(|age| age.as_millis() as u64).unwrap_or(u64::MAX) > STARTGG_IDLE_REFRESH_MS {
        needs_refresh = true;
      }
    } else {
      needs_refresh = true;
    }
  }

  if !needs_refresh || fetch_in_flight {
    return cached_state;
  }

  {
    let mut guard = live_state.lock().unwrap_or_else(|e| e.into_inner());
    guard.fetch_in_flight = true;
  }

  let result = fetch_live_startgg_state(config, &resolved_slug);
  let mut guard = live_state.lock().unwrap_or_else(|e| e.into_inner());
  guard.fetch_in_flight = false;
  guard.startgg_link = Some(link.to_string());
  guard.event_slug = Some(resolved_slug.clone());
  match result {
    Ok(state) => {
      guard.last_fetch = Some(SystemTime::now());
      guard.last_error = None;
      guard.state = Some(state.clone());
      Some(state)
    }
    Err(err) => {
      guard.last_error = Some(err);
      cached_state
    }
  }
}

pub fn spawn_startgg_polling(live_state: SharedLiveStartgg) {
  std::thread::spawn(move || loop {
    let config = load_config_inner().unwrap_or_else(|_| AppConfig::default());
    if config.test_mode || !config.startgg_polling {
      sleep(Duration::from_millis(STARTGG_POLL_INTERVAL_MS));
      continue;
    }
    if config.startgg_link.trim().is_empty() {
      sleep(Duration::from_millis(STARTGG_POLL_INTERVAL_MS));
      continue;
    }
    maybe_refresh_live_startgg(&config, &live_state, true);
    sleep(Duration::from_millis(STARTGG_POLL_INTERVAL_MS));
  });
}

pub fn build_default_startgg_sim_config() -> Result<StartggSimConfig, String> {
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

pub fn load_startgg_sim_config() -> Result<StartggSimConfig, String> {
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

pub fn load_startgg_sim_config_from(path: &Path) -> Result<StartggSimConfig, String> {
  if !path.is_file() {
    return Err(format!("Start.gg sim config not found at {}.", path.display()));
  }
  let data = fs::read_to_string(path)
    .map_err(|e| format!("read startgg sim config {}: {e}", path.display()))?;
  serde_json::from_str::<StartggSimConfig>(&data)
    .map_err(|e| format!("parse startgg sim config {}: {e}", path.display()))
}

pub fn init_startgg_sim(guard: &mut TestModeState, now: u64) -> Result<(), String> {
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

pub fn build_bracket_replay_map(config_path: &Path) -> HashMap<u64, PathBuf> {
  let mut out = HashMap::new();
  if !config_path.is_file() {
    return out;
  }
  let data = match fs::read_to_string(config_path) {
    Ok(data) => data,
    Err(_) => return out,
  };
  let value: Value = match serde_json::from_str(&data) {
    Ok(value) => value,
    Err(_) => return out,
  };
  let replay_map = match value.get("referenceReplayMap") {
    Some(map) => map,
    None => return out,
  };
  let base_dir = replay_map
    .get("replaysDir")
    .and_then(|v| v.as_str())
    .map(resolve_repo_path);
  let sets = match replay_map.get("sets").and_then(|sets| sets.as_array()) {
    Some(sets) => sets,
    None => return out,
  };

  for set in sets {
    let id = set.get("id").and_then(|v| v.as_u64());
    let replays = set.get("replays").and_then(|v| v.as_array());
    let (Some(id), Some(replays)) = (id, replays) else {
      continue;
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
      if path.is_file() {
        out.entry(id).or_insert(path);
        break;
      }
    }
  }

  out
}

pub fn read_bracket_set_replay_paths(config_path: &str, set_id: u64) -> Result<Vec<PathBuf>, String> {
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
