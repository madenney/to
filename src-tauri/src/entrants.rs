use std::collections::{HashMap, HashSet};
use crate::config::normalize_slippi_code;
use crate::startgg_sim::{StartggSimSet, StartggSimState};
use crate::types::{ActiveGame, EntrantBracketState, LiveGameInfo, UnifiedEntrant};

/// EntrantManager aggregates entrant data from multiple sources:
/// - Start.gg (primary source of truth for tournament data)
/// - Slippi App (streaming status via CDP scraping)
/// - Spectate Folder (playing status via .slp parsing)
#[derive(Default)]
pub struct EntrantManager {
    /// All entrants indexed by their Start.gg ID
    entrants: HashMap<u32, UnifiedEntrant>,
    /// Index from normalized slippi code to entrant ID for fast lookup
    slippi_code_index: HashMap<String, u32>,
    /// Whether auto-assignment is enabled
    auto_assign_enabled: bool,
    /// User-defined slippi code overrides (entrant_id -> slippi_code)
    slippi_code_overrides: HashMap<u32, String>,
}

impl EntrantManager {
    pub fn new() -> Self {
        EntrantManager::default()
    }

    /// Update entrants from Start.gg data (primary source)
    /// This replaces all tournament-related data while preserving
    /// streaming/playing status and user-defined slippi code overrides.
    pub fn update_from_startgg(&mut self, state: &StartggSimState) {
        // Build new entrant map from Start.gg data
        let mut new_entrants: HashMap<u32, UnifiedEntrant> = HashMap::new();
        let mut new_code_index: HashMap<String, u32> = HashMap::new();

        // Build a map of entrant_id -> in-progress set for current match info
        let mut entrant_current_sets: HashMap<u32, &StartggSimSet> = HashMap::new();
        for set in &state.sets {
            if set.state == "inProgress" {
                for slot in &set.slots {
                    if let Some(entrant_id) = slot.entrant_id {
                        entrant_current_sets.insert(entrant_id, set);
                    }
                }
            }
        }

        // Determine bracket state for each entrant
        let mut entrant_states: HashMap<u32, EntrantBracketState> = HashMap::new();
        for entrant in &state.entrants {
            // Default to active
            entrant_states.insert(entrant.id, EntrantBracketState::Active);
        }

        // Check for eliminated/winner status from completed sets
        for set in &state.sets {
            if set.state != "completed" {
                continue;
            }
            if let Some(winner_id) = set.winner_id {
                // Check if this is a finals set (last set in bracket)
                let is_finals = set.round_label.to_lowercase().contains("grand finals")
                    || set.round_label.to_lowercase().contains("finals");

                if is_finals {
                    entrant_states.insert(winner_id, EntrantBracketState::Winner);
                }

                // Mark loser as eliminated if they have no more sets
                for slot in &set.slots {
                    if let Some(entrant_id) = slot.entrant_id {
                        if entrant_id != winner_id && slot.result.as_deref() == Some("loss") {
                            // Check if entrant has any pending sets
                            let has_pending = state.sets.iter().any(|s| {
                                s.state == "pending" && s.slots.iter().any(|sl| sl.entrant_id == Some(entrant_id))
                            });
                            if !has_pending {
                                entrant_states.insert(entrant_id, EntrantBracketState::Eliminated);
                            }
                        }
                    }
                }
            }
        }

        for entrant in &state.entrants {
            // Check for user override first, then use Start.gg code
            let slippi_code = self.slippi_code_overrides
                .get(&entrant.id)
                .cloned()
                .or_else(|| {
                    if entrant.slippi_code.is_empty() {
                        None
                    } else {
                        Some(entrant.slippi_code.clone())
                    }
                });

            let mut unified = UnifiedEntrant::new(
                entrant.id,
                entrant.name.clone(),
                entrant.seed,
                slippi_code.clone(),
            );

            // Set bracket state
            unified.bracket_state = entrant_states
                .get(&entrant.id)
                .cloned()
                .unwrap_or(EntrantBracketState::Active);

            // Set current set info if in progress
            if let Some(current_set) = entrant_current_sets.get(&entrant.id) {
                unified.current_set_id = Some(current_set.id);

                // Find opponent and scores from the set slots
                let my_score = current_set.slots.iter()
                    .find(|s| s.entrant_id == Some(entrant.id))
                    .and_then(|s| s.score)
                    .unwrap_or(0);
                let opponent_slot = current_set.slots.iter()
                    .find(|s| s.entrant_id.is_some() && s.entrant_id != Some(entrant.id));
                let opp_score = opponent_slot
                    .and_then(|s| s.score)
                    .unwrap_or(0);
                let opp_name = opponent_slot
                    .and_then(|s| s.entrant_name.clone());
                let opp_code = opponent_slot
                    .and_then(|s| s.slippi_code.clone());
                let game_number = (my_score as u32) + (opp_score as u32) + 1;

                unified.current_game = Some(LiveGameInfo {
                    stage: None,
                    character: String::new(),
                    opponent_code: opp_code,
                    opponent_name: opp_name,
                    round_label: Some(current_set.round_label.clone()),
                    best_of: Some(current_set.best_of),
                    game_number: Some(game_number),
                    scores: Some([my_score, opp_score]),
                });
            }

            // Preserve existing status from previous state if available
            if let Some(existing) = self.entrants.get(&entrant.id) {
                unified.is_streaming = existing.is_streaming;
                unified.is_playing = existing.is_playing;
                // Merge spectate folder data (stage, character) into set-derived game info
                if let Some(ref existing_game) = existing.current_game {
                    if let Some(ref mut game) = unified.current_game {
                        if existing_game.stage.is_some() {
                            game.stage = existing_game.stage.clone();
                        }
                        if !existing_game.character.is_empty() {
                            game.character = existing_game.character.clone();
                        }
                    } else {
                        // No active set but spectate folder says they're playing
                        unified.current_game = existing.current_game.clone();
                    }
                }
                unified.assigned_setup_id = existing.assigned_setup_id;
                unified.auto_assigned = existing.auto_assigned;
            }

            // Update code index
            if let Some(ref code) = slippi_code {
                if let Some(normalized) = normalize_slippi_code(code) {
                    new_code_index.insert(normalized, entrant.id);
                }
            }

            new_entrants.insert(entrant.id, unified);
        }

        self.entrants = new_entrants;
        self.slippi_code_index = new_code_index;
    }

