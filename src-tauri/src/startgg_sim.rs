use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSimEventConfig {
  pub id: String,
  pub name: String,
  pub slug: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSimPhaseConfig {
  pub id: String,
  pub name: String,
  pub best_of: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSimEntrantConfig {
  pub id: u32,
  pub name: String,
  pub slippi_code: String,
  pub seed: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StartggSimSimulationConfig {
  pub time_scale: f64,
  pub min_set_duration_sec: u32,
  pub max_set_duration_sec: u32,
  pub max_concurrent_sets: u32,
  pub seed: u64,
  pub allow_grand_finals_reset: bool,
  pub manual_mode: bool,
}

impl Default for StartggSimSimulationConfig {
  fn default() -> Self {
    StartggSimSimulationConfig {
      time_scale: 1.0,
      min_set_duration_sec: 300,
      max_set_duration_sec: 540,
      max_concurrent_sets: 2,
      seed: 1337,
      allow_grand_finals_reset: true,
      manual_mode: true,
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartggReferenceScore {
  pub value: Option<i32>,
  pub label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartggReferenceStats {
  #[serde(default)]
  pub score: Option<StartggReferenceScore>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartggReferenceStanding {
  #[serde(default)]
  pub stats: Option<StartggReferenceStats>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartggReferenceEntrant {
  pub id: Option<u32>,
  pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartggReferenceSlot {
  #[serde(default)]
  pub entrant: Option<StartggReferenceEntrant>,
  #[serde(default)]
  pub standing: Option<StartggReferenceStanding>,
  #[serde(default)]
  pub prereq_id: Option<u64>,
  #[serde(default)]
  pub prereq_type: Option<String>,
  #[serde(default)]
  pub prereq_placement: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartggReferenceSet {
  pub id: Option<u64>,
  pub round: Option<i32>,
  pub full_round_text: Option<String>,
  pub state: Option<i32>,
  pub winner_id: Option<u32>,
  #[serde(default)]
  pub slots: Vec<StartggReferenceSlot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSimConfig {
  pub event: StartggSimEventConfig,
  pub phases: Vec<StartggSimPhaseConfig>,
  pub entrants: Vec<StartggSimEntrantConfig>,
  #[serde(default)]
  pub simulation: StartggSimSimulationConfig,
  pub reference_tournament_link: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub reference_sets: Vec<StartggReferenceSet>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSimEntrant {
  pub id: u32,
  pub name: String,
  pub seed: u32,
  pub slippi_code: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSimSlot {
  pub entrant_id: Option<u32>,
  pub entrant_name: Option<String>,
  pub slippi_code: Option<String>,
  pub seed: Option<u32>,
  pub score: Option<u8>,
  pub result: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSimSet {
  pub id: u64,
  pub phase_id: String,
  pub phase_name: String,
  pub round: i32,
  pub round_label: String,
  pub best_of: u8,
  pub state: String,
  pub started_at_ms: Option<u64>,
  pub completed_at_ms: Option<u64>,
  pub updated_at_ms: u64,
  pub winner_id: Option<u32>,
  pub slots: Vec<StartggSimSlot>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartggSimState {
  pub event: StartggSimEventConfig,
  pub phases: Vec<StartggSimPhaseConfig>,
  pub entrants: Vec<StartggSimEntrant>,
  pub sets: Vec<StartggSimSet>,
  pub started_at_ms: u64,
  pub now_ms: u64,
  pub reference_tournament_link: Option<String>,
}

#[derive(Clone, Debug)]
struct SimEntrant {
  id: u32,
  name: String,
  slippi_code: String,
  seed: u32,
}

#[derive(Clone, Copy, Debug)]
enum SlotSource {
  Entrant(u32),
  Winner(u64),
  Loser(u64),
  Empty,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SimSetState {
  Pending,
  InProgress,
  Completed,
  Skipped,
}

#[derive(Clone, Copy, Debug)]
enum SlotResult {
  Win,
  Loss,
  Dq,
}

#[derive(Clone, Copy, Debug)]
enum SimSetCondition {
  GrandFinalReset { gf1_id: u64, losers_slot_index: usize },
}

#[derive(Clone, Debug)]
struct SimSlot {
  source: SlotSource,
  entrant_id: Option<u32>,
  score: Option<u8>,
  result: Option<SlotResult>,
}

#[derive(Clone, Debug)]
struct SimSet {
  id: u64,
  phase_id: String,
  round: i32,
  round_label: String,
  best_of: u8,
  slots: [SimSlot; 2],
  state: SimSetState,
  started_at_ms: Option<u64>,
  completed_at_ms: Option<u64>,
  updated_at_ms: u64,
  winner_slot: Option<usize>,
  loser_slot: Option<usize>,
  condition: Option<SimSetCondition>,
  sort_order: u64,
  end_at_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
enum SlotResolution {
  Ready(u32),
  Pending,
  Empty,
}

#[derive(Clone, Copy, Debug)]
enum SetOutcomeKind {
  Finish { winner_slot: usize, scores: [u8; 2] },
  Dq { dq_slot: usize },
}

#[derive(Clone, Copy, Debug)]
struct SetOutcome {
  id: u64,
  sort_order: u64,
  kind: SetOutcomeKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RoundKind {
  Winners,
  Losers,
  GrandFinal,
  Unknown,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedOutcome {
  set_id: u64,
  winner_slot: usize,
  scores: [u8; 2],
  dq_slot: Option<usize>,
}

#[derive(Clone, Debug)]
struct ReferenceOutcome {
  id: Option<u64>,
  entrants: [Option<u32>; 2],
  scores: [Option<u8>; 2],
  winner_id: Option<u32>,
  round_kind: RoundKind,
  gf_reset: bool,
  dq_slot: Option<usize>,
}

#[derive(Clone, Debug)]
struct SimRng {
  state: u64,
}

impl SimRng {
  fn new(seed: u64) -> Self {
    let mut state = seed;
    if state == 0 {
      state = 0x9E37_79B9_7F4A_7C15;
    }
    SimRng { state }
  }

  fn next_u64(&mut self) -> u64 {
    let mut x = self.state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    self.state = x;
    x
  }

  fn next_f64(&mut self) -> f64 {
    let v = self.next_u64() >> 11;
    (v as f64) / ((1u64 << 53) as f64)
  }

  fn gen_range_u32(&mut self, min: u32, max_inclusive: u32) -> u32 {
    if max_inclusive <= min {
      return min;
    }
    let span = (max_inclusive - min + 1) as u64;
    min + (self.next_u64() % span) as u32
  }
}

pub struct StartggSim {
  config: StartggSimConfig,
  entrants: Vec<SimEntrant>,
  entrants_by_id: HashMap<u32, SimEntrant>,
  sets: Vec<SimSet>,
  set_index: HashMap<u64, usize>,
  started_at_ms: u64,
  rng: SimRng,
}

impl StartggSim {
  pub fn new(config: StartggSimConfig, now_ms: u64) -> Result<Self, String> {
    if config.phases.is_empty() {
      return Err("Start.gg sim config needs at least one phase.".to_string());
    }
    let entrants = normalize_entrants(&config.entrants)?;
    if entrants.len() < 2 {
      return Err("Start.gg sim config needs at least two entrants.".to_string());
    }
    let entrants_by_id = entrants
      .iter()
      .cloned()
      .map(|e| (e.id, e))
      .collect::<HashMap<_, _>>();

    let (sets, set_index) = if config.reference_sets.is_empty() {
      build_double_elim_sets(
        &entrants,
        &config.phases[0],
        config.simulation.allow_grand_finals_reset,
      )?
    } else {
      build_reference_sets(&entrants, &config.phases[0], &config.reference_sets)?
    };

    let sim_seed = config.simulation.seed;
    Ok(StartggSim {
      config,
      entrants,
      entrants_by_id,
      sets,
      set_index,
      started_at_ms: now_ms,
      rng: SimRng::new(sim_seed),
    })
  }

  pub fn has_reference_sets(&self) -> bool {
    !self.config.reference_sets.is_empty()
  }

  pub fn state(&mut self, now_ms: u64) -> StartggSimState {
    self.state_since(now_ms, None)
  }

  pub fn state_since(&mut self, now_ms: u64, since_ms: Option<u64>) -> StartggSimState {
    self.advance(now_ms);
    let mut snapshot = self.snapshot(now_ms);
    if let Some(since) = since_ms {
      if since > 0 {
        snapshot.sets.retain(|set| set.updated_at_ms > since);
        snapshot.entrants = Vec::new();
      }
    }
    snapshot
  }

  pub fn raw_response(&mut self, now_ms: u64, since_ms: Option<u64>) -> Value {
    let state = self.state_since(now_ms, since_ms);
    startgg_state_to_raw(&state, now_ms)
  }

  fn advance(&mut self, now_ms: u64) {
    let manual_mode = self.config.simulation.manual_mode;
    if !manual_mode {
      let mut to_complete = Vec::new();
      for (idx, set) in self.sets.iter().enumerate() {
        if set.state == SimSetState::InProgress {
          if let Some(end_at) = set.end_at_ms {
            if end_at <= now_ms {
              to_complete.push(idx);
            }
          }
        }
      }
      for idx in to_complete {
        self.complete_set(idx, now_ms);
      }
    }

    let mut safety = 0;
    loop {
      safety += 1;
      if safety > 1000 {
        break;
      }
      let mut progressed = false;

      for idx in 0..self.sets.len() {
        let state = self.sets[idx].state;
        if state != SimSetState::Pending {
          continue;
        }
        if self.apply_condition(idx, now_ms) {
          progressed = true;
          continue;
        }

        let (res_a, res_b) = {
          let set = &self.sets[idx];
          (self.resolve_slot(set.slots[0].source), self.resolve_slot(set.slots[1].source))
        };

        if self.apply_resolutions(idx, res_a, res_b, now_ms) {
          progressed = true;
        }

        if self.auto_advance_if_bye(idx, res_a, res_b, now_ms) {
          progressed = true;
        }
      }

      if !progressed {
        break;
      }
    }

    if manual_mode {
      return;
    }

    let ready_sets = self.ready_set_ids();
    if ready_sets.is_empty() {
      return;
    }
    let in_progress = self
      .sets
      .iter()
      .filter(|s| s.state == SimSetState::InProgress)
      .count() as u32;
    let max_concurrent = self.config.simulation.max_concurrent_sets.max(1);
    let available = max_concurrent.saturating_sub(in_progress);
    for set_id in ready_sets.into_iter().take(available as usize) {
      if let Some(index) = self.set_index.get(&set_id).copied() {
        self.start_set(index, now_ms);
      }
    }
  }

  fn apply_condition(&mut self, set_index: usize, now_ms: u64) -> bool {
    let condition = match self.sets[set_index].condition {
      Some(cond) => cond,
      None => return false,
    };
    match condition {
      SimSetCondition::GrandFinalReset { gf1_id, losers_slot_index } => {
        let Some(gf1_index) = self.set_index.get(&gf1_id).copied() else {
          self.sets[set_index].condition = None;
          return false;
        };
        let gf1 = &self.sets[gf1_index];
        if gf1.state != SimSetState::Completed {
          return false;
        }
        if gf1.winner_slot == Some(losers_slot_index) {
          self.sets[set_index].condition = None;
          return false;
        }
        let set = &mut self.sets[set_index];
        set.state = SimSetState::Skipped;
        set.started_at_ms = Some(now_ms);
        set.completed_at_ms = Some(now_ms);
        set.updated_at_ms = now_ms;
        set.condition = None;
        true
      }
    }
  }

  fn apply_resolutions(&mut self, set_index: usize, res_a: SlotResolution, res_b: SlotResolution, now_ms: u64) -> bool {
    let set = &mut self.sets[set_index];
    let mut changed = false;
    if set.state != SimSetState::Pending {
      return false;
    }
    changed |= apply_slot_resolution(&mut set.slots[0], res_a);
    changed |= apply_slot_resolution(&mut set.slots[1], res_b);
    if changed {
      set.updated_at_ms = now_ms;
    }
    changed
  }

  fn auto_advance_if_bye(
    &mut self,
    set_index: usize,
    res_a: SlotResolution,
    res_b: SlotResolution,
    now_ms: u64,
  ) -> bool {
    let set = &mut self.sets[set_index];
    if set.state != SimSetState::Pending {
      return false;
    }
    let games_to_win = games_to_win(set.best_of);
    match (res_a, res_b) {
      (SlotResolution::Ready(_), SlotResolution::Empty) => {
        finalize_bye_set(set, 0, games_to_win, now_ms);
        true
      }
      (SlotResolution::Empty, SlotResolution::Ready(_)) => {
        finalize_bye_set(set, 1, games_to_win, now_ms);
        true
      }
      (SlotResolution::Empty, SlotResolution::Empty) => {
        set.state = SimSetState::Skipped;
        set.started_at_ms = Some(now_ms);
        set.completed_at_ms = Some(now_ms);
        set.updated_at_ms = now_ms;
        true
      }
      _ => false,
    }
  }

  fn ready_set_ids(&self) -> Vec<u64> {
    let mut ids = Vec::new();
    for set in &self.sets {
      if set.state != SimSetState::Pending {
        continue;
      }
      if set.condition.is_some() {
        continue;
      }
      if set.slots.iter().all(|slot| slot.entrant_id.is_some()) {
        ids.push(set.id);
      }
    }
    ids.sort_by_key(|id| self.set_index.get(id).map(|idx| self.sets[*idx].sort_order).unwrap_or(*id));
    ids
  }

  fn start_set(&mut self, set_index: usize, now_ms: u64) {
    let duration = self.sample_duration_ms();
    let set = &mut self.sets[set_index];
    if set.state != SimSetState::Pending {
      return;
    }
    set.state = SimSetState::InProgress;
    set.started_at_ms = Some(now_ms);
    set.end_at_ms = Some(now_ms + duration);
    set.updated_at_ms = now_ms;
  }

  fn complete_set(&mut self, set_index: usize, now_ms: u64) {
    let (a_id, b_id, best_of) = {
      let set = &self.sets[set_index];
      if set.state != SimSetState::InProgress {
        return;
      }
      (set.slots[0].entrant_id, set.slots[1].entrant_id, set.best_of)
    };
    let Some(a_id) = a_id else {
      let set = &mut self.sets[set_index];
      set.state = SimSetState::Skipped;
      set.completed_at_ms = Some(now_ms);
      set.updated_at_ms = now_ms;
      return;
    };
    let Some(b_id) = b_id else {
      let set = &mut self.sets[set_index];
      set.state = SimSetState::Skipped;
      set.completed_at_ms = Some(now_ms);
      set.updated_at_ms = now_ms;
      return;
    };

    let winner_slot = self.pick_winner(a_id, b_id);
    let loser_slot = if winner_slot == 0 { 1 } else { 0 };
    let games_to_win = games_to_win(best_of);
    let loser_score = if games_to_win > 0 {
      self.rng.gen_range_u32(0, games_to_win as u32 - 1)
    } else {
      0
    };
    let set = &mut self.sets[set_index];
    set.slots[winner_slot].score = Some(games_to_win);
    set.slots[winner_slot].result = Some(SlotResult::Win);
    set.slots[loser_slot].score = Some(loser_score as u8);
    set.slots[loser_slot].result = Some(SlotResult::Loss);
    set.winner_slot = Some(winner_slot);
    set.loser_slot = Some(loser_slot);
    set.completed_at_ms = Some(now_ms);
    set.state = SimSetState::Completed;
    set.updated_at_ms = now_ms;
  }

  pub fn advance_set(&mut self, set_id: u64, now_ms: u64) -> Result<(), String> {
    let index = self
      .set_index
      .get(&set_id)
      .copied()
      .ok_or_else(|| "Set not found.".to_string())?;
    let state = self.sets[index].state;
    match state {
      SimSetState::Pending => {
        if self.sets[index].slots.iter().any(|slot| slot.entrant_id.is_none()) {
          return Err("Set is missing entrants.".to_string());
        }
        self.start_set(index, now_ms);
        Ok(())
      }
      SimSetState::InProgress => {
        self.complete_set(index, now_ms);
        Ok(())
      }
      SimSetState::Completed | SimSetState::Skipped => {
        Err("Set is already completed.".to_string())
      }
    }
  }

  pub fn start_set_manual(&mut self, set_id: u64, now_ms: u64) -> Result<(), String> {
    let index = self
      .set_index
      .get(&set_id)
      .copied()
      .ok_or_else(|| "Set not found.".to_string())?;
    let set = &mut self.sets[index];
    if set.state != SimSetState::Pending {
      return Err("Set has already started.".to_string());
    }
    if set.slots.iter().any(|slot| slot.entrant_id.is_none()) {
      return Err("Set is missing entrants.".to_string());
    }
    set.state = SimSetState::InProgress;
    set.started_at_ms = Some(now_ms);
    set.end_at_ms = None;
    set.updated_at_ms = now_ms;
    Ok(())
  }

  pub fn finish_set_manual(
    &mut self,
    set_id: u64,
    winner_slot: usize,
    scores: [u8; 2],
    now_ms: u64,
  ) -> Result<(), String> {
    if winner_slot > 1 {
      return Err("Winner slot must be 0 or 1.".to_string());
    }
    let index = self
      .set_index
      .get(&set_id)
      .copied()
      .ok_or_else(|| "Set not found.".to_string())?;
    let set = &mut self.sets[index];
    if matches!(set.state, SimSetState::Completed | SimSetState::Skipped) {
      return Err("Set is already completed.".to_string());
    }
    let present_slots = set
      .slots
      .iter()
      .enumerate()
      .filter_map(|(idx, slot)| slot.entrant_id.map(|_| idx))
      .collect::<Vec<_>>();
    if present_slots.len() < 2 {
      if present_slots.is_empty() {
        set.state = SimSetState::Skipped;
        set.started_at_ms = Some(now_ms);
        set.completed_at_ms = Some(now_ms);
        set.updated_at_ms = now_ms;
        set.end_at_ms = None;
        set.winner_slot = None;
        set.loser_slot = None;
        return Ok(());
      }
      let games_to_win = games_to_win(set.best_of);
      finalize_bye_set(set, present_slots[0], games_to_win, now_ms);
      set.end_at_ms = None;
      return Ok(());
    }
    let loser_slot = if winner_slot == 0 { 1 } else { 0 };
    set.state = SimSetState::Completed;
    set.started_at_ms = Some(set.started_at_ms.unwrap_or(now_ms));
    set.completed_at_ms = Some(now_ms);
    set.updated_at_ms = now_ms;
    set.winner_slot = Some(winner_slot);
    set.loser_slot = Some(loser_slot);
    set.end_at_ms = None;

    set.slots[winner_slot].score = Some(scores[winner_slot]);
    set.slots[loser_slot].score = Some(scores[loser_slot]);
    set.slots[winner_slot].result = Some(SlotResult::Win);
    set.slots[loser_slot].result = Some(SlotResult::Loss);
    Ok(())
  }

  pub fn force_winner(&mut self, set_id: u64, winner_slot: usize, now_ms: u64) -> Result<(), String> {
    if winner_slot > 1 {
      return Err("Winner slot must be 0 or 1.".to_string());
    }
    let index = self
      .set_index
      .get(&set_id)
      .copied()
      .ok_or_else(|| "Set not found.".to_string())?;
    let (winner_id, loser_id, best_of) = {
      let set = &self.sets[index];
      if matches!(set.state, SimSetState::Completed | SimSetState::Skipped) {
        return Err("Set is already completed.".to_string());
      }
      let winner_id = set.slots[winner_slot]
        .entrant_id
        .ok_or_else(|| "Selected winner slot has no entrant.".to_string())?;
      let loser_slot = if winner_slot == 0 { 1 } else { 0 };
      let loser_id = set.slots[loser_slot]
        .entrant_id
        .ok_or_else(|| "Opponent slot has no entrant.".to_string())?;
      (winner_id, loser_id, set.best_of)
    };

    let games_to_win = games_to_win(best_of);
    let set = &mut self.sets[index];
    let loser_slot = if winner_slot == 0 { 1 } else { 0 };
    set.state = SimSetState::Completed;
    set.started_at_ms = Some(set.started_at_ms.unwrap_or(now_ms));
    set.completed_at_ms = Some(now_ms);
    set.winner_slot = Some(winner_slot);
    set.loser_slot = Some(loser_slot);
    set.slots[winner_slot].entrant_id = Some(winner_id);
    set.slots[winner_slot].score = Some(games_to_win);
    set.slots[winner_slot].result = Some(SlotResult::Win);
    set.slots[loser_slot].entrant_id = Some(loser_id);
    set.slots[loser_slot].score = Some(0);
    set.slots[loser_slot].result = Some(SlotResult::Loss);
    set.updated_at_ms = now_ms;
    Ok(())
  }

  pub fn mark_dq(&mut self, set_id: u64, dq_slot: usize, now_ms: u64) -> Result<(), String> {
    if dq_slot > 1 {
      return Err("DQ slot must be 0 or 1.".to_string());
    }
    let index = self
      .set_index
      .get(&set_id)
      .copied()
      .ok_or_else(|| "Set not found.".to_string())?;
    let (winner_slot, best_of) = {
      let set = &self.sets[index];
      if matches!(set.state, SimSetState::Completed | SimSetState::Skipped) {
        return Err("Set is already completed.".to_string());
      }
      set.slots[dq_slot]
        .entrant_id
        .ok_or_else(|| "DQ slot has no entrant.".to_string())?;
      let winner_slot = if dq_slot == 0 { 1 } else { 0 };
      set.slots[winner_slot]
        .entrant_id
        .ok_or_else(|| "Opponent slot has no entrant.".to_string())?;
      (winner_slot, set.best_of)
    };

    let games_to_win = games_to_win(best_of);
    let loser_slot = if winner_slot == 0 { 1 } else { 0 };
    let set = &mut self.sets[index];
    set.state = SimSetState::Completed;
    set.started_at_ms = Some(set.started_at_ms.unwrap_or(now_ms));
    set.completed_at_ms = Some(now_ms);
    set.winner_slot = Some(winner_slot);
    set.loser_slot = Some(loser_slot);
    set.slots[winner_slot].score = Some(games_to_win);
    set.slots[winner_slot].result = Some(SlotResult::Win);
    set.slots[loser_slot].score = Some(0);
    set.slots[loser_slot].result = Some(SlotResult::Dq);
    set.updated_at_ms = now_ms;
    Ok(())
  }

  pub fn reset_set_and_dependents(&mut self, set_id: u64, now_ms: u64) -> Result<(), String> {
    if !self.set_index.contains_key(&set_id) {
      return Err("Set not found.".to_string());
    }
    let affected = self.collect_dependent_sets(set_id);
    let mut outcomes = self.collect_outcomes(&affected);
    outcomes.sort_by_key(|outcome| outcome.sort_order);

    let config = self.config.clone();
    let mut next = StartggSim::new(config, now_ms)?;
    next.advance(now_ms);

    for outcome in outcomes {
      match outcome.kind {
        SetOutcomeKind::Finish { winner_slot, scores } => {
          next.finish_set_manual(outcome.id, winner_slot, scores, now_ms)?;
        }
        SetOutcomeKind::Dq { dq_slot } => {
          next.mark_dq(outcome.id, dq_slot, now_ms)?;
        }
      }
      next.advance(now_ms);
    }

    *self = next;
    Ok(())
  }

  pub fn complete_all_sets(&mut self, now_ms: u64) -> Result<(), String> {
    let mut safety = 0;
    loop {
      safety += 1;
      if safety > 10_000 {
        return Err("Auto-complete exceeded safety limit.".to_string());
      }

      self.advance(now_ms);

      let mut next_id: Option<u64> = None;
      for set in &self.sets {
        if set.state == SimSetState::InProgress {
          next_id = Some(set.id);
          break;
        }
      }
      if next_id.is_none() {
        for set in &self.sets {
          if set.state != SimSetState::Pending {
            continue;
          }
          if set.slots.iter().any(|slot| slot.entrant_id.is_none()) {
            continue;
          }
          next_id = Some(set.id);
          break;
        }
      }

      let Some(set_id) = next_id else {
        break;
      };

      self.advance_set(set_id, now_ms)?;
    }
    Ok(())
  }

  pub fn complete_from_reference(&mut self, now_ms: u64) -> Result<(), String> {
    if self.config.reference_sets.is_empty() {
      return Err("No reference sets available in the config.".to_string());
    }

    let mut pending = self.build_reference_outcomes();
    if pending.is_empty() {
      return Err("No completed reference sets found to apply.".to_string());
    }

    let total = pending.len();
    let mut applied = 0usize;
    let mut safety = 0usize;

    loop {
      safety += 1;
      if safety > 10_000 {
        return Err("Applying reference sets exceeded safety limit.".to_string());
      }

      self.advance(now_ms);

      let mut progressed = false;
      let mut idx = 0usize;
      while idx < pending.len() {
        if let Some(resolved) = self.resolve_reference_outcome(&pending[idx]) {
          if let Some(dq_slot) = resolved.dq_slot {
            self.mark_dq(resolved.set_id, dq_slot, now_ms)?;
          } else {
            self.finish_set_manual(
              resolved.set_id,
              resolved.winner_slot,
              resolved.scores,
              now_ms,
            )?;
          }
          pending.swap_remove(idx);
          applied += 1;
          progressed = true;
        } else {
          idx += 1;
        }
      }

      if !progressed {
        break;
      }
    }

    if !pending.is_empty() {
      let examples = pending
        .iter()
        .filter_map(|outcome| outcome.id)
        .take(5)
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
      let hint = if examples.is_empty() {
        "".to_string()
      } else {
        format!(" Example ids: {examples}.")
      };
      return Err(format!(
        "Unable to apply {} of {} reference sets (applied {}).{hint}",
        pending.len(),
        total,
        applied
      ));
    }

    Ok(())
  }

  fn build_reference_outcomes(&self) -> Vec<ReferenceOutcome> {
    self
      .config
      .reference_sets
      .iter()
      .filter_map(|reference| {
        let winner_id = reference.winner_id;
        if winner_id.is_none() {
          return None;
        }
        let slot0 = reference.slots.get(0);
        let slot1 = reference.slots.get(1);
        let entrants = [
          slot0.and_then(|slot| slot.entrant.as_ref()?.id),
          slot1.and_then(|slot| slot.entrant.as_ref()?.id),
        ];
        let scores = [
          slot0.and_then(Self::slot_score),
          slot1.and_then(Self::slot_score),
        ];
        let raw_scores = [
          slot0.and_then(Self::slot_score_value),
          slot1.and_then(Self::slot_score_value),
        ];
        let dq_slot = raw_scores
          .iter()
          .position(|value| matches!(value, Some(score) if *score < 0));
        let (round_kind, gf_reset) = Self::reference_round_kind(reference);
        Some(ReferenceOutcome {
          id: reference.id,
          entrants,
          scores,
          winner_id,
          round_kind,
          gf_reset,
          dq_slot,
        })
      })
      .collect()
  }

  fn resolve_reference_outcome(&self, outcome: &ReferenceOutcome) -> Option<ResolvedOutcome> {
    if let Some(set_id) = outcome.id {
      if let Some(set) = self.get_set(set_id) {
        if matches!(set.state, SimSetState::Completed | SimSetState::Skipped) {
          return None;
        }
        return Self::match_reference_to_set(outcome, set);
      }
    }
    for set in &self.sets {
      if matches!(set.state, SimSetState::Completed | SimSetState::Skipped) {
        continue;
      }
      let set_kind = Self::round_kind_for_label(&set.round_label);
      if !Self::round_kind_matches(outcome, set_kind, &set.round_label) {
        continue;
      }
      if let Some(resolved) = Self::match_reference_to_set(outcome, set) {
        return Some(resolved);
      }
    }
    None
  }

  fn match_reference_to_set(outcome: &ReferenceOutcome, set: &SimSet) -> Option<ResolvedOutcome> {
    let sim_ids = [set.slots[0].entrant_id, set.slots[1].entrant_id];
    if sim_ids.iter().any(|id| id.is_none()) {
      return None;
    }
    let ref_ids = outcome.entrants;
    if ref_ids.iter().any(|id| id.is_none()) {
      return None;
    }

    let direct = sim_ids[0] == ref_ids[0] && sim_ids[1] == ref_ids[1];
    let swapped = sim_ids[0] == ref_ids[1] && sim_ids[1] == ref_ids[0];
    if !direct && !swapped {
      return None;
    }

    let winner_id = outcome.winner_id?;
    let winner_slot = if sim_ids[0] == Some(winner_id) {
      0
    } else if sim_ids[1] == Some(winner_id) {
      1
    } else {
      return None;
    };

    let score_a = outcome.scores[0].unwrap_or(0);
    let score_b = outcome.scores[1].unwrap_or(0);
    let scores = if direct {
      [score_a, score_b]
    } else {
      [score_b, score_a]
    };
    let dq_slot = outcome.dq_slot.map(|slot| if direct { slot } else { 1 - slot });

    Some(ResolvedOutcome {
      set_id: set.id,
      winner_slot,
      scores,
      dq_slot,
    })
  }

  fn slot_score_value(slot: &StartggReferenceSlot) -> Option<i32> {
    slot
      .standing
      .as_ref()?
      .stats
      .as_ref()?
      .score
      .as_ref()?
      .value
  }

  fn slot_score(slot: &StartggReferenceSlot) -> Option<u8> {
    let value = Self::slot_score_value(slot)?;
    let clamped = if value < 0 { 0 } else { value.min(u8::MAX as i32) };
    Some(clamped as u8)
  }

  fn reference_round_kind(reference: &StartggReferenceSet) -> (RoundKind, bool) {
    if let Some(text) = reference.full_round_text.as_ref() {
      let lower = text.to_lowercase();
      if lower.contains("grand final") {
        return (RoundKind::GrandFinal, lower.contains("reset"));
      }
      if lower.contains("losers") {
        return (RoundKind::Losers, false);
      }
      if lower.contains("winners") {
        return (RoundKind::Winners, false);
      }
    }

    match reference.round {
      Some(round) if round < 0 => (RoundKind::Losers, false),
      Some(round) if round > 0 => (RoundKind::Winners, false),
      _ => (RoundKind::Unknown, false),
    }
  }

  fn round_kind_for_label(label: &str) -> RoundKind {
    if label.starts_with('W') {
      RoundKind::Winners
    } else if label.starts_with('L') {
      RoundKind::Losers
    } else if label.starts_with("GF") {
      RoundKind::GrandFinal
    } else {
      RoundKind::Unknown
    }
  }

  fn round_kind_matches(outcome: &ReferenceOutcome, set_kind: RoundKind, label: &str) -> bool {
    if outcome.round_kind == RoundKind::Unknown {
      return true;
    }
    if outcome.round_kind != set_kind {
      return false;
    }
    if outcome.round_kind == RoundKind::GrandFinal {
      if outcome.gf_reset {
        return label == "GF2";
      }
      return label != "GF2";
    }
    true
  }

  fn collect_dependent_sets(&self, root_id: u64) -> HashSet<u64> {
    let mut dependents: HashMap<u64, Vec<u64>> = HashMap::new();
    for set in &self.sets {
      for slot in &set.slots {
        match slot.source {
          SlotSource::Winner(source_id) | SlotSource::Loser(source_id) => {
            dependents.entry(source_id).or_default().push(set.id);
          }
          _ => {}
        }
      }
    }

    let mut affected = HashSet::new();
    let mut stack = vec![root_id];
    while let Some(current) = stack.pop() {
      if !affected.insert(current) {
        continue;
      }
      if let Some(children) = dependents.get(&current) {
        stack.extend(children.iter().copied());
      }
    }
    affected
  }

  fn collect_outcomes(&self, skip: &HashSet<u64>) -> Vec<SetOutcome> {
    let mut outcomes = Vec::new();
    for set in &self.sets {
      if skip.contains(&set.id) {
        continue;
      }
      if set.state != SimSetState::Completed {
        continue;
      }
      if set.slots.iter().any(|slot| slot.entrant_id.is_none()) {
        continue;
      }
      if let Some(dq_slot) = set
        .slots
        .iter()
        .position(|slot| matches!(slot.result, Some(SlotResult::Dq)))
      {
        outcomes.push(SetOutcome {
          id: set.id,
          sort_order: set.sort_order,
          kind: SetOutcomeKind::Dq { dq_slot },
        });
        continue;
      }
      let Some(winner_slot) = set.winner_slot else {
        continue;
      };
      let scores = [
        set.slots[0].score.unwrap_or(0),
        set.slots[1].score.unwrap_or(0),
      ];
      outcomes.push(SetOutcome {
        id: set.id,
        sort_order: set.sort_order,
        kind: SetOutcomeKind::Finish { winner_slot, scores },
      });
    }
    outcomes
  }

  fn resolve_slot(&self, source: SlotSource) -> SlotResolution {
    match source {
      SlotSource::Empty => SlotResolution::Empty,
      SlotSource::Entrant(id) => SlotResolution::Ready(id),
      SlotSource::Winner(set_id) => {
        let Some(set) = self.get_set(set_id) else {
          return SlotResolution::Empty;
        };
        if set.state == SimSetState::Completed {
          if let Some(winner) = set_winner_id(set) {
            SlotResolution::Ready(winner)
          } else {
            SlotResolution::Empty
          }
        } else if set.state == SimSetState::Skipped {
          SlotResolution::Empty
        } else {
          SlotResolution::Pending
        }
      }
      SlotSource::Loser(set_id) => {
        let Some(set) = self.get_set(set_id) else {
          return SlotResolution::Empty;
        };
        if set.state == SimSetState::Completed {
          if let Some(loser) = set_loser_id(set) {
            SlotResolution::Ready(loser)
          } else {
            SlotResolution::Empty
          }
        } else if set.state == SimSetState::Skipped {
          SlotResolution::Empty
        } else {
          SlotResolution::Pending
        }
      }
    }
  }

  fn pick_winner(&mut self, a_id: u32, b_id: u32) -> usize {
    let seed_a = self.entrants_by_id.get(&a_id).map(|e| e.seed).unwrap_or(999);
    let seed_b = self.entrants_by_id.get(&b_id).map(|e| e.seed).unwrap_or(999);
    let weight_a = 1.0 / seed_a as f64;
    let weight_b = 1.0 / seed_b as f64;
    let roll = self.rng.next_f64() * (weight_a + weight_b);
    if roll < weight_a { 0 } else { 1 }
  }

  fn sample_duration_ms(&mut self) -> u64 {
    let mut min = self.config.simulation.min_set_duration_sec;
    let mut max = self.config.simulation.max_set_duration_sec;
    if min == 0 && max == 0 {
      min = 300;
      max = 540;
    }
    if max < min {
      std::mem::swap(&mut min, &mut max);
    }
    let picked = self.rng.gen_range_u32(min, max);
    let scale = if self.config.simulation.time_scale <= 0.0 {
      1.0
    } else {
      self.config.simulation.time_scale
    };
    ((picked as f64) * 1000.0 / scale).round() as u64
  }

  fn get_set(&self, set_id: u64) -> Option<&SimSet> {
    self.set_index.get(&set_id).and_then(|idx| self.sets.get(*idx))
  }

  fn snapshot(&self, now_ms: u64) -> StartggSimState {
    let entrants = self
      .entrants
      .iter()
      .cloned()
      .map(|e| StartggSimEntrant {
        id: e.id,
        name: e.name,
        seed: e.seed,
        slippi_code: e.slippi_code,
      })
      .collect::<Vec<_>>();
    let sets = self
      .sets
      .iter()
      .map(|set| {
        let slots = set
          .slots
          .iter()
          .map(|slot| {
            let entrant = slot.entrant_id.and_then(|id| self.entrants_by_id.get(&id));
            StartggSimSlot {
              entrant_id: slot.entrant_id,
              entrant_name: entrant.map(|e| e.name.clone()),
              slippi_code: entrant.map(|e| e.slippi_code.clone()),
              seed: entrant.map(|e| e.seed),
              score: slot.score,
              result: slot.result.map(|r| match r {
                SlotResult::Win => "win".to_string(),
                SlotResult::Loss => "loss".to_string(),
                SlotResult::Dq => "dq".to_string(),
              }),
            }
          })
          .collect();
        StartggSimSet {
          id: set.id,
          phase_id: set.phase_id.clone(),
          phase_name: self.config.phases[0].name.clone(),
          round: set.round,
          round_label: set.round_label.clone(),
          best_of: set.best_of,
          state: match set.state {
            SimSetState::Pending => "pending".to_string(),
            SimSetState::InProgress => "inProgress".to_string(),
            SimSetState::Completed => "completed".to_string(),
            SimSetState::Skipped => "skipped".to_string(),
          },
          started_at_ms: set.started_at_ms,
          completed_at_ms: set.completed_at_ms,
          updated_at_ms: set.updated_at_ms,
          winner_id: set_winner_id(set),
          slots,
        }
      })
      .collect::<Vec<_>>();

    StartggSimState {
      event: self.config.event.clone(),
      phases: self.config.phases.clone(),
      entrants,
      sets,
      started_at_ms: self.started_at_ms,
      now_ms,
      reference_tournament_link: self.config.reference_tournament_link.clone(),
    }
  }
}

fn startgg_state_to_raw(state: &StartggSimState, now_ms: u64) -> Value {
  let phases = state
    .phases
    .iter()
    .map(|phase| json!({ "id": phase.id, "name": phase.name }))
    .collect::<Vec<_>>();

  let entrants = state
    .entrants
    .iter()
    .map(|entrant| {
      let user_id = entrant.id + 10_000;
      let participant_id = entrant.id + 20_000;
      json!({
        "id": entrant.id,
        "name": entrant.name,
        "seeds": [{ "seedNum": entrant.seed }],
        "customFields": [
          { "id": "slippi-code", "name": "Slippi Code", "value": entrant.slippi_code }
        ],
        "participants": [{
          "id": participant_id,
          "gamerTag": entrant.name,
          "prefix": null,
          "user": {
            "id": user_id,
            "slug": slugify(&entrant.name),
            "authorizations": [
              { "type": "SLIPPI", "externalUsername": entrant.slippi_code }
            ]
          }
        }]
      })
    })
    .collect::<Vec<_>>();

  let sets = state
    .sets
    .iter()
    .map(|set| {
      let full_round_text = full_round_text(&set.round_label, set.round);
      let slots = set
        .slots
        .iter()
        .enumerate()
        .map(|(idx, slot)| {
          let entrant = slot.entrant_id.map(|id| {
            json!({
              "id": id,
              "name": slot.entrant_name.clone().unwrap_or_else(|| format!("Entrant {id}"))
            })
          });
          let placement = match (set.winner_id, slot.entrant_id) {
            (Some(winner), Some(id)) if winner == id => Some(1),
            (Some(_), Some(_)) => Some(2),
            _ => None,
          };
          let score_label = match slot.result.as_deref() {
            Some("dq") => Some("DQ"),
            _ => None,
          };
          json!({
            "id": format!("slot-{}-{}", set.id, idx + 1),
            "entrant": entrant,
            "standing": {
              "placement": placement,
              "stats": {
                "score": { "value": slot.score, "label": score_label }
              }
            }
          })
        })
        .collect::<Vec<_>>();
      json!({
        "id": set.id,
        "round": set.round,
        "fullRoundText": full_round_text,
        "state": state_code(&set.state),
        "winnerId": set.winner_id,
        "startedAt": to_seconds(set.started_at_ms),
        "completedAt": to_seconds(set.completed_at_ms),
        "updatedAt": to_seconds(Some(set.updated_at_ms)),
        "slots": slots,
        "phaseGroup": {
          "id": format!("pg-{}-{}", set.phase_id, set.round_label),
          "phase": { "id": set.phase_id, "name": set.phase_name }
        }
      })
    })
    .collect::<Vec<_>>();

  let total_sets = sets.len();
  json!({
    "data": {
      "event": {
        "id": state.event.id,
        "name": state.event.name,
        "slug": state.event.slug,
        "phases": phases,
        "entrants": {
          "nodes": entrants,
          "pageInfo": { "total": state.entrants.len(), "totalPages": 1, "page": 1, "perPage": state.entrants.len() }
        },
        "sets": {
          "nodes": sets,
          "pageInfo": { "total": total_sets, "totalPages": 1, "page": 1, "perPage": total_sets }
        }
      }
    },
    "extensions": {
      "nowMs": now_ms,
      "startedAtMs": state.started_at_ms,
      "eventLink": state.reference_tournament_link
    }
  })
}

fn state_code(state: &str) -> i32 {
  match state {
    "pending" => 1,
    "inProgress" => 3,
    "completed" => 4,
    "skipped" => 6,
    _ => 1,
  }
}

fn full_round_text(label: &str, round: i32) -> String {
  let trimmed = label.trim();
  if !trimmed.is_empty() {
    let lower = trimmed.to_lowercase();
    if lower.contains("winner")
      || lower.contains("loser")
      || lower.contains("grand")
      || lower.contains("final")
    {
      return trimmed.to_string();
    }
  }
  if let Some(rest) = trimmed.strip_prefix('W') {
    if let Ok(num) = rest.parse::<u32>() {
      return format!("Winners Round {}", num);
    }
  }
  if let Some(rest) = trimmed.strip_prefix('L') {
    if let Ok(num) = rest.parse::<u32>() {
      return format!("Losers Round {}", num);
    }
  }
  if trimmed.starts_with("GF") {
    return if trimmed.ends_with('2') {
      "Grand Finals Reset".to_string()
    } else {
      "Grand Finals".to_string()
    };
  }
  if round == 0 {
    "Grand Finals".to_string()
  } else {
    format!("Round {}", round)
  }
}

fn to_seconds(ms: Option<u64>) -> Option<u64> {
  ms.map(|value| value / 1000)
}

fn slugify(name: &str) -> String {
  let mut out = String::new();
  let mut last_dash = false;
  for ch in name.chars() {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_alphanumeric() {
      out.push(lower);
      last_dash = false;
    } else if !last_dash {
      out.push('-');
      last_dash = true;
    }
  }
  out.trim_matches('-').to_string()
}

fn apply_slot_resolution(slot: &mut SimSlot, resolution: SlotResolution) -> bool {
  match resolution {
    SlotResolution::Ready(id) => {
      if slot.entrant_id == Some(id) {
        false
      } else {
        slot.entrant_id = Some(id);
        true
      }
    }
    SlotResolution::Empty => {
      if slot.entrant_id.is_some() {
        slot.entrant_id = None;
        true
      } else {
        false
      }
    }
    SlotResolution::Pending => false,
  }
}

fn finalize_bye_set(set: &mut SimSet, winner_slot: usize, games_to_win: u8, now_ms: u64) {
  set.state = SimSetState::Completed;
  set.started_at_ms = Some(now_ms);
  set.completed_at_ms = Some(now_ms);
  set.updated_at_ms = now_ms;
  set.winner_slot = Some(winner_slot);
  set.loser_slot = None;
  set.slots[winner_slot].score = Some(games_to_win);
  set.slots[winner_slot].result = Some(SlotResult::Win);
}

fn games_to_win(best_of: u8) -> u8 {
  (best_of / 2) + 1
}

fn set_winner_id(set: &SimSet) -> Option<u32> {
  let winner_slot = set.winner_slot?;
  set.slots.get(winner_slot)?.entrant_id
}

fn set_loser_id(set: &SimSet) -> Option<u32> {
  let loser_slot = set.loser_slot?;
  set.slots.get(loser_slot)?.entrant_id
}

fn normalize_entrants(config_entrants: &[StartggSimEntrantConfig]) -> Result<Vec<SimEntrant>, String> {
  if config_entrants.is_empty() {
    return Err("No entrants provided for Start.gg sim.".to_string());
  }

  let mut used_seeds = HashSet::new();
  let mut assigned: Vec<(StartggSimEntrantConfig, u32)> = Vec::with_capacity(config_entrants.len());

  for entrant in config_entrants {
    let seed = entrant.seed.filter(|s| *s > 0 && !used_seeds.contains(s));
    let final_seed = if let Some(seed) = seed {
      used_seeds.insert(seed);
      seed
    } else {
      0
    };
    assigned.push((entrant.clone(), final_seed));
  }

  let mut next_seed = 1u32;
  for (_, seed) in assigned.iter_mut() {
    if *seed != 0 {
      continue;
    }
    while used_seeds.contains(&next_seed) {
      next_seed += 1;
    }
    *seed = next_seed;
    used_seeds.insert(next_seed);
    next_seed += 1;
  }

  let mut entrants = assigned
    .into_iter()
    .map(|(entrant, seed)| SimEntrant {
      id: entrant.id,
      name: entrant.name,
      slippi_code: entrant.slippi_code,
      seed,
    })
    .collect::<Vec<_>>();
  entrants.sort_by_key(|e| e.seed);
  Ok(entrants)
}

fn build_reference_sets(
  entrants: &[SimEntrant],
  phase: &StartggSimPhaseConfig,
  reference_sets: &[StartggReferenceSet],
) -> Result<(Vec<SimSet>, HashMap<u64, usize>), String> {
  if reference_sets.is_empty() {
    return Err("Reference sets are empty.".to_string());
  }
  let has_prereq = reference_sets.iter().any(|reference| {
    reference
      .slots
      .iter()
      .any(|slot| slot.prereq_id.is_some() || slot.prereq_type.is_some())
  });
  if !has_prereq {
    return Err("Reference sets are missing prereq data; re-sync from Start.gg with slot prereqs enabled.".to_string());
  }

  let mut seed_to_id = HashMap::new();
  for entrant in entrants {
    seed_to_id.insert(entrant.seed, entrant.id);
  }

  let set_ids = reference_sets
    .iter()
    .filter_map(|reference| reference.id)
    .collect::<HashSet<_>>();

  let mut sets = Vec::with_capacity(reference_sets.len());
  let mut index = HashMap::new();
  let mut next_order = 1u64;

  for reference in reference_sets {
    let Some(id) = reference.id else {
      continue;
    };
    if index.contains_key(&id) {
      continue;
    }
    let round = reference.round.unwrap_or(0);
    let round_label = reference_round_label(reference, round);
    let slot_a =
      slot_source_from_reference_slot(reference.slots.get(0), &seed_to_id, &set_ids);
    let slot_b =
      slot_source_from_reference_slot(reference.slots.get(1), &seed_to_id, &set_ids);
    let set = SimSet {
      id,
      phase_id: phase.id.clone(),
      round,
      round_label,
      best_of: phase.best_of,
      slots: [
        SimSlot {
          source: slot_a,
          entrant_id: None,
          score: None,
          result: None,
        },
        SimSlot {
          source: slot_b,
          entrant_id: None,
          score: None,
          result: None,
        },
      ],
      state: SimSetState::Pending,
      started_at_ms: None,
      completed_at_ms: None,
      updated_at_ms: 0,
      winner_slot: None,
      loser_slot: None,
      condition: None,
      sort_order: next_order,
      end_at_ms: None,
    };
    sets.push(set);
    index.insert(id, sets.len() - 1);
    next_order += 1;
  }

  Ok((sets, index))
}

fn reference_round_label(reference: &StartggReferenceSet, round: i32) -> String {
  if let Some(text) = reference.full_round_text.as_ref() {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
      return trimmed.to_string();
    }
  }
  if round == 0 {
    "Grand Final".to_string()
  } else if round > 0 {
    format!("W{}", round)
  } else {
    format!("L{}", round.abs())
  }
}

fn slot_source_from_reference_slot(
  slot: Option<&StartggReferenceSlot>,
  seed_to_id: &HashMap<u32, u32>,
  set_ids: &HashSet<u64>,
) -> SlotSource {
  let Some(slot) = slot else {
    return SlotSource::Empty;
  };

  let direct_entrant = slot.entrant.as_ref().and_then(|entrant| entrant.id);
  if let Some(prereq_type) = slot.prereq_type.as_ref() {
    let lower = prereq_type.to_lowercase();
    if lower.contains("set") {
      if let Some(prereq_id) = slot.prereq_id {
        if !set_ids.contains(&prereq_id) {
          if let Some(entrant_id) = direct_entrant {
            return SlotSource::Entrant(entrant_id);
          }
        }
        let placement = slot.prereq_placement.unwrap_or(1);
        if placement <= 1 {
          return SlotSource::Winner(prereq_id);
        }
        return SlotSource::Loser(prereq_id);
      }
    } else if lower.contains("winner") {
      if let Some(prereq_id) = slot.prereq_id {
        if !set_ids.contains(&prereq_id) {
          if let Some(entrant_id) = direct_entrant {
            return SlotSource::Entrant(entrant_id);
          }
        }
        return SlotSource::Winner(prereq_id);
      }
    } else if lower.contains("loser") {
      if let Some(prereq_id) = slot.prereq_id {
        if !set_ids.contains(&prereq_id) {
          if let Some(entrant_id) = direct_entrant {
            return SlotSource::Entrant(entrant_id);
          }
        }
        return SlotSource::Loser(prereq_id);
      }
    } else if lower.contains("seed") {
      if let Some(entrant_id) = direct_entrant {
        return SlotSource::Entrant(entrant_id);
      }
      if let Some(prereq_id) = slot.prereq_id {
        if let Ok(seed) = u32::try_from(prereq_id) {
          if let Some(entrant_id) = seed_to_id.get(&seed).copied() {
            return SlotSource::Entrant(entrant_id);
          }
        }
      }
    }
  }

  if let Some(entrant_id) = direct_entrant {
    return SlotSource::Entrant(entrant_id);
  }

  SlotSource::Empty
}

fn build_double_elim_sets(
  entrants: &[SimEntrant],
  phase: &StartggSimPhaseConfig,
  allow_reset: bool,
) -> Result<(Vec<SimSet>, HashMap<u64, usize>), String> {
  let entrant_count = entrants.len();
  let bracket_size = next_power_of_two(entrant_count.max(2));
  let mut rounds = 0usize;
  let mut size = bracket_size;
  while size > 1 {
    rounds += 1;
    size /= 2;
  }

  let mut seed_map: HashMap<u32, u32> = HashMap::new();
  for entrant in entrants {
    seed_map.insert(entrant.seed, entrant.id);
  }

  let seeds = seed_positions(bracket_size as u32);
  let mut sets = Vec::new();
  let mut index = HashMap::new();
  let mut next_id = 1u64;
  let mut next_order = 1u64;

  let mut winners_rounds: Vec<Vec<u64>> = Vec::new();

  let mut w1_ids = Vec::new();
  for i in 0..(bracket_size / 2) {
    let seed_a = seeds[i * 2];
    let seed_b = seeds[i * 2 + 1];
    let slot_a = seed_map
      .get(&seed_a)
      .copied()
      .map(SlotSource::Entrant)
      .unwrap_or(SlotSource::Empty);
    let slot_b = seed_map
      .get(&seed_b)
      .copied()
      .map(SlotSource::Entrant)
      .unwrap_or(SlotSource::Empty);
    let id = push_set(
      &mut sets,
      &mut index,
      &mut next_id,
      &mut next_order,
      phase,
      1,
      "W1".to_string(),
      slot_a,
      slot_b,
    );
    w1_ids.push(id);
  }
  winners_rounds.push(w1_ids);

  for round in 2..=rounds {
    let prev = &winners_rounds[round - 2];
    let mut ids = Vec::new();
    for i in 0..(prev.len() / 2) {
      let slot_a = SlotSource::Winner(prev[i * 2]);
      let slot_b = SlotSource::Winner(prev[i * 2 + 1]);
      let id = push_set(
        &mut sets,
        &mut index,
        &mut next_id,
        &mut next_order,
        phase,
        round as i32,
        format!("W{round}"),
        slot_a,
        slot_b,
      );
      ids.push(id);
    }
    winners_rounds.push(ids);
  }

  let mut losers_rounds: Vec<Vec<u64>> = Vec::new();

  if rounds > 1 {
    for i in 1..rounds {
      let count = winners_rounds[i].len();
      let mut odd_ids = Vec::new();
      for j in 0..count {
        let (slot_a, slot_b) = if i == 1 {
          let w1 = &winners_rounds[0];
          (SlotSource::Loser(w1[j * 2]), SlotSource::Loser(w1[j * 2 + 1]))
        } else {
          let prev_even = losers_rounds.last().ok_or_else(|| "Missing losers round".to_string())?;
          (
            SlotSource::Winner(prev_even[j * 2]),
            SlotSource::Winner(prev_even[j * 2 + 1]),
          )
        };
        let id = push_set(
          &mut sets,
          &mut index,
          &mut next_id,
          &mut next_order,
          phase,
          -((i as i32) * 2 - 1),
          format!("L{}", (i * 2) - 1),
          slot_a,
          slot_b,
        );
        odd_ids.push(id);
      }
      losers_rounds.push(odd_ids);

      let mut even_ids = Vec::new();
      let l_odd = losers_rounds.last().ok_or_else(|| "Missing losers round".to_string())?;
      for j in 0..count {
        let slot_a = SlotSource::Winner(l_odd[j]);
        let slot_b = SlotSource::Loser(winners_rounds[i][j]);
        let id = push_set(
          &mut sets,
          &mut index,
          &mut next_id,
          &mut next_order,
        phase,
        -((i as i32) * 2),
        format!("L{}", i * 2),
        slot_a,
        slot_b,
      );
        even_ids.push(id);
      }
      losers_rounds.push(even_ids);
    }
  }

  let winners_final = *winners_rounds
    .last()
    .and_then(|round| round.first())
    .ok_or_else(|| "Missing winners final".to_string())?;

  let losers_final_source = if let Some(last_round) = losers_rounds.last() {
    let set_id = *last_round.first().ok_or_else(|| "Missing losers final".to_string())?;
    SlotSource::Winner(set_id)
  } else {
    SlotSource::Loser(winners_final)
  };

  let gf1_id = push_set(
    &mut sets,
    &mut index,
    &mut next_id,
    &mut next_order,
    phase,
    0,
    "GF1".to_string(),
    SlotSource::Winner(winners_final),
    losers_final_source,
  );

  if allow_reset {
    let gf2_id = push_set(
      &mut sets,
      &mut index,
      &mut next_id,
      &mut next_order,
      phase,
      0,
      "GF2".to_string(),
      SlotSource::Winner(gf1_id),
      SlotSource::Loser(gf1_id),
    );
    if let Some(set) = index.get(&gf2_id).and_then(|idx| sets.get_mut(*idx)) {
      set.condition = Some(SimSetCondition::GrandFinalReset {
        gf1_id,
        losers_slot_index: 1,
      });
    }
  }

  Ok((sets, index))
}

fn push_set(
  sets: &mut Vec<SimSet>,
  index: &mut HashMap<u64, usize>,
  next_id: &mut u64,
  next_order: &mut u64,
  phase: &StartggSimPhaseConfig,
  round: i32,
  round_label: String,
  slot_a: SlotSource,
  slot_b: SlotSource,
) -> u64 {
  let id = *next_id;
  *next_id += 1;
  let order = *next_order;
  *next_order += 1;
  let set = SimSet {
    id,
    phase_id: phase.id.clone(),
    round,
    round_label,
    best_of: phase.best_of,
    slots: [
      SimSlot {
        source: slot_a,
        entrant_id: None,
        score: None,
        result: None,
      },
      SimSlot {
        source: slot_b,
        entrant_id: None,
        score: None,
        result: None,
      },
    ],
    state: SimSetState::Pending,
    started_at_ms: None,
    completed_at_ms: None,
    updated_at_ms: 0,
    winner_slot: None,
    loser_slot: None,
    condition: None,
    sort_order: order,
    end_at_ms: None,
  };
  sets.push(set);
  index.insert(id, sets.len() - 1);
  id
}

fn seed_positions(size: u32) -> Vec<u32> {
  let mut seeds = vec![1u32];
  while seeds.len() < size as usize {
    let n = seeds.len() as u32;
    let mut next = Vec::with_capacity(seeds.len() * 2);
    for seed in seeds.iter().copied() {
      next.push(seed);
      next.push((n * 2 + 1).saturating_sub(seed));
    }
    seeds = next;
  }
  seeds
}

fn next_power_of_two(n: usize) -> usize {
  let mut value = n.max(1);
  if value.is_power_of_two() {
    return value;
  }
  value = value.next_power_of_two();
  value
}
