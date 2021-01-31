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
use std::marker::Send;
use std::result::Result as StdResult;
use std::str::FromStr;

use futures::future::{self, Future};
use mongodb::bson::oid::ObjectId;
use serde::Serialize;
use warp::{
    http, reject,
    reply::{self, Json, Reply, WithStatus},
    Filter, Rejection,
};

use crate::error::{Error, HttpError};

/// Unauthorized rejection
pub fn forbidden() -> Rejection {
    reject::custom(HttpError::Forbidden)
}

pub fn unauthenticated() -> Rejection {
    reject::custom(HttpError::Unauthenticated)
}

/// extract an ApiUser from the json body request
pub fn required_parameter<'a, F, E, V>(
    filter: F,
    err: &'a E,
) -> impl Filter<Extract = (V,), Error = Rejection> + Clone + 'a
where
    F: Filter<Extract = (Option<V>,), Error = Infallible> + Clone + 'a,
    V: Send + Sync,
    E: Fn() -> Rejection + Clone + Send + Sync + 'a,
{
    filter.and_then(move |v: Option<V>| async move { v.ok_or_else(err) })
}

pub fn required_or_unauthenticated<'a, T>(
    o: Option<T>,
) -> impl Future<Output = StdResult<T, Rejection>> {
    if let Some(t) = o {
        return future::ok(t);
    }
    future::err(unauthenticated())
}

pub fn required_or_forbidden<'a, T>(o: Option<T>) -> impl Future<Output = StdResult<T, Rejection>> {
    if let Some(t) = o {
        return future::ok(t);
    }
    future::err(forbidden())
}

#[derive(Clone, Debug)]
pub struct Id(ObjectId);

impl FromStr for Id {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Id(ObjectId::with_string(s)?))
    }
}

impl From<Id> for ObjectId {
    fn from(id: Id) -> ObjectId {
        id.0
    }
}

pub fn with<T>(t: T) -> impl Filter<Extract = (T,), Error = std::convert::Infallible> + Clone
where
    T: Clone + Sync + Send,
{
    warp::any().map(move || t.clone())
}

pub async fn json_object_or_no_content<T: Serialize>(
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
pub struct ErrorMessage {
    code: u16,
    message: String,
}

// This function receives a `Rejection` and tries to return a custom
// value, otherwise simply passes the rejection along.
pub async fn recover(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let message;

    if err.is_not_found() {
        code = http::StatusCode::NOT_FOUND;
        message = "NOT_FOUND";
    } else if let Some(HttpError::Unauthenticated) = err.find() {
        code = http::StatusCode::UNAUTHORIZED;
        message = "UNAUTHORIZED";
    } else if let Some(HttpError::Forbidden) = err.find() {
        code = http::StatusCode::FORBIDDEN;
        message = "FORBIDDEN";
    } else if err.find::<reject::MethodNotAllowed>().is_some() {
        code = http::StatusCode::METHOD_NOT_ALLOWED;
        message = "METHOD_NOT_ALLOWED";
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
