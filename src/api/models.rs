use crate::domain::models::{GameResult, PieceType, Player};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiGameState {
    pub pieces: Vec<ApiPiece>,
    pub current_player: Player,
    pub valid_moves: HashMap<String, Vec<ApiValidMove>>,
    pub status: GameResult,
    pub dimension: usize,
    pub side: usize,
    pub in_check: bool,
    pub sequence: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiPiece {
    pub piece_type: PieceType,
    pub owner: Player,
    pub coordinate: Vec<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiValidMove {
    pub to: Vec<usize>,
    pub consequence: MoveConsequence,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum MoveConsequence {
    Capture,
    NoEffect,
    Victory,
}

#[derive(Deserialize, Debug)]
pub struct NewGameRequest {
    pub mode: String,
    pub dimension: Option<usize>,
    pub side: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub struct TurnRequest {
    pub uuid: String,
    pub start: Vec<usize>,
    pub end: Vec<usize>,
}

#[derive(Serialize, Debug)]
pub struct NewGameResponse {
    pub uuid: String,
}
