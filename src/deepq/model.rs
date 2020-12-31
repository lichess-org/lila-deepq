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

#[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
pub struct UserId(pub String);

// TODO: this should be easy enough to make into a macro
impl From<UserId> for Bson {
    fn from(ui: UserId) -> Bson {
        Bson::String(ui.to_string().to_lowercase())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
pub struct GameId(pub String);

impl From<GameId> for Bson {
    fn from(gi: GameId) -> Bson {
        Bson::String(gi.to_string().to_lowercase())
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Blurs {
    pub nb: i32,
    pub bits: String, // TODO: why string?!
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Eval {
    pub cp: Option<i32>,
    pub mate: Option<i32>,
}

// TODO: this should come directly from the lila db, why store this more than once?
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Game {
    pub _id: GameId,
    pub emts: Vec<i32>,
    pub pgn: String,
    pub black: Option<UserId>,
    pub white: Option<UserId>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameAnalysis {
    pub _id: ObjectId,
    pub game_id: GameId,
    pub analysis: Vec<Eval>, // TODO: we should be able to compress this.
    pub requested_pvs: u8,
    pub requested_depth: Option<i32>,
    pub requested_nodes: Option<i32>,
}
