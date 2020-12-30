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

pub mod model {
    use chrono::prelude::*;
    use derive_more::{From, Display};
    use serde::{Serialize, Deserialize};
    use mongodb::bson::{
        doc,
        Bson,
        Document,
        ser::to_document,
        oid::ObjectId
    };

    #[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
    pub struct UserId(pub String);

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

    pub fn precedence_for_origin(origin: ReportOrigin) -> i64 {
        match origin {
            ReportOrigin::Moderator => 1_000_000i64,
            ReportOrigin::Tournament => 100i64,
            ReportOrigin::Leaderboard => 100i64,
            ReportOrigin::Random => 10i64,
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
        pub date_requested: DateTime<Utc>,
        pub date_completed: Option<DateTime<Utc>>,
        pub origin: ReportOrigin,
        pub report_type: ReportType,
        pub games: Vec<GameId>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct CreateReport {
        pub user_id: UserId,
        pub origin: ReportOrigin,
        pub report_type: ReportType,
        pub games: Vec<GameId>,
    }

    impl From<CreateReport> for Report {
        fn from(report: CreateReport) -> Report {
            Report {
                _id: ObjectId::new(),
                user_id: report.user_id,
                origin: report.origin,
                report_type: report.report_type,
                games: report.games,
                date_requested: Utc::now(),
                date_completed: None
            }
        }
    }

    impl From<CreateReport> for Document {
        fn from(report: CreateReport) -> Document {
            let report: Report = report.into();
            to_document(&report)
                .expect("should never fail")
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, strum_macros::ToString)]
    #[serde(rename_all = "lowercase")]
    pub enum AnalysisType {
        Fishnet,
        Deep,
    }

    impl From<AnalysisType> for Bson {
        fn from(at: AnalysisType) -> Bson {
            Bson::String(at.to_string().to_lowercase())
        }
    }


    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct FishnetJob {
        pub _id: ObjectId,
        pub game_id: GameId,
        pub analysis_type: AnalysisType,
        pub precedence: i64,
        pub owner: Option<String>, // TODO: this should be the key from the database
        pub date_last_updated: DateTime<Utc>,
    }


    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct CreateFishnetJob {
        pub game_id: GameId,
        pub analysis_type: AnalysisType,
        pub report_origin: Option<ReportOrigin>,
    }

    impl From<CreateFishnetJob> for FishnetJob {
        fn from(job: CreateFishnetJob) -> FishnetJob {
            FishnetJob {
                _id: ObjectId::new(),
                game_id: job.game_id,
                analysis_type: job.analysis_type,
                precedence: job.report_origin.map(precedence_for_origin).unwrap_or(100_i64),
                owner: None,
                date_last_updated: Utc::now(),
            }
        }
    }

    impl From<CreateFishnetJob> for Document {
        fn from(job: CreateFishnetJob) -> Document {
            let job: FishnetJob = job.into();
            to_document(&job)
                .expect("should never fail")
        }
    }



    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Blurs {
        pub nb: i64,
        pub bits: String, // TODO: why string?!
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Eval {
        pub cp: Option<i64>,
        pub mate: Option<i64>,
    }

    // TODO: this should come directly from the lila db, why store this more than once?
    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Game {
        pub _id: ObjectId, // TODO: I couldn't figure out how to make this directly the GameId
        pub game_id: GameId,
        pub emts: Vec<i64>, // TODO: maybe a smaller datatype is more appropriate? u32? u16?
        pub pgn: String,
        pub black: Option<UserId>,
        pub white: Option<UserId>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct CreateGame {
        pub game_id: GameId,
        pub emts: Vec<i64>, // TODO: maybe a smaller datatype is more appropriate? u32? u16?
        pub pgn: String,
        pub black: Option<UserId>,
        pub white: Option<UserId>,
    }

    impl From<CreateGame> for Game {
        fn from(g: CreateGame) -> Game {
            Game {
                _id: ObjectId::new(),
                game_id: g.game_id,
                emts: g.emts,
                pgn: g.pgn,
                black: g.black,
                white: g.white,
            }
        }
    }

    impl From<CreateGame> for Document {
        fn from(game: CreateGame) -> Document {
            let game: Game = game.into();
            to_document(&game)
                .expect("should never fail")
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GameAnalysis {
        pub _id: ObjectId,
        pub game_id: GameId,
        pub analysis: Vec<Eval>, // TODO: we should be able to compress this.
        pub requested_pvs: u8,
        pub requested_depth: Option<i64>,
        pub requested_nodes: Option<i64>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct CreateGameAnalysis {
        pub game_id: GameId,
        pub analysis: Vec<Eval>,
        pub requested_pvs: u8,
        pub requested_depth: Option<i64>,
        pub requested_nodes: Option<i64>,
    }

    impl From<CreateGameAnalysis> for GameAnalysis {
        fn from(g: CreateGameAnalysis) -> GameAnalysis {
            GameAnalysis {
                _id: ObjectId::new(),
                game_id: g.game_id,
                analysis: g.analysis,
                requested_pvs: g.requested_pvs,
                requested_depth: g.requested_depth,
                requested_nodes: g.requested_nodes,
            }
        }
    }

    impl From<CreateGameAnalysis> for Document {
        fn from(game_analysis: CreateGameAnalysis) -> Document {
            let game_analysis: GameAnalysis = game_analysis.into();
            to_document(&game_analysis)
                .expect("should never fail")
        }
    }
}

pub mod api {
    use mongodb::{
        bson::{doc, to_document, oid::ObjectId},
        options::UpdateOptions,
        results::InsertOneResult,
    };
    use futures::future::Future;

    use crate::db::DbConn;
    use crate::error::{Error, Result};
    use crate::deepq::model;

    fn inserted_object_id(result: InsertOneResult) -> Result<ObjectId> {
        Ok(result.inserted_id.as_object_id().ok_or(Error::CreateError)?.clone())
    }

    pub async fn insert_one_game(db: DbConn, game: model::CreateGame) -> Result<ObjectId> {
        // TODO: because games are unique on their game id, we have to do an upsert
        let games_coll = db.database.collection("deepq_games");
        games_coll.update_one(
            doc!{ "game_id": game.game_id.clone() },
            to_document(&game)?,
            Some(UpdateOptions::builder().upsert(true).build())
        ).await?;
        Ok(
            games_coll
                .find_one(doc!{ "game_id": game.game_id.clone() }, None).await?
                .ok_or(Error::CreateError)?
                .get_object_id("_id")?
                .clone()
        )
    }

    pub fn insert_many_games<T>(db: DbConn, games: T)
        -> impl Iterator<Item=impl Future<Output=Result<ObjectId>>>
        where
            T: Iterator<Item=model::CreateGame> + Clone
    {
        games.clone().map(move |game| insert_one_game(db.clone(), game.clone()))
    }

    pub async fn insert_one_report(db: DbConn, report: model::CreateReport) -> Result<ObjectId> {
        let reports_coll = db.database.collection("deepq_reports");
        inserted_object_id(reports_coll.insert_one(report.into(), None).await?)
    }

    pub async fn insert_one_fishnet_job(db: DbConn, job: model::CreateFishnetJob) -> Result<ObjectId> {
        let fishnet_job_col = db.database.collection("deepq_fishnetjobs");
        inserted_object_id(fishnet_job_col.insert_one(job.into(), None).await?)
    }

    pub fn insert_many_fishnet_jobs<'a, T>(db: DbConn, jobs: &'a T)
        -> impl Iterator<Item=impl Future<Output=Result<ObjectId>>> + 'a
        where
            T: Iterator<Item=&'a model::CreateFishnetJob> + Clone
    {
        jobs.clone().map(move |job| insert_one_fishnet_job(db.clone(), job.clone()))
    }

}
