// Copyright 2020 Lakin Wecker
//
// This file is part of lila-deepq.
//
// lila-deepq is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// lila-deepq is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with lila-deepq.  If not, see <https://www.gnu.org/licenses/>.

use derive_more::{Display, From};
use mongodb::bson::{doc, oid::ObjectId, Bson, DateTime};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr, SpaceSeparator, StringWithSeparator};
use shakmaty::uci::Uci;
use mongodb::Collection;

use crate::db::DbConn;

#[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
pub struct UserId(pub String);

// TODO: this should be easy enough to make into a macro
impl From<UserId> for Bson {
    fn from(ui: UserId) -> Bson {
        Bson::String(ui.to_string())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
pub struct GameId(pub String);

impl From<GameId> for Bson {
    fn from(gi: GameId) -> Bson {
        Bson::String(gi.to_string())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
#[serde(rename_all = "lowercase")]
pub enum ReportOrigin {
    Moderator,
    Random,
    Leaderboard,
    Tournament,
}

impl From<ReportOrigin> for Bson {
    fn from(ro: ReportOrigin) -> Bson {
        Bson::String(ro.to_string().to_lowercase())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, strum_macros::ToString)]
#[serde(rename_all = "lowercase")]
pub enum ReportType {
    Irwin,
    CR,
    PGNSPY,
}

impl From<ReportType> for Bson {
    fn from(rt: ReportType) -> Bson {
        Bson::String(rt.to_string().to_lowercase())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Report {
    pub _id: ObjectId,
    pub user_id: UserId,
    pub date_requested: DateTime,
    pub date_completed: Option<DateTime>,
    pub origin: ReportOrigin,
    pub report_type: ReportType,
    pub games: Vec<GameId>,
}

impl Report {
    pub fn coll(db: DbConn) -> Collection {
        db.database.collection("deepq_reports")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Blurs {
    pub nb: i32,
    pub bits: String, // TODO: why string?!
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Score {
    #[serde(rename = "cp")]
    Cp(i64),
    #[serde(rename = "mate")]
    Mate(i64),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SkippedAnalysis {
    skipped: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EmptyAnalysis {
    depth: u8,
    score: Score,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BestMove {
    #[serde_as(as = "StringWithSeparator::<SpaceSeparator, Uci>")]
    pv: Vec<Uci>,
    depth: u8,
    score: Score,
    time: u64,
    nodes: u64,
    nps: Option<u32>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MatrixAnalysis {
    #[serde_as(as = "Vec<Vec<Option<Vec<DisplayFromStr>>>>")]
    pub pv: Vec<Vec<Option<Vec<Uci>>>>,
    pub score: Vec<Vec<Option<Score>>>,
    pub depth: u8,
    pub nodes: u64,
    pub time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nps: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum PlyAnalysis {
    Matrix(MatrixAnalysis),
    Best(BestMove),
    Skipped(SkippedAnalysis),
    Empty(EmptyAnalysis),
}

// TODO: this should come directly from the lila db, why store this more than once?
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Game {
    pub _id: GameId,
    pub emts: Vec<i32>,
    #[serde_as(as = "StringWithSeparator::<SpaceSeparator, Uci>")]
    pub pgn: Vec<Uci>,
    pub black: Option<UserId>,
    pub white: Option<UserId>,
}

impl Game {
    pub fn coll(db: DbConn) -> Collection {
        db.database.collection("deepq_games")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameAnalysis {
    pub _id: ObjectId,
    pub game_id: GameId,
    pub analysis: Vec<Option<PlyAnalysis>>,
    pub requested_pvs: u8,
    pub requested_depth: Option<u8>,
    pub requested_nodes: Option<u64>,
}

impl GameAnalysis {
    pub fn coll(db: DbConn) -> Collection {
        db.database.collection("deepq_analysis")
    }
}
