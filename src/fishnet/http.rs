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
use std::num::NonZeroU8;
use std::result::Result as StdResult;
use std::str::FromStr;

use log::{debug, info};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::{
    serde_as, skip_serializing_none, DisplayFromStr, SpaceSeparator, StringWithSeparator,
};
use shakmaty::{fen::Fen, uci::Uci};
use warp::{
    filters::{method, BoxedFilter},
    http, path, reject,
    reply::{self, Reply},
    Filter, Rejection,
};

use crate::db::DbConn;
use crate::deepq::api::{find_game, starting_position};
use crate::error::{Error, HttpError};
use crate::fishnet::api;
use crate::fishnet::model as m;
use crate::http::{
    json_object_or_no_content, recover, required_parameter, unauthenticated, with_db, Id,
};

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

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct WorkInfo {
    #[serde(rename = "type")]
    _type: WorkType,
    id: String,
    nodes: Nodes,
    depth: Option<u8>,
    multipv: Option<NonZeroU8>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct Job {
    work: WorkInfo,
    game_id: String,
    #[serde_as(as = "DisplayFromStr")]
    position: Fen,
    variant: Variant,
    #[serde_as(as = "StringWithSeparator::<SpaceSeparator, Uci>")]
    moves: Vec<Uci>,

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

impl From<AnalysisReport> for m::Key {
    fn from(report: AnalysisReport) -> m::Key {
        report.fishnet.api_key
    }
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

async fn authorized_from_api_user(api_user: m::ApiUser) -> StdResult<m::ApiUser, Rejection> {
    // TODO: This will need to verify the key hasn't been deactivated
    Ok(api_user)
}

async fn api_user_from_key<T>(
    db: DbConn,
    payload_with_key: T,
) -> StdResult<Option<m::ApiUser>, Rejection>
where
    T: Into<m::Key>,
{
    Ok(api::get_api_user(db, payload_with_key.into()).await?)
}

async fn api_user_with_payload_from_payload_key<T>(
    db: DbConn,
    payload_with_key: T,
) -> StdResult<(Option<m::ApiUser>, T), Rejection>
where
    T: Into<m::Key> + Clone,
{
    Ok((
        api::get_api_user(db, payload_with_key.clone().into()).await?,
        payload_with_key,
    ))
}

fn authentication_from_body_with_payload<T>(
    db: DbConn,
) -> impl Filter<Extract = ((Option<m::ApiUser>, T),), Error = Rejection> + Clone + Send + Sync
where
    T: Into<m::Key> + DeserializeOwned + Clone + Send + Sync,
{
    warp::any()
        .map(move || db.clone())
        .and(warp::body::json::<T>())
        .and_then(api_user_with_payload_from_payload_key)
}

fn authorized_from_body_with_payload<T>(
    db: DbConn,
) -> impl Filter<Extract = ((m::ApiUser, T),), Error = Rejection> + Clone + Send + Sync
where
    T: Into<m::Key> + DeserializeOwned + Clone + Send + Sync,
{
    required_parameter(authentication_from_body_with_payload(db), &unauthenticated)
}

fn extract_key_from_header() -> impl Filter<Extract = (HeaderKey,), Error = Rejection> + Clone {
    warp::any().and(warp::header::<HeaderKey>("authorization"))
}

fn api_user_from_header(
    db: DbConn,
) -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Rejection> + Clone {
    warp::any()
        .map(move || db.clone())
        .and(extract_key_from_header())
        .and_then(api_user_from_key)
}

fn no_api_user() -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Infallible> + Clone {
    warp::any().map(move || None)
}

fn authentication_from_header(
    db: DbConn,
) -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Infallible> + Clone {
    warp::any()
        .and(api_user_from_header(db.clone()))
        .or(no_api_user())
        .unify()
}

fn authorized_from_header(
    db: DbConn,
) -> impl Filter<Extract = (m::ApiUser,), Error = Rejection> + Clone {
    required_parameter(authentication_from_header(db), &unauthenticated)
        .and_then(authorized_from_api_user)
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
fn multipv_for_job(job: &m::Job) -> Option<NonZeroU8> {
    match job.analysis_type {
        m::AnalysisType::Deep => NonZeroU8::new(5u8),
        _ => None,
    }
}

fn depth_for_job(_job: &m::Job) -> Option<u8> {
    // TODO: Currently none of them request a specific depth, I thought they did?
    None
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
    info!("acquire_job > {}", api_user.name);
    // TODO: Multiple active jobs are allowed. Instead we should unassign old ones that
    //       are not finished.
    // NOTE: not using .map because of unstable async lambdas
    debug!("start");
    Ok(match api::assign_job(db.clone(), api_user.clone()).await? {
        Some(job) => {
            debug!("Some(job) = {:?}", job);
            let game = match find_game(db.clone(), job.game_id.clone()).await {
                Ok(game) => Ok(game),
                Err(err) => {
                    api::unassign_job(db.clone(), api_user, job._id.clone()).await?;
                    Err(err)
                }
            }?;
            match game {
                None => {
                    debug!("No game for game_id: {:?}", job.game_id);
                    api::delete_job(db.clone(), job._id).await?;
                    // TODO: I don't yet understand recursion in an async function in Rust.
                    None // acquire_job(db.clone(), api_user.clone())?
                }
                Some(game) => {
                    debug!("Some(game) = {:?}", game);
                    Some(Job {
                        game_id: job.game_id.to_string(),
                        position: starting_position(game.clone()),
                        variant: Variant::Standard,
                        skip_positions: skip_positions_for_job(&job),
                        moves: game.pgn,
                        work: WorkInfo {
                            id: job._id.to_string(),
                            _type: WorkType::Analysis,
                            nodes: nodes_for_job(&job),
                            multipv: multipv_for_job(&job),
                            depth: depth_for_job(&job),
                        },
                    })
                }
            }
        }
        None => None,
    })
}

async fn abort_job(
    db: DbConn,
    api_user: m::ApiUser,
    job_id: Id,
) -> StdResult<Option<()>, Rejection> {
    info!("abort_job > {}", api_user.name);
    api::unassign_job(db.clone(), api_user, job_id.into()).await?;
    Ok(None) // None because we're going to return no-content
}

async fn save_job_analysis(
    db: DbConn,
    api_user: m::ApiUser,
    job_id: Id,
    analysis: AnalysisReport,
) -> StdResult<Option<Job>, Rejection> {
    info!("save_job_analysis");
    debug!("AnalysisReport: {:?}", analysis);
    Ok(None)
}

async fn check_key_validity(db: DbConn, key: String) -> StdResult<String, Rejection> {
    api::get_api_user(db, key.into())
        .await?
        .ok_or_else(reject::not_found)
        .map(|_| String::new())
}

#[derive(Serialize)]
struct FishnetAnalysisStatus {
    user: api::QStatus,
    system: api::QStatus,
    deep: api::QStatus,
}

#[skip_serializing_none]
#[derive(Serialize)]
struct FishnetStatus {
    analysis: FishnetAnalysisStatus,
    key: Option<api::KeyStatus>,
}

async fn fishnet_status(
    db: DbConn,
    api_user: Option<m::ApiUser>,
) -> StdResult<FishnetStatus, Rejection> {
    info!("status");
    let user = api::q_status(db.clone(), m::AnalysisType::UserAnalysis).await?;
    let system = api::q_status(db.clone(), m::AnalysisType::SystemAnalysis).await?;
    let deep = api::q_status(db.clone(), m::AnalysisType::Deep).await?;
    let key = api::key_status(api_user.clone());
    let analysis = FishnetAnalysisStatus { user, system, deep };
    Ok(FishnetStatus { analysis, key })
}

pub fn mount(db: DbConn) -> BoxedFilter<(impl Reply,)> {
    let acquire = path("acquire")
        .and(method::post())
        .and(with_db(db.clone()))
        .and(authorized_from_header(db.clone()))
        .and_then(acquire_job)
        .and_then(json_object_or_no_content::<Job>);

    let abort = path("abort")
        .and(method::post())
        .and(with_db(db.clone()))
        .and(authorized_from_header(db.clone()))
        .and(path::param())
        .and_then(abort_job)
        .and_then(json_object_or_no_content::<()>);

    let analysis = path("analysis")
        .and(method::post())
        .and(with_db(db.clone()))
        .and(authorized_from_header(db.clone()))
        .and(path::param())
        .and(warp::body::json::<AnalysisReport>())
        .and_then(save_job_analysis)
        .and_then(json_object_or_no_content::<Job>);

    let valid_key = path("key")
        .and(method::get())
        .and(with_db(db.clone()))
        .and(path::param())
        .and_then(check_key_validity);

    let status = path("status")
        .and(method::get())
        .and(with_db(db.clone()))
        .and(authentication_from_header(db))
        .and_then(fishnet_status)
        .map(|status| {
            Ok(reply::with_status(
                reply::json(&status),
                http::StatusCode::OK,
            ))
        });

    acquire
        .or(abort)
        .or(analysis)
        .or(valid_key)
        .or(status)
        .recover(recover)
        .boxed()
}
