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

pub mod api {
    use serde::{
        Serialize,
        Deserialize,
    };

    use mongodb::bson::{
        doc,
        from_document,
    };
    use crate::error::Result;
    use crate::deepq::model::UserId;
    use crate::db::DbConn;

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Key(pub String);

    impl From<String> for Key {
        fn from(key: String) -> Self {
            Key(key)
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct APIUser {
        pub key: Key,
        pub user: Option<UserId>,
        pub name: String,
    }

    pub async fn get_api_user(db: DbConn, key: &Key) -> Result<Option<APIUser>> {
        let col = db.database.collection("token");
        Ok(
            col.find_one(doc!{"key": key.0.clone()}, None).await?
                .map(from_document)
                .transpose()?
        )
    }
}

pub mod filters {
    use std::result::{Result as StdResult};
    use serde::{Serialize, Deserialize};
    use warp::{
        Filter,
        filters::BoxedFilter,
        http,
        reject,
        Rejection,
        reply::{self, Json, Reply, WithStatus},
    };
    use serde_with::{serde_as, DisplayFromStr};
    use shakmaty::fen::Fen;

    use crate::error::Error;
    use crate::db::DbConn;
    use crate::fishnet::api;

    // TODO: make this complete for all of the variant types we should support.
    #[derive(Serialize, Deserialize, Debug)]
    pub enum Variant {
        #[serde(rename = "standard")]
        Standard
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub enum WorkType {
        #[serde(rename = "analysis")]
        Analysis,
        #[serde(rename = "move")]
        Move
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct RequestInfo {
        version: String,
        #[serde(rename = "apikey")]
        api_key: api::Key
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct FishnetRequest {
        fishnet: RequestInfo
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct AcquireRequest {
        fishnet: RequestInfo
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
        nodes: Nodes
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
        key: &api::Key,
    ) -> StdResult<Option<api::APIUser>, Rejection> {
        Ok(api::get_api_user(db, key).await?)
    }

    // NOTE: This is not a lambda because async lambdas
    //      are unstable.
    async fn authorize_api_request_impl(
        db: DbConn,
        request_info: FishnetRequest
    ) -> StdResult<api::APIUser, Rejection> {
        get_user_from_key(db, &request_info.fishnet.api_key).await?
            .ok_or(reject::custom(Error::Unauthorized))
    }

    /// extract an APIUser from the json body request
    fn extract_api_user(db: DbConn) -> impl Filter<Extract = (api::APIUser,), Error = Rejection> + Clone {
        warp::any()
            .map(move || db.clone())
            .and(warp::body::json())
            .and_then(authorize_api_request_impl)
    }

    async fn acquire_job(db: DbConn, api_user: api::APIUser) -> StdResult<Option<Job>, Rejection>  {
        return Ok(None);
    }

    async fn check_key_validity(db: DbConn, key: String) -> StdResult<String, Rejection>  {
        get_user_from_key(db, &key.into()).await?
            .ok_or(reject::not_found())
            .map(|_| String::new())
    }

    async fn json_object_or_no_content<T: Serialize>(value: Option<T>) -> StdResult<WithStatus<Json>, Rejection> {
        value.map_or(
            Ok(reply::with_status(reply::json(&String::new()), http::StatusCode::NO_CONTENT)),
            |val| Ok(reply::with_status(reply::json(&val), http::StatusCode::OK))
        )
    }

    pub fn mount(db: DbConn) -> BoxedFilter<(impl Reply,)> {
        let extract_api_user =
            extract_api_user(db.clone());
        let db = warp::any().map(move || db.clone());

        let acquire =
            warp::path("acquire")
                .and(db.clone())
                .and(extract_api_user)
                .and_then(acquire_job)
                .and_then(json_object_or_no_content::<Job>);

        let valid_key = 
            warp::path("key")
            .and(db.clone())
            .and(warp::path::param())
            .and_then(check_key_validity);

        acquire
            .or(valid_key)
            .boxed()
    }
}
