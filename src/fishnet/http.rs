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

use std::convert::Infallible;
use std::result::Result as StdResult;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none, DisplayFromStr};
use shakmaty::fen::Fen;
use warp::{
    filters::BoxedFilter,
    http, reject,
    reply::{self, Reply},
    Filter, Rejection,
};

use crate::db::DbConn;
use crate::deepq::api::{find_game, starting_position};
use crate::error::{Error, HttpError};
use crate::fishnet::api;
use crate::fishnet::model as m;
use crate::http::{json_object_or_no_content, recover, required_parameter, unauthorized};

// TODO: make this complete for all of the variant types we should support.
#[derive(Serialize, Deserialize, Debug)]
pub enum Variant {
    #[serde(rename = "standard")]
    Standard,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WorkType {
    #[serde(rename = "analysis")]
    Analysis,
    #[serde(rename = "move")]
    Move,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestInfo {
    version: String,
    #[serde(rename = "apikey")]
    api_key: m::Key,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FishnetRequest {
    fishnet: RequestInfo,
}

impl From<FishnetRequest> for m::Key {
    fn from(request: FishnetRequest) -> m::Key {
        request.fishnet.api_key
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AcquireRequest {
    fishnet: RequestInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Nodes {
    nnue: u64,
    classical: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestedAnalysis {
    depth: Option<u8>,
    multipv: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkInfo {
    #[serde(rename = "type")]
    _type: WorkType,
    id: String,
    nodes: Nodes,
    analysis: Option<RequestedAnalysis>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct Job {
    work: WorkInfo,
    game_id: String,
    #[serde_as(as = "DisplayFromStr")]
    position: Fen,
    variant: Variant,
    // TODO: make this a real type as well
    moves: String,

    #[serde(rename = "skipPositions")]
    skip_positions: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum StockfishFlavor {
    Nnue,
    Classical,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StockfishType {
    flavor: StockfishFlavor,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlyScore {
    cp: Option<i32>,
    mate: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SkippedAnalysis {
    skipped: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EmptyAnalysis {
    depth: i32,
    score: PlyScore,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FullAnalysis {
    pv: String, // TODO: better type here?
    depth: i32,
    score: PlyScore,
    time: i32,
    nodes: i32,
    nps: i32,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum PlyAnalysis {
    Skipped(SkippedAnalysis),
    Full(FullAnalysis),
    Empty(EmptyAnalysis),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AnalysisReport {
    fishnet: RequestInfo,
    stockfish: StockfishType,
    analysis: Vec<PlyAnalysis>,
}

#[derive(Debug)]
pub struct HeaderKey(pub m::Key);

impl FromStr for HeaderKey {
    type Err = Error;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        Ok(HeaderKey(m::Key(
            s.strip_prefix("Bearer ")
                .ok_or(HttpError::MalformedHeader)?
                .to_string(),
        )))
    }
}

impl From<HeaderKey> for m::Key {
    fn from(hk: HeaderKey) -> m::Key {
        hk.0
    }
}

async fn api_user_for_key<T>(db: DbConn, key: T) -> StdResult<Option<m::ApiUser>, Rejection>
where
    T: Into<m::Key>,
{
    Ok(api::get_api_user(db, key.into()).await?)
}

/// extract an ApiUser from the json body request
fn api_user_from_body(
    db: DbConn,
) -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Rejection> + Clone {
    warp::any()
        .map(move || db.clone())
        .and(warp::body::json::<FishnetRequest>())
        .and_then(api_user_for_key)
}

/// extract a HeaderKey from the Authorization header
fn extract_key_from_header() -> impl Filter<Extract = (HeaderKey,), Error = Rejection> + Clone {
    warp::any().and(warp::header::<HeaderKey>("authorization"))
}

/// extract an m::ApiUser from the Authorization header
fn api_user_from_header(
    db: DbConn,
) -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Rejection> + Clone {
    warp::any()
        .map(move || db.clone())
        .and(extract_key_from_header())
        .and_then(api_user_for_key)
}

/// extract an m::ApiUser from the Authorization header
fn no_api_user() -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Infallible> + Clone {
    warp::any().map(move || None)
}

// TODO: get this from config or env? or lila? (probably lila, tbh)
fn nodes_for_job(job: &m::Job) -> Nodes {
    match job.analysis_type {
        // TODO: what is the default right now for lila's fishnet queue?
        m::AnalysisType::UserAnalysis => Nodes {
            nnue: 2_250_000_u64,
            classical: 4_050_000_u64,
        },
        m::AnalysisType::SystemAnalysis => Nodes {
            nnue: 2_250_000_u64,
            classical: 4_050_000_u64,
        },
        m::AnalysisType::Deep => Nodes {
            nnue: 2_500_000_u64,
            classical: 4_500_000_u64,
        },
    }
}

// TODO: get this from config or env? or lila? (probably lila, tbh)
fn requested_analysis_for_job(job: &m::Job) -> Option<RequestedAnalysis> {
    match job.analysis_type {
        m::AnalysisType::Deep => Some(RequestedAnalysis {
            // TODO: what is the default that we tend to use for CR?
            depth: None,
            multipv: Some(true),
        }),
        _ => None,
    }
}

// TODO: get this from config or env? or lila? (probably lila, tbh)
fn skip_positions_for_job(job: &m::Job) -> Vec<u8> {
    match job.analysis_type {
        // TODO: what is the default right now for lila's fishnet queue?
        m::AnalysisType::UserAnalysis => vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        m::AnalysisType::SystemAnalysis => vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        m::AnalysisType::Deep => Vec::new(),
    }
}

async fn acquire_job(db: DbConn, api_user: m::ApiUser) -> StdResult<Option<Job>, Rejection> {
    // TODO: don't give someone a new job if they already have one!
    //       or just abort their old job?
    // NOTE: not using .map because of unstable async lambdas
    Ok(match api::assign_job(db.clone(), api_user.clone()).await? {
        Some(job) => {
            let game = match find_game(db.clone(), job.game_id.clone()).await {
                Ok(game) => Ok(game),
                Err(err) => {
                    api::unassign_job(db.clone(), api_user, job._id.clone()).await?;
                    Err(err)
                }
            }?;
            match game {
                None => {
                    api::delete_job(db.clone(), job._id).await?;
                    // TODO: I don't yet understand recursion in an async function in Rust.
                    None // acquire_job(db.clone(), api_user.clone())?
                }
                Some(game) => Some(Job {
                    game_id: job.game_id.to_string(),
                    position: starting_position(game.clone()),
                    variant: Variant::Standard,
                    skip_positions: skip_positions_for_job(&job),
                    moves: game.pgn,
                    work: WorkInfo {
                        id: job._id.to_string(),
                        _type: WorkType::Analysis,
                        nodes: nodes_for_job(&job),
                        analysis: requested_analysis_for_job(&job),
                    },
                }),
            }
        }
        None => None,
    })
}

async fn abort_job(db: DbConn, api_usr: m::ApiUser, job_id: String) -> StdResult<Option<()>, Rejection> {
    Err(reject::not_found())
}

async fn check_key_validity(db: DbConn, key: String) -> StdResult<String, Rejection> {
    api::get_api_user(db, key.into())
        .await?
        .ok_or_else(reject::not_found)
        .map(|_| String::new())
}

#[skip_serializing_none]
#[derive(Serialize)]
struct FishnetStatus {
    analysis: api::QStatus,
    system: api::QStatus,
    deep: api::QStatus,
    key: Option<api::KeyStatus>,
}

async fn fishnet_status(
    db: DbConn,
    api_user: Option<m::ApiUser>,
) -> StdResult<FishnetStatus, Rejection> {
    let analysis = api::q_status(db.clone(), m::AnalysisType::UserAnalysis).await?;
    let system = api::q_status(db.clone(), m::AnalysisType::SystemAnalysis).await?;
    let deep = api::q_status(db.clone(), m::AnalysisType::Deep).await?;
    let key = api::key_status(api_user.clone());
    Ok(FishnetStatus {
        analysis,
        system,
        deep,
        key,
    })
}

pub fn mount(db: DbConn) -> BoxedFilter<(impl Reply,)> {
    let authorization_possible = warp::any()
        .and(api_user_from_body(db.clone()))
        .or(api_user_from_header(db.clone()))
        .unify()
        .or(no_api_user())
        .unify();

    let authorization_required = required_parameter(authorization_possible.clone(), &unauthorized);

    let db = warp::any().map(move || db.clone());
    let empty = warp::any().map(String::new);

    let acquire = warp::path("acquire")
        .and(warp::filters::method::post())
        .and(db.clone())
        .and(authorization_required.clone())
        .and_then(acquire_job)
        .and_then(json_object_or_no_content::<Job>);

    let abort = warp::path("abort")
        .and(warp::filters::method::post())
        .and(db.clone())
        .and(authorization_required.clone())
        .and(empty)
        .and_then(abort_job)
        .and_then(json_object_or_no_content::<()>);

    let valid_key = warp::path("key")
        .and(warp::filters::method::get())
        .and(db.clone())
        .and(warp::path::param())
        .and_then(check_key_validity);

    let status = warp::path("status")
        .and(warp::filters::method::get())
        .and(db)
        .and(authorization_possible)
        .and_then(fishnet_status)
        .map(|status| {
            Ok(reply::with_status(
                reply::json(&status),
                http::StatusCode::OK,
            ))
        });

    acquire
        .or(abort)
        .or(valid_key)
        .or(status)
        .recover(recover)
        .boxed()
}
