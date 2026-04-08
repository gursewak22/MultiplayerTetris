#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::fs;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use shared::{ServerMsg, TournamentBracket};

// ---------------------------------------------------------------------------
// Ranking persistence
// ---------------------------------------------------------------------------

const RANKINGS_FILE: &str = "rankings.json";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Rankings {
    pub scores: HashMap<String, u32>,
}

impl Rankings {
    pub fn load() -> Self {
        if std::path::Path::new(RANKINGS_FILE).exists() {
            match std::fs::read_to_string(RANKINGS_FILE) {
                Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
                Err(_) => Default::default(),
            }
        } else {
            Default::default()
        }
    }

    pub fn save(&self) {
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(RANKINGS_FILE, data);
        }
    }

    /// Simple Elo-like update: winner gains points, loser loses them.
    pub fn update(&mut self, winner: &str, loser: &str) {
        let winner_rating = self.scores.entry(winner.to_string()).or_insert(1000);
        *winner_rating = winner_rating.saturating_add(25);
        let loser_rating = self.scores.entry(loser.to_string()).or_insert(1000);
        *loser_rating = loser_rating.saturating_sub(20);
        self.save();
    }
}

// Old tournament bracket struct removed

// ---------------------------------------------------------------------------
// Connected player handle
// ---------------------------------------------------------------------------

pub struct PlayerHandle {
    pub tx: mpsc::UnboundedSender<ServerMsg>,
}

// ---------------------------------------------------------------------------
// Lobby
// ---------------------------------------------------------------------------

/// Represents a pending challenge: challenger → target.
#[derive(Debug, Clone)]
pub struct PendingChallenge {
    pub from: String,
    pub to: String,
}

pub struct Lobby {
    pub players: HashMap<String, PlayerHandle>,
    /// Pending challenges keyed by (from, to) as a composite key.
    pub challenges: Vec<PendingChallenge>,
    pub rankings: Rankings,
    pub tournament: Option<TournamentBracket>,
    pub tournament_queue: Vec<String>,
    pub in_game: HashSet<String>,
}

