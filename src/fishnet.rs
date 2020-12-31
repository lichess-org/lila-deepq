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
    use mongodb::{
        bson::{oid::ObjectId, Bson, DateTime},
        Collection,
    };
    use serde::{Deserialize, Serialize};

    use crate::db::DbConn;
    use crate::deepq::model::{GameId, UserId};

    #[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
    pub struct Key(pub String);

    impl From<Key> for Bson {
        fn from(k: Key) -> Bson {
            Bson::String(k.to_string().to_lowercase())
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, strum_macros::ToString)]
    #[serde(rename_all = "lowercase")]
    pub enum AnalysisType {
        UserAnalysis,
        IrwinDeep,
        CRDeep,
    }

    impl From<AnalysisType> for Bson {
        fn from(at: AnalysisType) -> Bson {
            Bson::String(at.to_string().to_lowercase())
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct ApiUser {
        pub key: Key,
        pub user: Option<UserId>,
        pub name: String,
        pub perms: Vec<AnalysisType>,
    }

    impl ApiUser {
        pub fn coll(db: DbConn) -> Collection {
            db.database.collection("deepq_apiuser")
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Job {
        pub _id: ObjectId,
        pub game_id: GameId,
        pub analysis_type: AnalysisType,
        pub precedence: i32,
        pub owner: Option<String>, // TODO: this should be the key from the database
        pub date_last_updated: DateTime,
    }

    impl Job {
        pub fn coll(db: DbConn) -> Collection {
            db.database.collection("deepq_fishnetjobs")
        }
    }
}

pub mod api {

    use chrono::prelude::*;
    use futures::future::Future;
    use mongodb::bson::{
        doc, from_document, oid::ObjectId, to_document, Bson, DateTime as BsonDateTime,
    };
    use mongodb::options::{FindOneAndUpdateOptions, UpdateModifications};

    use crate::db::DbConn;
    use crate::deepq::model::GameId;
    use crate::error::{Error, Result};
    use crate::fishnet::model as m;

    #[derive(Debug, Clone)]
    pub struct CreateJob {
        pub game_id: GameId,
        pub analysis_type: m::AnalysisType,
        pub precedence: i32,
    }

    impl From<CreateJob> for m::Job {
        fn from(job: CreateJob) -> m::Job {
            m::Job {
                _id: ObjectId::new(),
                game_id: job.game_id,
                analysis_type: job.analysis_type,
                precedence: job.precedence,
                owner: None,
                date_last_updated: BsonDateTime(Utc::now()),
            }
        }
    }

    pub async fn get_api_user(db: DbConn, key: m::Key) -> Result<Option<m::ApiUser>> {
        let col = m::ApiUser::coll(db);
        Ok(col
            .find_one(doc! {"key": key.0.clone()}, None)
            .await?
            .map(from_document)
            .transpose()?)
    }

    pub async fn insert_one_job(db: DbConn, job: CreateJob) -> Result<ObjectId> {
        let job_col = m::Job::coll(db);
        let job: m::Job = job.into();
        Ok(job_col
            .insert_one(to_document(&job)?, None)
            .await?
            .inserted_id
            .as_object_id()
            .ok_or(Error::CreateError)?
            .clone())
    }

    pub fn insert_many_jobs<'a, T>(
        db: DbConn,
        jobs: &'a T,
    ) -> impl Iterator<Item = impl Future<Output = Result<ObjectId>>> + 'a
    where
        T: Iterator<Item = &'a CreateJob> + Clone,
    {
        jobs.clone()
            .map(move |job| insert_one_job(db.clone(), job.clone()))
    }

    pub async fn assign_job(db: DbConn, api_user: m::ApiUser) -> Result<Option<m::Job>> {
        let job_col = m::Job::coll(db);
        Ok(job_col
            .find_one_and_update(
                doc! {
                    "owner": Bson::Null,
                    "analysis_type": doc!{ "$in": Bson::Array(api_user.perms.iter().map(Into::into).collect()) },
                },
                UpdateModifications::Document(doc! {"$set": {"owner": api_user.key.clone()}}),
                FindOneAndUpdateOptions::builder()
                    .sort(doc! {"precedence": -1, "date_last_updated": 1})
                    .build(),
            )
            .await?
            .map(from_document)
            .transpose()?)
    }

    pub async fn unassign_job(db: DbConn, id: ObjectId) -> Result<()> {
        m::Job::coll(db)
            .update_one(
                doc! { "_id": id },
                UpdateModifications::Document(doc! {"owner": Bson::Null}),
                None,
            )
            .await?;
        Ok(())
    }

    pub async fn delete_job(db: DbConn, id: ObjectId) -> Result<()> {
        m::Job::coll(db)
            .delete_one(doc! { "_id": id }, None)
            .await?;
        Ok(())
    }
}

pub mod http {
    use std::convert::Infallible;
    use std::result::Result as StdResult;
    use std::str::FromStr;

    use serde::{Deserialize, Serialize};
    use serde_with::{serde_as, DisplayFromStr};
    use shakmaty::fen::Fen;
    use warp::{
        filters::BoxedFilter,
        http, reject,
        reply::{self, Json, Reply, WithStatus},
        Filter, Rejection,
    };

    use crate::db::DbConn;
    use crate::deepq::api::{find_game, starting_position};
    use crate::error::{Error, HttpError};
    use crate::fishnet::api;
    use crate::fishnet::model as m;

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
    pub struct Analysis {
        depth: u8,
        multipv: bool,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct WorkInfo {
        #[serde(rename = "type")]
        _type: WorkType,
        id: String,
        nodes: Nodes,
        analysis: Option<Analysis>,
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

    #[derive(Debug)]
    pub struct HeaderKey(pub m::Key);

    impl FromStr for HeaderKey {
        type Err = Error;

        fn from_str(s: &str) -> StdResult<Self, Self::Err> {
            Ok(HeaderKey(m::Key(s
                .strip_prefix("Bearer ")
                .ok_or(HttpError::MalformedHeader)?
                .to_string())))
        }
    }

    impl From<HeaderKey> for m::Key {
        fn from(hk: HeaderKey) -> m::Key {
            hk.0
        }
    }

    async fn get_user_from_key(
        db: DbConn,
        key: m::Key,
    ) -> StdResult<Option<m::ApiUser>, Rejection> {
        Ok(api::get_api_user(db, key).await?)
    }

    // NOTE: This is not a lambda because async lambdas
    //      are unstable.
    async fn authorize_api_request_impl<T>(
        db: DbConn,
        key: T,
    ) -> StdResult<m::ApiUser, Rejection>
        where T: Into<m::Key>
    {
        get_user_from_key(db, key.into())
            .await?
            .ok_or(reject::custom(HttpError::Unauthorized))
    }

    /// extract an ApiUser from the json body request
    fn require_valid_key_in_body(
        db: DbConn,
    ) -> impl Filter<Extract = (m::ApiUser,), Error = Rejection> + Clone {
        warp::any()
            .map(move || db.clone())
            .and(warp::body::json::<FishnetRequest>())
            .and_then(authorize_api_request_impl)
    }

    /// extract a m::Key from the Authorization header
    fn extract_key_from_header()
        -> impl Filter<Extract = (HeaderKey,), Error = Rejection> + Clone
    {
        warp::any()
            .and(warp::header::<HeaderKey>("authorization"))
    }

    /// extract an m::ApiUser from the Authorization header
    fn require_valid_key_in_header(
        db: DbConn,
    ) -> impl Filter<Extract = (m::ApiUser,), Error = Rejection> + Clone {
        warp::any()
            .map(move || db.clone())
            .and(extract_key_from_header())
            .and_then(authorize_api_request_impl)
    }

    // TODO: get this from config or env? or lila? (probably lila, tbh)
    fn nodes_for_job(job: &m::Job) -> Nodes {
        match job.analysis_type {
            // TODO: what is the default right now for lila's fishnet queue?
            m::AnalysisType::UserAnalysis => Nodes {
                nnue: 2_250_000_u64,
                classical: 4_050_000_u64,
            },
            m::AnalysisType::CRDeep => Nodes {
                nnue: 2_500_000_u64,
                classical: 4_500_000_u64,
            },
            m::AnalysisType::IrwinDeep => Nodes {
                nnue: 2_500_000_u64,
                classical: 4_500_000_u64,
            },
        }
    }

    // TODO: get this from config or env? or lila? (probably lila, tbh)
    fn analysis_for_job(job: &m::Job) -> Option<Analysis> {
        match job.analysis_type {
            m::AnalysisType::CRDeep => Some(Analysis {
                // TODO: what is the default that we tend to use for CR?
                depth: 40,
                multipv: false,
            }),
            _ => None,
        }
    }

    // TODO: get this from config or env? or lila? (probably lila, tbh)
    fn skip_positions_for_job(job: &m::Job) -> Vec<u8> {
        match job.analysis_type {
            // TODO: what is the default right now for lila's fishnet queue?
            m::AnalysisType::UserAnalysis => vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            m::AnalysisType::CRDeep => vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            m::AnalysisType::IrwinDeep => Vec::new(),
        }
    }

    async fn acquire_job(db: DbConn, api_user: m::ApiUser) -> StdResult<Option<Job>, Rejection> {
        // NOTE: not using .map because of unstable async lambdas
        Ok(match api::assign_job(db.clone(), api_user).await? {
            Some(job) => {
                let game = match find_game(db.clone(), job.game_id.clone()).await {
                    Ok(game) => Ok(game),
                    Err(err) => {
                        api::unassign_job(db.clone(), job._id.clone()).await?;
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
                            analysis: analysis_for_job(&job),
                        },
                    }),
                }
            }
            None => None,
        })
    }

    async fn check_key_validity(db: DbConn, key: String) -> StdResult<String, Rejection> {
        get_user_from_key(db, key.into())
            .await?
            .ok_or(reject::not_found())
            .map(|_| String::new())
    }

    async fn json_object_or_no_content<T: Serialize>(
        value: Option<T>,
    ) -> StdResult<WithStatus<Json>, Rejection> {
        value.map_or(
            Ok(reply::with_status(
                reply::json(&String::new()),
                http::StatusCode::NO_CONTENT,
            )),
            |val| Ok(reply::with_status(reply::json(&val), http::StatusCode::OK)),
        )
    }

    /// An API error serializable to JSON.
    #[derive(Serialize)]
    struct ErrorMessage {
        code: u16,
        message: String,
    }

    // This function receives a `Rejection` and tries to return a custom
    // value, otherwise simply passes the rejection along.
    async fn fishnet_recover(err: Rejection) -> Result<impl Reply, Infallible> {
        let code;
        let message;

        if err.is_not_found() {
            code = http::StatusCode::NOT_FOUND;
            message = "NOT_FOUND";
        } else if let Some(HttpError::Unauthorized) = err.find() {
            code = http::StatusCode::UNAUTHORIZED;
            message = "UNAUTHORIZED";
        } else if let Some(HttpError::Forbidden) = err.find() {
            code = http::StatusCode::FORBIDDEN;
            message = "FORBIDDEN";
        } else {
            // We should have expected this... Just log and say its a 500
            eprintln!("unhandled rejection: {:?}", err);
            code = http::StatusCode::INTERNAL_SERVER_ERROR;
            message = "UNHANDLED_REJECTION";
        }

        let json = warp::reply::json(&ErrorMessage {
            code: code.as_u16(),
            message: message.into(),
        });

        Ok(warp::reply::with_status(json, code))
    }

    pub fn mount(db: DbConn) -> BoxedFilter<(impl Reply,)> {
        let require_valid_key_in_body = require_valid_key_in_body(db.clone());
        let require_valid_key_in_header = require_valid_key_in_header(db.clone());
        let db = warp::any().map(move || db.clone());

        let acquire = warp::path("acquire")
            .and(db.clone())
            .and(require_valid_key_in_body)
            .and_then(acquire_job)
            .and_then(json_object_or_no_content::<Job>);

        let valid_key = warp::path("key")
            .and(db.clone())
            .and(warp::path::param())
            .and_then(check_key_validity);

        let status = warp::path("status")
            .and(db.clone())
            .and(require_valid_key_in_header)
            .map(|_, _| "");

        acquire.or(valid_key).or(status).recover(fishnet_recover).boxed()
    }
}
