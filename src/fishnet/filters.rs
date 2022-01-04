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

use serde::de::DeserializeOwned;
use warp::{Filter, Rejection};

use super::{api, model as m};
use crate::db::DbConn;
use crate::error::{Error, HttpError};
use crate::http::{forbidden, with};

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

impl From<m::ApiUser> for m::Key {
    fn from(api_user: m::ApiUser) -> m::Key {
        api_user.key
    }
}

#[derive(Clone)]
pub struct Authorized<T>
where
    T: Into<m::Key> + Clone,
{
    val: T,
    api_user: m::ApiUser,
}

impl<T> Authorized<T>
where
    T: Into<m::Key> + Clone,
{
    pub async fn new(db: DbConn, val: T) -> StdResult<Authorized<T>, Rejection> {
        let api_user = api::get_api_user(db, val.clone().into())
            .await?
            .ok_or_else(forbidden)?;
        Ok(Authorized::<T> { val, api_user })
    }

    pub fn val(&self) -> T {
        self.val.clone()
    }

    pub fn api_user(&self) -> m::ApiUser {
        self.api_user.clone()
    }

    pub fn map<T2, F>(&self, f: F) -> Authorized<T2>
    where
        F: Fn(T) -> T2,
        T2: Into<m::Key> + Clone,
    {
        Authorized::<T2> {
            val: f(self.val()),
            api_user: self.api_user(),
        }
    }
}

pub async fn authorize<T>(db: DbConn, t: T) -> StdResult<Authorized<T>, Rejection>
where
    T: Into<m::Key> + Clone,
{
    Ok(Authorized::<T>::new(db.clone(), t).await?)
}

pub async fn api_user_from_key<T>(
    db: DbConn,
    payload_with_key: T,
) -> StdResult<Option<m::ApiUser>, Rejection>
where
    T: Into<m::Key>,
{
    Ok(api::get_api_user(db, payload_with_key.into()).await?)
}

pub fn extract_key_from_header() -> impl Filter<Extract = (HeaderKey,), Error = Rejection> + Clone {
    warp::any().and(warp::header::<HeaderKey>("authorization"))
}

pub fn api_user_from_header(
    db: DbConn,
) -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Rejection> + Clone {
    warp::any()
        .map(move || db.clone())
        .and(extract_key_from_header())
        .and_then(api_user_from_key)
}

pub fn no_api_user() -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Infallible> + Clone {
    warp::any().map(move || None)
}

pub fn authentication_from_header(
    db: DbConn,
) -> impl Filter<Extract = (Option<m::ApiUser>,), Error = Infallible> + Clone {
    warp::any()
        .and(api_user_from_header(db))
        .or(no_api_user())
        .unify()
}

pub fn authorized_json_body<T>(
    db: DbConn,
) -> impl Filter<Extract = (Authorized<T>,), Error = Rejection> + Clone
where
    T: Into<m::Key> + Clone + Send + Sync + DeserializeOwned,
{
    warp::any()
        .and(with(db))
        .and(warp::body::json::<T>())
        .and_then(authorize::<T>)
}