impl Lobby {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            challenges: Vec::new(),
            rankings: Rankings::load(),
            tournament: None,
            tournament_queue: Vec::new(),
            in_game: HashSet::new(),
        }
    }

    pub fn mark_in_game(&mut self, p1: &str, p2: &str) {
        self.in_game.insert(p1.to_string());
        self.in_game.insert(p2.to_string());
    }

    pub fn mark_available(&mut self, p1: &str, p2: &str) {
        self.in_game.remove(p1);
        self.in_game.remove(p2);
    }

    pub fn is_in_game(&self, name: &str) -> bool {
        self.in_game.contains(name)
    }

    pub fn add_player(&mut self, name: String, tx: mpsc::UnboundedSender<ServerMsg>) {
        self.players.insert(name.clone(), PlayerHandle { tx });
        self.broadcast_lobby_state();
    }

    pub fn remove_player(&mut self, name: &str) {
        self.players.remove(name);
        self.challenges.retain(|c| c.from != name && c.to != name);
        self.tournament_queue.retain(|p| p != name);
        self.broadcast_lobby_state();
        self.broadcast_tournament_state();
    }

    pub fn broadcast_lobby_state(&self) {
        let player_list: Vec<String> = self.players.keys().cloned().collect();
        let in_game_list: Vec<String> = self.in_game.iter().cloned().collect();
        let mut scores: Vec<(String, u32)> = self.rankings.scores.clone().into_iter().collect();
        scores.sort_by(|a, b| b.1.cmp(&a.1)); // Sort descending

        let state_msg = ServerMsg::LobbyState { players: player_list, in_game: in_game_list };
        let score_msg = ServerMsg::Scoreboard { scores };

        for handle in self.players.values() {
            let _ = handle.tx.send(state_msg.clone());
            let _ = handle.tx.send(score_msg.clone());
        }
        self.broadcast_tournament_state();
    }

    pub fn broadcast_tournament_state(&self) {
        let msg = ServerMsg::TournamentState {
            queue: self.tournament_queue.clone(),
            bracket: self.tournament.clone(),
        };
        for handle in self.players.values() {
            let _ = handle.tx.send(msg.clone());
        }
    }

    pub fn send_to(&self, name: &str, msg: ServerMsg) {
        if let Some(handle) = self.players.get(name) {
            let _ = handle.tx.send(msg);
        }
    }

    pub fn handle_challenge(&mut self, from: &str, target: &str) {
        // Block challenges involving players already in a match.
        if self.in_game.contains(from) {
            self.send_to(from, ServerMsg::ServerError {
                msg: "You are already in a match.".to_string(),
            });
            return;
        }
        if self.in_game.contains(target) {
            self.send_to(from, ServerMsg::ServerError {
                msg: format!("{target} is already in a match."),
            });
            return;
        }
        // Avoid duplicate challenges.
        let already = self.challenges.iter().any(|c| c.from == from && c.to == target);
        if already {
            return;
        }
        self.challenges.push(PendingChallenge {
            from: from.to_string(),
            to: target.to_string(),
        });
        self.send_to(target, ServerMsg::ChallengeReceived { from: from.to_string() });
    }

    pub fn accept_challenge(&mut self, acceptor: &str, challenger: &str) -> bool {
        let pos = self
            .challenges
            .iter()
            .position(|c| c.from == challenger && c.to == acceptor);
        if let Some(i) = pos {
            self.challenges.remove(i);
            true
        } else {
            false
        }
    }

    pub fn decline_challenge(&mut self, decliner: &str, challenger: &str) {
        self.challenges
            .retain(|c| !(c.from == challenger && c.to == decliner));
    }

    pub fn record_game_result(&mut self, winner: &str, loser: &str) -> (bool, bool) {
        self.rankings.update(winner, loser);
        self.send_to(winner, ServerMsg::GameOver { winner: winner.to_string() });
        self.send_to(loser, ServerMsg::GameOver { winner: winner.to_string() });

        let mut round_complete = false;
        let mut tourney_finished = false;
        if let Some(ref mut bracket) = self.tournament {
            // Is this a tournament match?
            let is_tourney = bracket.matches.iter().any(|(p1, p2)| {
                (p1 == winner && p2 == loser) || (p1 == loser && p2 == winner)
            });
            if is_tourney {
                let round_ended = bracket.record_winner(winner.to_string());
                if round_ended && bracket.champion.is_none() {
                    round_complete = true;
                }
                
                if bracket.champion.is_some() {
                    tourney_finished = true;
                }
            }
        }
        self.broadcast_tournament_state();
        (round_complete, tourney_finished)
    }

    pub fn join_tournament(&mut self, player: &str) -> Result<(), String> {
        if self.tournament.is_some() {
            return Err("Tournament already in progress.".to_string());
        }
        if !self.tournament_queue.contains(&player.to_string()) {
            self.tournament_queue.push(player.to_string());
            self.broadcast_tournament_state();
        }
        Ok(())
    }

    pub fn leave_tournament(&mut self, player: &str) {
        self.tournament_queue.retain(|p| p != player);
        self.broadcast_tournament_state();
    }

    pub fn start_tournament(&mut self) -> Result<(), String> {
        if self.tournament.is_some() {
            return Err("Tournament already running.".to_string());
        }
        let len = self.tournament_queue.len();
        if len != 2 && len != 4 && len != 8 {
            return Err("Tournament requires exactly 2, 4, or 8 players.".to_string());
        }
        
        let mut participants = std::mem::take(&mut self.tournament_queue);
        // Shuffle could happen here, but simplistic rotation or standard order is fine for deterministic tests.
        // Actually letting them naturally pair up by queue order is standard!
        
        self.tournament = Some(TournamentBracket::new(participants));
        self.broadcast_tournament_state();
        Ok(())
    }
}
