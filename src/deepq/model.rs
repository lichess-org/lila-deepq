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
use std::str::FromStr;

use derive_more::{Display, From};
use futures::stream::{Stream, StreamExt};
use log::warn;
use mongodb::bson::{doc, from_document, oid::ObjectId, Bson, DateTime, Document};
use mongodb::Collection;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr, SpaceSeparator, StringWithSeparator};
use shakmaty::uci::Uci;

use crate::db::DbConn;
use crate::error::{Error, Result};
use crate::fishnet::model::JobId;

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

#[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
pub struct ReportId(pub ObjectId);

impl From<ReportId> for ObjectId {
    fn from(ri: ReportId) -> ObjectId {
        ri.0
    }
}

impl FromStr for ReportId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(ReportId(ObjectId::parse_str(s)?))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Report {
    pub _id: ReportId,
    pub user_id: UserId,
    pub date_requested: DateTime,
    pub date_completed: Option<DateTime>,
    pub origin: ReportOrigin,
    pub report_type: ReportType,
    pub games: Vec<GameId>,
    pub sent_to_irwin: bool,
}

impl Report {
    pub fn coll(db: DbConn) -> Collection<Document> {
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
    pub skipped: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EmptyAnalysis {
    pub depth: i32,
    pub score: Score,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BestMove {
    #[serde_as(as = "StringWithSeparator::<SpaceSeparator, Uci>")]
    pub pv: Vec<Uci>,
    pub depth: i32,
    pub score: Score,
    pub time: i64,
    pub nodes: i64,
    pub nps: Option<i64>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MatrixAnalysis {
    #[serde_as(as = "Vec<Vec<Option<Vec<DisplayFromStr>>>>")]
    pub pv: Vec<Vec<Option<Vec<Uci>>>>,
    pub score: Vec<Vec<Option<Score>>>,
    pub depth: i32,
    pub nodes: i64,
    pub time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nps: Option<i64>,
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
    pub fn coll(db: DbConn) -> Collection<Document> {
        db.database.collection("deepq_games")
    }

    pub async fn by_id(
        db: DbConn,
        game_id: GameId,
    ) -> Result<Option<Game>> {
        let filter = doc! { "_id": { "$eq": game_id.0 } };
        Ok(GameAnalysis::coll(db.clone())
            .find_one(filter, None)
            .await?
            .map(from_document)
            .transpose()?)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Nodes {
    pub nnue: i64,
    pub classical: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameAnalysis {
    pub _id: ObjectId,
    pub job_id: JobId,
    pub game_id: GameId,
    pub source_id: UserId,
    pub analysis: Vec<Option<PlyAnalysis>>,
    pub requested_pvs: Option<i32>,
    pub requested_depth: Option<i32>,
    pub requested_nodes: Nodes,
}

impl GameAnalysis {
    pub fn coll(db: DbConn) -> Collection<Document> {
        db.database.collection("deepq_analysis")
    }
    pub fn is_analysis_complete(&self) -> bool {
        self.analysis.iter().filter(|o| o.is_none()).count() == 0_usize
    }

    pub async fn find_by_jobs(
        db: DbConn,
        job_ids: Vec<JobId>,
    ) -> Result<impl Stream<Item = Result<GameAnalysis>>> {
        let p = "GameAnalysis::find_by_jobs >";
        let filter = doc! {
            "job_id": { "$in": job_ids.iter().map(|ji| ji.0).collect::<Vec<ObjectId>>() }
        };
        Ok(GameAnalysis::coll(db.clone())
            .find(filter, None)
            .await?
            .filter_map(move |doc_result| async move {
                match doc_result.is_ok() {
                    false => {
                        warn!(
                            "{} error processing cursor of jobs: {:?}.",
                            p,
                            doc_result.expect_err("silly rabbit")
                        );
                        None
                    }
                    true => Some(doc_result.expect("silly rabbit")),
                }
            })
            .map(from_document::<GameAnalysis>)
            .map(|i| i.map_err(|e| e.into()))
            .boxed())
    }
}
