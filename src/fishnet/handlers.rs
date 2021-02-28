// Copyright 2020-2021 Lakin Wecker
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

use std::num::NonZeroU8;
use std::result::Result as StdResult;
use std::convert::{TryFrom, TryInto, Into};

use log::{debug, info, error};
use serde::{Deserialize, Serialize};
use serde_with::{
    serde_as, skip_serializing_none, DisplayFromStr, SpaceSeparator, StringWithSeparator,
};
use shakmaty::{fen::Fen, uci::Uci};
use tokio::sync::broadcast;
use warp::{
    filters::{method, BoxedFilter},
    http, path, reject,
    reply::{self, Reply},
    Filter, Rejection,
};

use super::{api, filters as f, model as m, FishnetMsg};
use crate::db::DbConn;
use crate::deepq::api::{
    find_game, starting_position, upsert_one_game_analysis, UpdateGameAnalysis
};
use crate::deepq::model::{PlyAnalysis, UserId, Nodes as ModelNodes};
use crate::http::{json_object_or_no_content, recover, required_or_unauthenticated, with};
use crate::error::{Error, Result};

// TODO: make this complete for all of the variant types we should support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Variant {
    #[serde(rename = "standard")]
    Standard,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkType {
    #[serde(rename = "analysis")]
    Analysis,
    #[serde(rename = "move")]
    Move,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RequestInfo {
    version: String,
    #[serde(rename = "apikey")]
    api_key: m::Key,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FishnetRequest {
    fishnet: RequestInfo,
}

impl From<FishnetRequest> for m::Key {
    fn from(request: FishnetRequest) -> m::Key {
        request.fishnet.api_key
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AcquireRequest {
    fishnet: RequestInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Nodes {
    nnue: u64,
    classical: u64,
}

impl TryFrom<Nodes> for ModelNodes {
    type Error = Error;

    fn try_from(nodes: Nodes) -> Result<ModelNodes> {
        Ok(ModelNodes{
            nnue: nodes.nnue.try_into()?,
            classical: nodes.classical.try_into()?
        })
    }
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum StockfishFlavor {
    Nnue,
    Classical,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StockfishType {
    flavor: StockfishFlavor,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysisReport {
    fishnet: RequestInfo,
    stockfish: StockfishType,
    analysis: Vec<Option<PlyAnalysis>>,
}

impl From<AnalysisReport> for m::Key {
    fn from(report: AnalysisReport) -> m::Key {
        report.fishnet.api_key
    }
}
impl AnalysisReport {
    pub fn is_complete(&self) -> bool {
        self.analysis.iter().filter(|o| o.is_none()).count() == 0_usize
    }
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

fn send(
    tx: broadcast::Sender<FishnetMsg>,
    msg: FishnetMsg
) {
    if let Err(err) = tx.send(msg.clone()) {
        error!("Unable to send msg: {:?} due to: {:?}", msg, err);
    } else {
        debug!("Msg sent: {:?}", msg);
    }
}

async fn acquire_job(
    db: DbConn,
    tx: broadcast::Sender<FishnetMsg>,
    api_user: f::Authorized<m::ApiUser>,
) -> StdResult<Option<Job>, Rejection> {
    let api_user = api_user.val();
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
                    send(
                        tx,
                        FishnetMsg::JobAcquired(job._id.clone())
                    );
                    let job = Job {
                        game_id: job.game_id.to_string(),
                        position: starting_position(game.clone()),
                        variant: Variant::Standard,
                        skip_positions: skip_positions_for_job(&job),
                        moves: game.pgn,
                        work: WorkInfo {
                            id: job._id.to_string(),
                            _type: WorkType::Analysis,
                            nodes: nodes_for_job(&job).try_into()?,
                            multipv: multipv_for_job(&job),
                            depth: depth_for_job(&job),
                        },
                    };
                    Some(job)
                }
            }
        }
        None => None,
    })
}

async fn abort_job(
    db: DbConn,
    tx: broadcast::Sender<FishnetMsg>,
    api_user: f::Authorized<m::ApiUser>,
    job_id: m::JobId,
) -> StdResult<Option<()>, Rejection> {
    let api_user = api_user.val();
    info!("abort_job > {}", api_user.name);
    api::unassign_job(db.clone(), api_user, job_id.clone()).await?;
    send(tx, FishnetMsg::JobAborted(job_id));
    Ok(None) // None because we're going to return no-content
}

/// TODO: Not sure I'm checking to ensure that the job is "done"
/// TODO: Need to mark job as done if it is done and update report.
async fn save_job_analysis(
    db: DbConn,
    tx: broadcast::Sender<FishnetMsg>,
    api_user: f::Authorized<m::ApiUser>,
    job_id: m::JobId,
    report: AnalysisReport,
) -> StdResult<Option<Job>, Rejection> {
    let api_user = api_user.val();
    info!("save_job_analysis > {:?} > {:?}", api_user.name, job_id);

    let job = api::get_user_job(db.clone(), job_id.clone().into(), api_user.clone())
        .await?
        .ok_or(reject::not_found())?;
    debug!("save_job_analysis > get_user_job > success");

    let analysis = UpdateGameAnalysis {
        job_id: job_id.into(),
        game_id: job.clone().game_id.into(),
        analysis: report.analysis.clone(),
        source_id: UserId(api_user._id.to_string()),
        requested_pvs: multipv_for_job(&job).map(|v| i32::from(v.get())),
        requested_depth: depth_for_job(&job).map(Into::into),
        requested_nodes: nodes_for_job(&job).try_into()?,
    };
    debug!("save_job_analysis > created UpdateGameAnalysis");
    upsert_one_game_analysis(db.clone(), analysis).await?;
    debug!("save_job_analysis > upsert_one_game_analysis > success");
    if report.is_complete() {
        debug!("save_job_analysis > JobCompleted");
        api::set_complete(db, job._id.clone()).await?;
        send(tx, FishnetMsg::JobCompleted(job._id.clone()));
    }
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

fn _log_body() -> impl Filter<Extract = (), Error = Rejection> + Copy {
    warp::body::bytes()
        .map(|b: warp::hyper::body::Bytes| {
            println!("Request body: {:?}", b);
        })
        .untuple_one()
}

pub fn mount(db: DbConn, tx: broadcast::Sender<FishnetMsg>) -> BoxedFilter<(impl Reply,)> {
    let authenticated = f::api_user_from_header(db.clone());
    let authentication_required = authenticated.clone().and_then(required_or_unauthenticated);

    let header_authorization_required = warp::any()
        .and(with(db.clone()))
        .and(authentication_required.clone())
        .and_then(f::authorize);

    // NOTE: this supports the old fishnet 1.x style of authorization
    //       which I am not going to worry about supporting out of the box.
    //let authorized_api_user = warp::any()
    //.and(header_authorization_required)
    //.or(f::authorized_json_body(db.clone())
    //.map(|fr: f::Authorized<FishnetRequest>| fr.clone().map(|_| fr.api_user())))
    //.unify();

    let acquire = path("acquire")
        .and(method::post())
        .and(with(db.clone()))
        .and(with(tx.clone()))
        .and(header_authorization_required.clone())
        .and_then(acquire_job)
        .and_then(json_object_or_no_content::<Job>);

    let abort = path("abort")
        .and(method::post())
        .and(with(db.clone()))
        .and(with(tx.clone()))
        .and(header_authorization_required.clone())
        .and(path::param())
        .and_then(abort_job)
        .and_then(json_object_or_no_content::<()>);

    let analysis = path("analysis")
        .and(method::post())
        .and(with(db.clone()))
        .and(with(tx.clone()))
        .and(header_authorization_required.clone())
        .and(path::param())
        .and(warp::body::json())
        .and_then(save_job_analysis)
        .and_then(json_object_or_no_content::<Job>);

    let valid_key = path("key")
        .and(method::get())
        .and(with(db.clone()))
        .and(path::param())
        .and_then(check_key_validity);

    let status = path("status")
        .and(method::get())
        .and(with(db.clone()))
        .and(f::authentication_from_header(db))
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
