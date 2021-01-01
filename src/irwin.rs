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
//
//

use std::str::FromStr;
use std::io::{Error as IoError, ErrorKind};
use std::result::{Result as StdResult};
use std::iter::Iterator;

// use log::debug;
use futures::{future::{self, try_join_all}, stream::{Stream, StreamExt}};
use tokio::{
    io::{stream_reader, AsyncBufReadExt},
    time::Duration
};

use serde::{Deserialize, Serialize};

use crate::db::DbConn;
use crate::deepq::api::{
    insert_many_games, insert_one_report, precedence_for_origin, CreateGame, CreateReport,
};
use crate::deepq::model::{Eval, GameId, ReportOrigin, ReportType, UserId};
use crate::error::{Error, Result};
use crate::fishnet::api::{insert_many_jobs, CreateJob};
use crate::fishnet::model::AnalysisType;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub id: UserId,
    pub titled: bool,
    pub engine: bool,
    pub games: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Game {
    pub id: GameId,
    pub white: UserId,
    pub black: UserId,
    pub emts: Option<Vec<i32>>,
    pub pgn: String, // TODO: this should be more strongly typed.
    pub analysis: Option<Vec<Eval>>,
}

impl From<&Game> for CreateGame {
    fn from(g: &Game) -> CreateGame {
        let g = g.clone();
        CreateGame {
            game_id: g.id,
            emts: g.emts.unwrap_or_else(Vec::new),
            pgn: g.pgn,
            black: Some(g.black),
            white: Some(g.white),
        }
    }
}

// TODO: Consider using an enum for the Request/KeepAlive pair here.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Request {
    pub t: String,
    pub origin: ReportOrigin,
    pub user: User,
    pub games: Vec<Game>,
}

impl From<Request> for CreateReport {
    fn from(request: Request) -> CreateReport {
        CreateReport {
            user_id: request.user.id,
            origin: request.origin,
            report_type: ReportType::Irwin,
            games: request.games.iter().map(|g| g.id.clone()).collect(),
        }
    }
}

impl From<Request> for Vec<CreateJob> {
    fn from(request: Request) -> Vec<CreateJob> {
        request
            .games
            .iter()
            .map(|g| CreateJob {
                game_id: g.id.clone(),
                analysis_type: AnalysisType::Deep,
                precedence: precedence_for_origin(request.clone().origin),
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeepAlive {
    #[serde(rename = "keepAlive")]
    pub keep_alive: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamMsg {
    KeepAlive(KeepAlive),
    Request(Request),
}

impl FromStr for StreamMsg {
    type Err = Error;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        Ok(serde_json::from_str(&s)?)
    }
}

pub async fn add_to_queue(db: DbConn, request: Request) -> Result<()> {
    try_join_all(insert_many_games(
        db.clone(),
        request.games.iter().map(Into::into),
    ))
    .await?;
    let fishnet_jobs: Vec<CreateJob> = request.clone().into();
    try_join_all(insert_many_jobs(db.clone(), fishnet_jobs.iter().by_ref())).await?;
    insert_one_report(db.clone(), request.into()).await?;
    Ok(())
}

pub async fn stream(
    url: &str,
    api_key: &str,
) -> Result<impl Stream<Item = Result<StreamMsg>>> {
    let client = reqwest::Client::builder()
        .tcp_keepalive(Duration::from_millis(1000))
        .build()?;
    let response = client
        .get(url)
        .header("User-Agent", "lila-deepq")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;
    Ok(stream_reader(
        response
            .bytes_stream()
            .map(|i| i.map_err(|e| IoError::new(ErrorKind::Other, e))),
    ).lines()
        .filter(|line| future::ready(!line.as_ref().unwrap().is_empty()))
        .map(|line| FromStr::from_str(&(line?))))
}
