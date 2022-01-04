//
// Copyright 2021 Lakin Wecker
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

use std::convert::{TryFrom, TryInto};
use std::iter::Iterator;
use std::result::Result as StdResult;

use derive_more::{Display, From};
use futures::{future::try_join_all, stream::StreamExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, SpaceSeparator, StringWithSeparator};
use serde_json;
use shakmaty::{san::San, uci::Uci, CastlingMode, Chess, Position};
use tokio::sync::broadcast::{self, error::RecvError};

use crate::db::DbConn;
use crate::deepq::api::{
    atomically_update_sent_to_irwin, find_report, insert_many_games, insert_one_report,
    precedence_for_origin, CreateGame, CreateReport,
};
use crate::deepq::model::{
    Game, GameAnalysis, GameId, PlyAnalysis, Report, ReportOrigin, ReportType, Score, UserId,
};
use crate::error::{Error, Result};
use crate::fishnet::api::{get_job, insert_many_jobs};
use crate::fishnet::model::{AnalysisType, Job as FishnetJob, CreateJob, JobId};
use crate::fishnet::FishnetMsg;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub id: UserId,
    pub titled: bool,
    pub engine: bool,
    pub games: i32,
}

// This game is the incoming irwin report from lichess, not the
// format that irwin uses internally.  For that see below
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RequestGame {
    pub id: GameId,
    pub white: UserId,
    pub black: UserId,
    pub emts: Option<Vec<i32>>,

    #[serde_as(as = "StringWithSeparator::<SpaceSeparator, San>")]
    pub pgn: Vec<San>,
    pub analysis: Option<Vec<Score>>,
}

fn uci_from_san(pgn: &[San]) -> Result<Vec<Uci>> {
    let mut pos = Chess::default();
    let mut ret_val = Vec::new();
    for san in pgn.iter() {
        let m = san.to_move(&pos)?;
        // TODO: the castling mode needs to come from the game!!
        ret_val.push(Uci::from_move(&m, CastlingMode::Standard));
        pos = pos.play(&m).map_err(|_pos| Error::PositionError)?;
    }
    Ok(ret_val)
}

impl TryFrom<&RequestGame> for CreateGame {
    type Error = Error;

    fn try_from(g: &RequestGame) -> StdResult<CreateGame, Self::Error> {
        let g = g.clone();
        Ok(CreateGame {
            game_id: g.id,
            emts: g.emts.unwrap_or_else(Vec::new),
            pgn: uci_from_san(&g.pgn)?,
            black: Some(g.black),
            white: Some(g.white),
        })
    }
}

// TODO: Consider using an enum for the Request/KeepAlive pair here.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Request {
    pub t: String,
    pub origin: ReportOrigin,
    pub user: User,
    pub games: Vec<RequestGame>,
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
                report_id: None,
                analysis_type: AnalysisType::Deep,
                precedence: precedence_for_origin(request.clone().origin),
            })
            .collect()
    }
}

pub async fn add_to_queue(db: DbConn, request: Request) -> Result<()> {
    let p = "irwin_add_to_queue >";
    let games_with_uci = request
        .games
        .iter()
        .map(TryInto::try_into)
        .collect::<Result<Vec<CreateGame>>>()?;
    debug!("{} try insert_many_games({})", p, request.games.len());
    try_join_all(insert_many_games(
        db.clone(),
        games_with_uci.iter().cloned(),
    ))
    .await?;
    debug!("done");

    let report_id = insert_one_report(db.clone(), request.clone().into()).await?;

    let fishnet_jobs: Vec<CreateJob> = request.into();
    let fishnet_jobs: Vec<CreateJob> = fishnet_jobs
        .iter()
        .map(|j: &CreateJob| CreateJob {
            game_id: j.game_id.clone(),
            report_id: Some(report_id.clone()),
            analysis_type: j.analysis_type.clone(),
            precedence: j.precedence,
        })
        .collect();

    try_join_all(insert_many_jobs(db.clone(), fishnet_jobs.iter().by_ref())).await?;
    Ok(())
}

#[derive(Serialize, Debug, Clone, From, Display)]
pub struct Key(pub String);

#[derive(Debug, Clone)]
pub struct IrwinOpts {
    pub uri: String,
    pub api_key: Key,
}

// This is a custom set of structs to represent the job we're submitting to irwin.
//
// I am not re-using the pre-existing structs from fishnet, because I don't want
// to couple them.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct EngineEval {
    // TODO: Don't know if this is large enough or too large
    #[serde(skip_serializing_if = "Option::is_none")]
    cp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mate: Option<i64>,
}

impl EngineEval {
    pub fn flip(&self) -> EngineEval {
        EngineEval{
            cp: self.cp.map(|cp| -cp),
            mate: self.mate.map(|mate| -mate)
        }
    }
}

