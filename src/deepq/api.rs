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

use chrono::prelude::*;
use futures::future::Future;
use log::debug;
use mongodb::{
    bson::{doc, from_document, oid::ObjectId, to_document, Bson, DateTime as BsonDateTime},
    options::UpdateOptions,
};
use shakmaty::{fen::Fen, uci::Uci};

use crate::db::DbConn;
use crate::deepq::model as m;
use crate::error::Result;

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

pub async fn insert_one_report(db: DbConn, report: CreateReport) -> Result<Bson> {
    let reports_coll = m::Report::coll(db.clone());
    let report: m::Report = report.into();
    Ok(reports_coll
        .insert_one(to_document(&report)?, None)
        .await?
        .inserted_id)
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
    // NOTE: I am purposefully renaming this here, from _id.
    //       Maybe I'll regret it later
    pub game_id: m::GameId,
    pub emts: Vec<i32>,
    pub pgn: Vec<Uci>,
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

pub async fn insert_one_game(db: DbConn, game: CreateGame) -> Result<m::GameId> {
    // NOTE: because games are unique on their game id, we have to do an upsert
    let game: m::Game = game.into();
    debug!("Insert One Game: {:?}", game);
    let games_coll = m::Game::coll(db.clone());
    let result = games_coll
        .update_one(
            doc! { "_id": game._id.clone() },
            to_document(&game)?,
            Some(UpdateOptions::builder().upsert(true).build()),
        )
        .await?;
    debug!("Result: {:?}", result);
    Ok(game._id)
}

pub fn insert_many_games<T>(
    db: DbConn,
    games: T,
) -> impl Iterator<Item = impl Future<Output = Result<m::GameId>>>
where
    T: Iterator<Item = CreateGame> + Clone,
{
    debug!("Insert Many Games");
    games.map(move |game| insert_one_game(db.clone(), game))
}

pub async fn find_game(db: DbConn, game_id: m::GameId) -> Result<Option<m::Game>> {
    let games_coll = db.database.collection("deepq_games");
    Ok(games_coll
        .find_one(doc! {"_id": game_id}, None)
        .await?
        .map(from_document)
        .transpose()?)
}

#[derive(Debug, Clone)]
pub struct UpdateGameAnalysis {
    pub job_id: ObjectId,
    pub game_id: m::GameId,
    pub source_id: m::UserId,
    pub analysis: Vec<Option<m::PlyAnalysis>>,
    pub requested_pvs: Option<i32>,
    pub requested_depth: Option<i32>,
    pub requested_nodes: m::Nodes,
}

impl From<UpdateGameAnalysis> for m::GameAnalysis {
    fn from(g: UpdateGameAnalysis) -> m::GameAnalysis {
        m::GameAnalysis {
            _id: ObjectId::new(),
            job_id: g.job_id,
            game_id: g.game_id,
            source_id: g.source_id,
            analysis: g.analysis,
            requested_pvs: g.requested_pvs,
            requested_depth: g.requested_depth,
            requested_nodes: g.requested_nodes,
        }
    }
}

pub async fn upsert_one_game_analysis(
    db: DbConn, analysis: UpdateGameAnalysis
) -> Result<ObjectId> {
    let analysis_coll = m::GameAnalysis::coll(db.clone());
    let analysis: m::GameAnalysis = analysis.into();
    let result = analysis_coll
        .update_one(
            doc! { "_id": analysis._id.clone() },
            to_document(&analysis)?,
            Some(UpdateOptions::builder().upsert(true).build()),
        )
        .await?;
    debug!("Result: {:?}", result);
    Ok(analysis._id)
}
