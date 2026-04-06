#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use shared::ServerMsg;

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
        if Path::new(RANKINGS_FILE).exists() {
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

// ---------------------------------------------------------------------------
// Tournament bracket (single elimination, up to 8 players)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TournamentBracket {
    pub players: Vec<String>,
    /// Pairs of players for current round matches.
    pub matches: Vec<(String, String)>,
    /// Winners of each match in the current round.
    pub winners: Vec<String>,
    pub round: u32,
    pub champion: Option<String>,
}

impl TournamentBracket {
    pub fn new(players: Vec<String>) -> Self {
        let mut p = players;
        // Pad to power of two (max 8).
        let target = if p.len() <= 2 {
            2
        } else if p.len() <= 4 {
            4
        } else {
            8
        };
        while p.len() < target {
            p.push(format!("BYE_{}", p.len()));
        }
        let matches = Self::make_matches(&p);
        Self {
            players: p,
            matches,
            winners: Vec::new(),
            round: 1,
            champion: None,
        }
    }

    fn make_matches(players: &[String]) -> Vec<(String, String)> {
        players.chunks(2).map(|c| (c[0].clone(), c[1].clone())).collect()
    }

    /// Record the winner of a match. If it is a BYE match, the non-BYE player
    /// advances automatically.
    pub fn record_winner(&mut self, winner: String) {
        self.winners.push(winner);
        if self.winners.len() == self.matches.len() {
            // Round complete.
            if self.winners.len() == 1 {
                self.champion = Some(self.winners[0].clone());
            } else {
                self.players = self.winners.drain(..).collect();
                self.matches = Self::make_matches(&self.players);
                self.round += 1;
            }
        }
    }

    pub fn current_matches(&self) -> &[(String, String)] {
        &self.matches
    }
}

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
}

impl Lobby {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            challenges: Vec::new(),
            rankings: Rankings::load(),
            tournament: None,
        }
    }

    pub fn add_player(&mut self, name: String, tx: mpsc::UnboundedSender<ServerMsg>) {
        self.players.insert(name.clone(), PlayerHandle { tx });
        self.broadcast_lobby_state();
    }

    pub fn remove_player(&mut self, name: &str) {
        self.players.remove(name);
        self.challenges.retain(|c| c.from != name && c.to != name);
        self.broadcast_lobby_state();
    }

    pub fn broadcast_lobby_state(&self) {
        let players: Vec<String> = self.players.keys().cloned().collect();
        let msg = ServerMsg::LobbyState { players };
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
            // Notify both that the game is starting.
            self.send_to(challenger, ServerMsg::GameStart);
            self.send_to(acceptor, ServerMsg::GameStart);
            true
        } else {
            false
        }
    }

    pub fn decline_challenge(&mut self, decliner: &str, challenger: &str) {
        self.challenges
            .retain(|c| !(c.from == challenger && c.to == decliner));
    }

    pub fn record_game_result(&mut self, winner: &str, loser: &str) {
        self.rankings.update(winner, loser);
        self.send_to(winner, ServerMsg::GameOver { winner: winner.to_string() });
        self.send_to(loser, ServerMsg::GameOver { winner: winner.to_string() });
    }

    pub fn start_tournament(&mut self, mut participants: Vec<String>) {
        participants.retain(|p| self.players.contains_key(p));
        if participants.len() < 2 {
            return;
        }
        self.tournament = Some(TournamentBracket::new(participants));
    }
}
