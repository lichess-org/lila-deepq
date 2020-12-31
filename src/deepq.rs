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
}

pub mod api {
    use chrono::prelude::*;
    use futures::future::Future;
    use mongodb::{
        bson::{doc, from_document, oid::ObjectId, to_document, Bson, DateTime as BsonDateTime},
        options::UpdateOptions,
    };
    use shakmaty::fen::Fen;

    use crate::db::DbConn;
    use crate::deepq::model as m;
    use crate::error::{Error, Result};

    #[derive(Debug, Clone)]
    pub struct CreateReport {
        pub user_id: m::UserId,
        pub origin: m::ReportOrigin,
        pub report_type: m::ReportType,
        pub games: Vec<m::GameId>,
    }

    impl From<CreateReport> for m::Report {
        fn from(report: CreateReport) -> m::Report {
            m::Report {
                _id: ObjectId::new(),
                user_id: report.user_id,
                origin: report.origin,
                report_type: report.report_type,
                games: report.games,
                date_requested: BsonDateTime(Utc::now()),
                date_completed: None,
            }
        }
    }

    pub fn precedence_for_origin(origin: m::ReportOrigin) -> i32 {
        match origin {
            m::ReportOrigin::Moderator => 1_000_000i32,
            m::ReportOrigin::Leaderboard => 1000i32,
            m::ReportOrigin::Tournament => 100i32,
            m::ReportOrigin::Random => 10i32,
        }
    }

    pub fn starting_position(_game: m::Game) -> Fen {
        // TODO: this will eventually need to be smarter, but not for v1
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .expect("this cannot fail")
    }

    #[derive(Debug, Clone)]
    pub struct CreateGame {
        pub game_id: m::GameId, // NOTE: I am purposefully renaming this here, from _id.
        //       Maybe I'll regret it later
        pub emts: Vec<i32>,
        pub pgn: String,
        pub black: Option<m::UserId>,
        pub white: Option<m::UserId>,
    }

    impl From<CreateGame> for m::Game {
        fn from(g: CreateGame) -> m::Game {
            m::Game {
                _id: g.game_id,
                emts: g.emts,
                pgn: g.pgn,
                black: g.black,
                white: g.white,
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct CreateGameAnalysis {
        pub game_id: m::GameId,
        pub analysis: Vec<m::Eval>,
        pub requested_pvs: u8,
        pub requested_depth: Option<i32>,
        pub requested_nodes: Option<i32>,
    }

    impl From<CreateGameAnalysis> for m::GameAnalysis {
        fn from(g: CreateGameAnalysis) -> m::GameAnalysis {
            m::GameAnalysis {
                _id: ObjectId::new(),
                game_id: g.game_id,
                analysis: g.analysis,
                requested_pvs: g.requested_pvs,
                requested_depth: g.requested_depth,
                requested_nodes: g.requested_nodes,
            }
        }
    }

    pub async fn insert_one_game(db: DbConn, game: CreateGame) -> Result<Bson> {
        // NOTE: because games are unique on their game id, we have to do an upsert
        let game: m::Game = game.into();
        let games_coll = db.database.collection("deepq_games");
        games_coll
            .update_one(
                doc! { "_id": game._id.clone() },
                to_document(&game)?,
                Some(UpdateOptions::builder().upsert(true).build()),
            )
            .await?;
        Ok(games_coll
            .find_one(doc! { "_id": game._id.clone() }, None)
            .await?
            .ok_or(Error::CreateError)?
            .get("_id")
            .ok_or(Error::CreateError)?
            .clone())
    }

    pub fn insert_many_games<T>(
        db: DbConn,
        games: T,
    ) -> impl Iterator<Item = impl Future<Output = Result<Bson>>>
    where
        T: Iterator<Item = CreateGame> + Clone,
    {
        games
            .clone()
            .map(move |game| insert_one_game(db.clone(), game.clone()))
    }

    pub async fn find_game(db: DbConn, game_id: m::GameId) -> Result<Option<m::Game>> {
        let games_coll = db.database.collection("deepq_games");
        Ok(games_coll
            .find_one(doc! {"_id": game_id}, None)
            .await?
            .map(from_document)
            .transpose()?)
    }

    pub async fn insert_one_report(db: DbConn, report: CreateReport) -> Result<Bson> {
        let reports_coll = db.database.collection("deepq_reports");
        let report: m::Report = report.into();
        Ok(reports_coll
            .insert_one(to_document(&report)?, None)
            .await?
            .inserted_id)
    }
}