    /// Update streaming status from Slippi App
    /// streaming_codes should contain normalized slippi codes of entrants currently streaming
    pub fn update_streaming_status(&mut self, streaming_codes: &HashSet<String>) {
        for entrant in self.entrants.values_mut() {
            let is_streaming = entrant.slippi_code.as_ref()
                .and_then(|code| normalize_slippi_code(code))
                .map(|normalized| streaming_codes.contains(&normalized))
                .unwrap_or(false);
            entrant.is_streaming = is_streaming;
        }
    }

    /// Update playing status from spectate folder.
    /// Merges spectate data (stage, character) with existing set-derived data
    /// (round_label, game_number, scores) if present.
    pub fn update_playing_status(&mut self, active_games: &[ActiveGame]) {
        // First, clear playing status but preserve set-derived game info
        for entrant in self.entrants.values_mut() {
            entrant.is_playing = false;
            // Keep current_game if it has set-derived data (round_label etc.),
            // but clear the spectate-only fields
            if let Some(ref mut game) = entrant.current_game {
                if game.round_label.is_some() {
                    // Has set data - just clear spectate fields
                    game.stage = None;
                    game.character = String::new();
                } else {
                    // Spectate-only game info - clear entirely
                    entrant.current_game = None;
                }
            }
        }

        // Then set playing status for entrants in active games
        for game in active_games {
            for (i, code) in game.slippi_codes.iter().enumerate() {
                if let Some(normalized) = normalize_slippi_code(code) {
                    if let Some(&entrant_id) = self.slippi_code_index.get(&normalized) {
                        if let Some(entrant) = self.entrants.get_mut(&entrant_id) {
                            entrant.is_playing = true;

                            let opponent_code = game.slippi_codes.iter()
                                .enumerate()
                                .find(|(j, _)| *j != i)
                                .map(|(_, c)| c.clone());
                            let stage = game.stage.clone();
                            let character = game.characters.get(i).cloned().unwrap_or_default();

                            if let Some(ref mut existing_game) = entrant.current_game {
                                // Merge spectate data into existing set-derived info
                                existing_game.stage = stage;
                                existing_game.character = character;
                                if existing_game.opponent_code.is_none() {
                                    existing_game.opponent_code = opponent_code;
                                }
                            } else {
                                // No set data - create spectate-only game info
                                entrant.current_game = Some(LiveGameInfo {
                                    stage,
                                    character,
                                    opponent_code,
                                    opponent_name: None,
                                    round_label: None,
                                    best_of: None,
                                    game_number: None,
                                    scores: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Set slippi code for an entrant (user edit/override)
    pub fn set_slippi_code(&mut self, entrant_id: u32, code: Option<String>) -> Result<(), String> {
        // Update or remove the override
        if let Some(ref new_code) = code {
            let normalized = normalize_slippi_code(new_code)
                .ok_or_else(|| format!("Invalid slippi code format: {}", new_code))?;

            // Remove old code from index if present
            if let Some(entrant) = self.entrants.get(&entrant_id) {
                if let Some(old_code) = &entrant.slippi_code {
                    if let Some(old_normalized) = normalize_slippi_code(old_code) {
                        self.slippi_code_index.remove(&old_normalized);
                    }
                }
            }

            // Add new code to index and override map
            self.slippi_code_index.insert(normalized, entrant_id);
            self.slippi_code_overrides.insert(entrant_id, new_code.clone());
        } else {
            // Clear override
            self.slippi_code_overrides.remove(&entrant_id);

            // Remove from index
            if let Some(entrant) = self.entrants.get(&entrant_id) {
                if let Some(old_code) = &entrant.slippi_code {
                    if let Some(old_normalized) = normalize_slippi_code(old_code) {
                        self.slippi_code_index.remove(&old_normalized);
                    }
                }
            }
        }

        // Update the entrant
        if let Some(entrant) = self.entrants.get_mut(&entrant_id) {
            entrant.slippi_code = code;
            Ok(())
        } else {
            Err(format!("Entrant {} not found", entrant_id))
        }
    }

    /// Assign entrant to setup
    pub fn assign_to_setup(&mut self, entrant_id: u32, setup_id: Option<u32>, auto: bool) -> Result<(), String> {
        // If assigning to a setup, first unassign anyone else from that setup
        if let Some(sid) = setup_id {
            for entrant in self.entrants.values_mut() {
                if entrant.assigned_setup_id == Some(sid) && entrant.id != entrant_id {
                    entrant.assigned_setup_id = None;
                    entrant.auto_assigned = false;
                }
            }
        }

        // Assign this entrant
        if let Some(entrant) = self.entrants.get_mut(&entrant_id) {
            entrant.assigned_setup_id = setup_id;
            entrant.auto_assigned = auto;
            Ok(())
        } else {
            Err(format!("Entrant {} not found", entrant_id))
        }
    }

    /// Unassign entrant from their current setup
    pub fn unassign(&mut self, entrant_id: u32) -> Result<(), String> {
        if let Some(entrant) = self.entrants.get_mut(&entrant_id) {
            entrant.assigned_setup_id = None;
            entrant.auto_assigned = false;
            Ok(())
        } else {
            Err(format!("Entrant {} not found", entrant_id))
        }
    }

    /// Run auto-assignment logic
    /// Returns list of (entrant_id, setup_id) assignments made
    pub fn auto_assign(&mut self, available_setups: &[u32]) -> Vec<(u32, u32)> {
        if !self.auto_assign_enabled {
            return Vec::new();
        }

        let mut assignments = Vec::new();

        // Find entrants that are streaming AND playing but not assigned
        let candidates: Vec<u32> = self.entrants.values()
            .filter(|e| {
                e.is_streaming
                && e.is_playing
                && e.assigned_setup_id.is_none()
                && e.bracket_state == EntrantBracketState::Active
            })
            .map(|e| e.id)
            .collect();

        // Find pairs of entrants playing each other
        let pairs = self.find_playing_pairs(&candidates);

        // Track which setups are now used
        let mut used_setups: HashSet<u32> = self.entrants.values()
            .filter_map(|e| e.assigned_setup_id)
            .collect();

        for (entrant1, entrant2) in pairs {
            // Find an available setup
            if let Some(&setup_id) = available_setups.iter()
                .find(|&&id| !used_setups.contains(&id))
            {
                // Assign both entrants to this setup
                if let Some(e1) = self.entrants.get_mut(&entrant1) {
                    e1.assigned_setup_id = Some(setup_id);
                    e1.auto_assigned = true;
                    assignments.push((entrant1, setup_id));
                }
                if let Some(e2) = self.entrants.get_mut(&entrant2) {
                    e2.assigned_setup_id = Some(setup_id);
                    e2.auto_assigned = true;
                    assignments.push((entrant2, setup_id));
                }
                used_setups.insert(setup_id);
            }
        }

        assignments
    }

    /// Find pairs of candidates that are playing each other
    fn find_playing_pairs(&self, candidates: &[u32]) -> Vec<(u32, u32)> {
        let mut pairs = Vec::new();
        let mut used = HashSet::new();

        for &candidate_id in candidates {
            if used.contains(&candidate_id) {
                continue;
            }

            let candidate = match self.entrants.get(&candidate_id) {
                Some(e) => e,
                None => continue,
            };

            // Check if this entrant's opponent is also a candidate
            if let Some(ref game) = candidate.current_game {
                if let Some(ref opp_code) = game.opponent_code {
                    if let Some(normalized) = normalize_slippi_code(opp_code) {
                        if let Some(&opp_id) = self.slippi_code_index.get(&normalized) {
                            if candidates.contains(&opp_id) && !used.contains(&opp_id) {
                                pairs.push((candidate_id, opp_id));
                                used.insert(candidate_id);
                                used.insert(opp_id);
                            }
                        }
                    }
                }
            }
        }

        pairs
    }

    /// Toggle auto-assignment
    pub fn set_auto_assign_enabled(&mut self, enabled: bool) {
        self.auto_assign_enabled = enabled;
    }

    pub fn is_auto_assign_enabled(&self) -> bool {
        self.auto_assign_enabled
    }

    /// Get all entrants
    pub fn get_all(&self) -> Vec<UnifiedEntrant> {
        self.entrants.values().cloned().collect()
    }

    /// Get entrants sorted for display (by seed, with status indicators)
    pub fn get_sorted_for_display(&self) -> Vec<UnifiedEntrant> {
        let mut entrants: Vec<UnifiedEntrant> = self.entrants.values().cloned().collect();
        entrants.sort_by(|a, b| {
            // Active entrants first
            let state_order = |state: &EntrantBracketState| -> u8 {
                match state {
                    EntrantBracketState::Active => 0,
                    EntrantBracketState::Winner => 1,
                    EntrantBracketState::Eliminated => 2,
                }
            };
            let state_cmp = state_order(&a.bracket_state).cmp(&state_order(&b.bracket_state));
            if state_cmp != std::cmp::Ordering::Equal {
                return state_cmp;
            }
            // Then by seed
            a.seed.cmp(&b.seed)
        });
        entrants
    }

    /// Get entrant by ID
    pub fn get(&self, id: u32) -> Option<&UnifiedEntrant> {
        self.entrants.get(&id)
    }

    /// Get entrant by slippi code
    pub fn get_by_slippi_code(&self, code: &str) -> Option<&UnifiedEntrant> {
        normalize_slippi_code(code)
            .and_then(|normalized| self.slippi_code_index.get(&normalized))
            .and_then(|&id| self.entrants.get(&id))
    }

    /// Get entrants assigned to a specific setup
    pub fn get_by_setup(&self, setup_id: u32) -> Vec<&UnifiedEntrant> {
        self.entrants.values()
            .filter(|e| e.assigned_setup_id == Some(setup_id))
            .collect()
    }

    /// Get the highest seed among entrants assigned to a setup
    pub fn highest_seed_for_setup(&self, setup_id: u32) -> Option<u32> {
        self.get_by_setup(setup_id)
            .iter()
            .map(|e| e.seed)
            .min()
    }

    /// Clear all entrants (used when switching tournaments)
    pub fn clear(&mut self) {
        self.entrants.clear();
        self.slippi_code_index.clear();
        self.slippi_code_overrides.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startgg_sim::StartggSimEntrant;

    fn make_test_state() -> StartggSimState {
        StartggSimState {
            event: crate::startgg_sim::StartggSimEventConfig {
                id: "1".to_string(),
                name: "Test Event".to_string(),
                slug: "test-event".to_string(),
            },
            phases: vec![],
            entrants: vec![
                StartggSimEntrant {
                    id: 1,
                    name: "Player1".to_string(),
                    seed: 1,
                    slippi_code: "PLAY#001".to_string(),
                },
                StartggSimEntrant {
                    id: 2,
                    name: "Player2".to_string(),
                    seed: 2,
                    slippi_code: "PLAY#002".to_string(),
                },
            ],
            sets: vec![],
            started_at_ms: 0,
            now_ms: 0,
            reference_tournament_link: None,
        }
    }

    #[test]
    fn test_update_from_startgg() {
        let mut manager = EntrantManager::new();
        let state = make_test_state();

        manager.update_from_startgg(&state);

        assert_eq!(manager.entrants.len(), 2);
        assert!(manager.get(1).is_some());
        assert!(manager.get(2).is_some());
    }

    #[test]
    fn test_slippi_code_lookup() {
        let mut manager = EntrantManager::new();
        let state = make_test_state();

        manager.update_from_startgg(&state);

        let entrant = manager.get_by_slippi_code("PLAY#001").unwrap();
        assert_eq!(entrant.id, 1);
        assert_eq!(entrant.name, "Player1");
    }

    #[test]
    fn test_set_slippi_code() {
        let mut manager = EntrantManager::new();
        let state = make_test_state();

        manager.update_from_startgg(&state);

        // Change Player1's code
        manager.set_slippi_code(1, Some("NEW#001".to_string())).unwrap();

        let entrant = manager.get_by_slippi_code("NEW#001").unwrap();
        assert_eq!(entrant.id, 1);

        // Old code should not work
        assert!(manager.get_by_slippi_code("PLAY#001").is_none());
    }
}
