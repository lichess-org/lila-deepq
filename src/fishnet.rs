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
    use mongodb::bson::{oid::ObjectId, Bson, DateTime};
    use serde::{Deserialize, Serialize};

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
        Deep,
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

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Job {
        pub _id: ObjectId,
        pub game_id: GameId,
        pub analysis_type: AnalysisType,
        pub precedence: i32,
        pub owner: Option<String>, // TODO: this should be the key from the database
        pub date_last_updated: DateTime,
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

    pub async fn get_api_user(db: DbConn, key: &m::Key) -> Result<Option<m::ApiUser>> {
        let col = db.database.collection("deepq_apiuser");
        Ok(col
            .find_one(doc! {"key": key.0.clone()}, None)
            .await?
            .map(from_document)
            .transpose()?)
    }

    pub async fn insert_one_job(db: DbConn, job: CreateJob) -> Result<ObjectId> {
        let job_col = db.database.collection("deepq_fishnetjobs");
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
        let job_col = &db.database.collection("deepq_fishnetjobs");
        let api_user = &api_user;
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
}

pub mod filters {
    use serde::{Deserialize, Serialize};
    use serde_with::{serde_as, DisplayFromStr};
    use shakmaty::fen::Fen;
    use std::result::Result as StdResult;
    use warp::{
        filters::BoxedFilter,
        http, reject,
        reply::{self, Json, Reply, WithStatus},
        Filter, Rejection,
    };

    use crate::db::DbConn;
    use crate::deepq::api::find_game;
    use crate::error::Error;
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
    pub struct WorkInfo {
        #[serde(rename = "type")]
        _type: WorkType,
        id: String,
        nodes: Nodes,
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
        skip_positions: Vec<u64>,
    }

    async fn get_user_from_key(
        db: DbConn,
        key: &m::Key,
    ) -> StdResult<Option<m::ApiUser>, Rejection> {
        Ok(api::get_api_user(db, key).await?)
    }

    // NOTE: This is not a lambda because async lambdas
    //      are unstable.
    async fn authorize_api_request_impl(
        db: DbConn,
        request_info: FishnetRequest,
    ) -> StdResult<m::ApiUser, Rejection> {
        get_user_from_key(db, &request_info.fishnet.api_key)
            .await?
            .ok_or(reject::custom(Error::Unauthorized))
    }

    /// extract an ApiUser from the json body request
    fn extract_api_user(
        db: DbConn,
    ) -> impl Filter<Extract = (m::ApiUser,), Error = Rejection> + Clone {
        warp::any()
            .map(move || db.clone())
            .and(warp::body::json())
            .and_then(authorize_api_request_impl)
    }

    async fn acquire_job(db: DbConn, api_user: m::ApiUser) -> StdResult<Option<Job>, Rejection> {
        match api::assign_job(db.clone(), api_user).await? {
            Some(job) => {
                // TODO: This doesn't properly unassign the job in the case of error
                let game = find_game(db, job.game_id.clone()).await?;
                // TODO: eventually this fen should come from the actual game.
                let position: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
                    .parse()
                    .expect("this cannot fail");
                Ok(game.map(|game| {
                    Job {
                        game_id: job.game_id.to_string(),
                        position: position,
                        variant: Variant::Standard,
                        // TODO: Figure out what these should be.
                        skip_positions: Vec::new(),
                        moves: game.pgn,
                        work: WorkInfo {
                            id: job._id.to_string(),
                            _type: WorkType::Analysis,
                            nodes: Nodes {
                                // TODO: Grab these from somewhere
                                nnue: 2_000_000_u64,
                                classical: 4_000_000_u64,
                            },
                        },
                    }
                }))
            }
            None => Ok(None),
        }
    }

    async fn check_key_validity(db: DbConn, key: String) -> StdResult<String, Rejection> {
        get_user_from_key(db, &key.into())
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

    pub fn mount(db: DbConn) -> BoxedFilter<(impl Reply,)> {
        let extract_api_user = extract_api_user(db.clone());
        let db = warp::any().map(move || db.clone());

        let acquire = warp::path("acquire")
            .and(db.clone())
            .and(extract_api_user)
            .and_then(acquire_job)
            .and_then(json_object_or_no_content::<Job>);

        let valid_key = warp::path("key")
            .and(db.clone())
            .and(warp::path::param())
            .and_then(check_key_validity);

        acquire.or(valid_key).boxed()
    }
}