impl From<Score> for EngineEval {
    fn from(s: Score) -> EngineEval {
        match s {
            Score::Cp(cp) => EngineEval {
                cp: Some(cp as i64),
                mate: None,
            },
            Score::Mate(m) => EngineEval {
                cp: None,
                mate: Some(m as i64),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Analysis {
    uci: String,
    #[serde(rename = "engineEval")]
    engine_eval: EngineEval,
}

impl Analysis {
    fn from_ply_analysis(uci: &Uci, ply_analysis: &PlyAnalysis, flip: bool) -> Result<Analysis> {
        match ply_analysis {
            PlyAnalysis::Best(m) => Ok(Analysis {
                uci: uci.to_string(),
                engine_eval: m.score.clone().into(),
            }),
            PlyAnalysis::Matrix(m) => {
                match m
                    .score
                    .iter()
                    .find(|d| d.iter().flatten().count() > 0)
                    .map(|pvs| pvs.iter().flatten().last()) // but the last depth.
                    .flatten()
                {
                    Some(s) => {
                        let engine_eval: EngineEval = s.clone().into();
                        Ok(Analysis {
                            uci: uci.to_string(),
                            engine_eval: if flip { engine_eval.flip() } else { engine_eval },
                        })
                    },
                    None => Err(Error::IncompleteIrwinAnalysis),
                }
            }
            _ => Err(Error::IncompleteIrwinAnalysis),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnalyzedPosition {
    id: String, // The zobrist hash
    analyses: Vec<Analysis>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct IrwinGame {
    #[serde(rename = "_id")]
    id: String,
    white: String,
    black: String,
    pgn: Vec<String>,
    emts: Option<Vec<i32>>,
    analysis: Option<Vec<EngineEval>>,
    analysed: bool
}

impl TryFrom<Game> for IrwinGame {
    type Error = Error;

    fn try_from(game: Game) -> StdResult<IrwinGame, Self::Error> {
        let game = game;

        let mut sans: Vec<String> = Vec::new();
        let mut pos = Chess::default();
        for uci in game.clone().pgn {
            let m = uci.to_move(&pos.clone())?;
            pos = pos.play(&m)?;
            sans.push(San::from_move(&pos, &m).to_string());
        }
        Ok(IrwinGame {
            id: game._id.0,
            white: game.white.map(|p| p.0).unwrap_or_else(|| "Unknown (white)".into()),
            black: game.black.map(|p| p.0).unwrap_or_else(|| "Unknown (black)".into()),
            pgn: sans,
            emts: Some(game.emts),
            analysis: None,
            analysed: false
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct IrwinJob {
    #[serde(rename = "playerId")]
    player_id: String, // The zobrist hash
    games: Vec<IrwinGame>,
    #[serde(rename = "analysedPositions")]
    analyzed_positions: Vec<AnalyzedPosition>,
}

async fn ok_or_warn<S>(r: Result<S>) -> Option<S> {
    match r {
        Err(e) => {
            warn!("Error parsing stream element: {:?}", e);
            None
        }
        Ok(s) => Some(s),
    }
}

async fn irwin_job_from_report(db: DbConn, report: Report) -> Result<IrwinJob> {
    let p = "irwin_job_from_report >";
    let jobs: Vec<FishnetJob> = FishnetJob::find_by_report(db.clone(), report._id.clone())
        .await?
        .filter_map(ok_or_warn)
        .collect()
        .await;
    info!("{} got fishnet job", p);
    // TODO: Theoretically we might have more than one analysis
    //       per game from the way the database structure is setup.
    //       I believe that the code is organized in such a way that
    //       this will not be possible _right_ now, but something to
    //       keep in mind.
    let analyzed_games = GameAnalysis::find_by_jobs(db.clone(), jobs.iter().map(|j| j._id.clone()).collect())
            .await?;
    let analyzed_games = analyzed_games
            .filter_map(ok_or_warn);
    let analyzed_games = analyzed_games.collect();
    let analyzed_games: Vec<GameAnalysis> = analyzed_games.await;
    info!("{} got analysis", p);
    let mut games: Vec<IrwinGame> = Vec::new();
    for game_analysis in analyzed_games {
        let game = game_analysis.game(db.clone()).await?;

        let mut pos = Chess::default();
        match game {
            None => info!(
                "{} skipping game id {} because we can't find it in the database",
                p, game_analysis.game_id
            ),
            Some(game) => {
                let mut irwin_game: IrwinGame = game.clone().try_into()?;
                let mut irwin_evals: Vec<EngineEval> = Vec::new();

                for (num, (uci, analysis)) in game.pgn.iter().zip(game_analysis.analysis.iter()).enumerate() {
                    match analysis {
                        Some(analysis) => {
                            irwin_evals
                                .push(Analysis::from_ply_analysis(uci, analysis, num%2==1)?.engine_eval);
                            let m = uci.to_move(&pos.clone())?;
                            pos = pos.play(&m)?;
                        }
                        // TODO: Waiting on zobrist hashes from shakmaty
                        // https://github.com/niklasf/shakmaty/issues/40
                        // and https://github.com/niklasf/shakmaty/pull/45
                        None => {
                            return Err(Error::IncompleteIrwinAnalysis);
                        }
                    }
                }
                irwin_game.analysis = Some(irwin_evals);
                irwin_game.analysed = true;
                games.push(irwin_game);
            }
        }
    }

    info!("{} got games", p);

    info!("{} returning irwin job", p);
    Ok(IrwinJob {
        player_id: report.user_id.0,
        games,
        analyzed_positions: Vec::new(), // Irwin doesn't seem to use this. So empty it is.
    })
}

async fn handle_job_acquired(_db: DbConn, _opts: IrwinOpts, job_id: JobId) {
    let p = "handle_job_acquired >";
    debug!("{} Fishnet::JobAcquired({})", p, job_id);
}

async fn handle_job_aborted(_db: DbConn, _opts: IrwinOpts, job_id: JobId) {
    let p = "handle_job_aborted >";
    debug!("{} Fishnet::JobAborted({})", p, job_id);
}

async fn handle_job_completed(db: DbConn, opts: IrwinOpts, job_id: JobId) {
    let p = "handle_job_completed >";
    match get_job(db.clone(), job_id.clone()).await {
        Err(err) => {
            error!(
                "{} Unable find job for {:?}. Error: {:?}",
                p,
                job_id.clone(),
                err
            );
        }
        Ok(None) => {
            error!("{} Unable find job for {:?}.", p, job_id.clone());
        }
        Ok(Some(job)) => {
            if let Some(report_id) = job.report_id {
                match find_report(db.clone(), report_id.clone()).await {
                    Err(err) => {
                        error!(
                            "{} Unable find report for {:?}. Error: {:?}",
                            p,
                            report_id.clone(),
                            err
                        );
                    }
                    Ok(None) => {
                        error!("{} Unable find report for {:?}.", p, report_id.clone());
                    }
                    Ok(Some(report)) => {
                        debug!("{} Fishnet::JobCompleted({}) > handled", p, job_id);
                        match update_report_completeness(db.clone(), opts.clone(), report).await {
                            Ok(_) => {}
                            Err(err) => {
                                error!(
                                    "{} Unable to update report completness for report {:?}. Error: {:?}",
                                    p,
                                    report_id.clone(),
                                    err
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn report_complete_percentage(db: DbConn, report: Report) -> Result<f64> {
    let p = "report_complete_percentage >";
    let mut jobs = FishnetJob::find_by_report(db.clone(), report._id.clone()).await?;
    let mut complete = 0f64;
    let mut incomplete = 0f64;

    while let Some(job_result) = jobs.next().await {
        let is_complete = match job_result {
            Ok(job) => job.is_complete,
            Err(err) => {
                error!(
                    "{} Error retrieving jobs for report: {}. Error: {}",
                    p,
                    report._id.clone(),
                    err
                );
                false
            }
        };
        if is_complete {
            complete += 1f64;
        } else {
            incomplete += 1f64;
        }
    }
    Ok(complete / (complete + incomplete))
}

async fn update_report_completeness(db: DbConn, _opts: IrwinOpts, report: Report) -> Result<()> {
    let p = "update_report_completeness";
    let percentage = report_complete_percentage(db.clone(), report.clone()).await?;
    if percentage >= 1f64 {
        let updated_report =
            atomically_update_sent_to_irwin(db.clone(), report._id.clone()).await?;
        if let Some(updated_report) = updated_report {
            info!(
                "{} > Report({:?}) > complete. Submitting to irwin!",
                &p, updated_report._id
            );

            info!("1");
            let irwin_job: IrwinJob = irwin_job_from_report(db.clone(), report).await?;
            info!("2");
            let j = serde_json::to_string(&irwin_job)?;
            info!("3");
            info!("{}", j);
            info!("4");

            // TODO: do something with this job?
        } else {
            info!(
                "{} > Report({:?}) > complete. Already submitted to irwin!",
                &p, report._id
            );
        }
    } else {
        info!(
            "{} > Report({:?}) > {:.1}% complete!",
            &p,
            report._id,
            percentage * 100f64
        );
    }
    Ok(())
}

pub async fn fishnet_listener(db: DbConn, opts: IrwinOpts, tx: broadcast::Sender<FishnetMsg>) {
    let p = "fishnet_listener >";
    let mut should_stop: bool = false;
    let mut rx = tx.subscribe();
    while !should_stop {
        let db = db.clone();
        let msg = rx.recv().await;
        debug!("Received message: {:?}", msg);
        if let Ok(msg) = msg {
            if let FishnetMsg::JobAcquired(id) = msg {
                handle_job_acquired(db.clone(), opts.clone(), id.clone()).await;
            } else if let FishnetMsg::JobAborted(id) = msg {
                handle_job_aborted(db.clone(), opts.clone(), id.clone()).await;
            } else if let FishnetMsg::JobCompleted(id) = msg {
                handle_job_completed(db.clone(), opts.clone(), id.clone()).await;
            }
        } else if let Err(e) = msg {
            match e {
                RecvError::Lagged(n) => {
                    warn!("{} unable to keep up. Skip {} messages", p, n);
                }
                RecvError::Closed => {
                    should_stop = true;
                }
            }
        }
    }
}
