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
use futures::future::Future;
use std::convert::TryInto;

use mongodb::bson::{
    doc, from_document, Bson,
};
use mongodb::options::{FindOneAndUpdateOptions, UpdateModifications};
use serde::Serialize;

use crate::db::{ DbConn, Queryable };
use crate::deepq::model::GameId;
use crate::error::Result;
use crate::fishnet::model as m;

pub async fn create_api_user(db: DbConn, create: m::CreateApiUser) -> Result<m::ApiUser> {
    m::ApiUser::insert(db, create).await
}

pub async fn get_api_user(db: DbConn, key: m::Key) -> Result<Option<m::ApiUser>> {
    let col = m::ApiUser::coll(db);
    Ok(col
        .find_one(doc! {"key": key.0.clone()}, None)
        .await?
        .map(from_document)
        .transpose()?)
}

pub fn insert_many_jobs<'a, T>(
    db: DbConn,
    jobs: &'a T,
) -> impl Iterator<Item = impl Future<Output = Result<m::Job>>> + 'a
where
    T: Iterator<Item = &'a m::CreateJob> + Clone,
{
    jobs.clone()
        .map(move |job| m::Job::insert(db.clone(), job.clone()))
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

pub async fn unassign_job(db: DbConn, api_user: m::ApiUser, id: m::JobId) -> Result<()> {
    m::Job::coll(db)
        .update_one(
            doc! { "_id": id.0, "owner": api_user.key.clone() },
            UpdateModifications::Document(doc! {"owner": Bson::Null}),
            None,
        )
        .await?;
    Ok(())
}

pub async fn game_id_for_job_id(db: DbConn, id: m::JobId) -> Result<Option<GameId>> {
    Ok(m::Job::coll(db)
        .find_one(doc! {"_id": id.0}, None)
        .await?
        .map(from_document)
        .transpose()?
        .map(|d: m::Job| d.game_id))
}

pub async fn set_complete(db: DbConn, id: m::JobId) -> Result<()> {
    m::Job::coll(db)
        .update_one(
            doc! {"_id": {"$eq": id.0}},
            UpdateModifications::Document(doc! {"$set": { "is_complete": true }}),
            None,
        )
        .await?;
    Ok(())
}

pub async fn delete_job(db: DbConn, id: m::JobId) -> Result<()> {
    m::Job::coll(db)
        .delete_one(doc! { "_id": id.0 }, None)
        .await?;
    Ok(())
}

pub async fn get_user_job(db: DbConn, id: m::JobId, user: m::ApiUser) -> Result<Option<m::Job>> {
    Ok(m::Job::coll(db)
        .find_one(doc! {"_id": id.0, "owner": user.key}, None)
        .await?
        .map(from_document)
        .transpose()?)
}

pub async fn get_job(db: DbConn, id: m::JobId) -> Result<Option<m::Job>> {
    Ok(m::Job::coll(db)
        .find_one(doc! {"_id": id.0}, None)
        .await?
        .map(from_document)
        .transpose()?)
}

#[derive(Serialize)]
pub struct QStatus {
    acquired: u64,
    queued: u64,
    oldest: u64,
}

pub async fn q_status(db: DbConn, analysis_type: m::AnalysisType) -> Result<QStatus> {
    let acquired = m::Job::acquired_jobs(db.clone(), analysis_type.clone())
        .await?;
    let queued = m::Job::queued_jobs(db.clone(), analysis_type.clone())
        .await?;
    let oldest = m::Job::oldest_job(db.clone(), analysis_type.clone())
        .await?
        .map(|job| job.seconds_since_created())
        .unwrap_or(0_i64)
        .try_into()?;
    Ok(QStatus {
        acquired,
        queued,
        oldest,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStatus {
    Unknown,
    Active,
    Inactive,
}

pub fn key_status(api_user: Option<m::ApiUser>) -> Option<KeyStatus> {
    // TODO: Add in appropriate tracking for invalidated keys.
    api_user.map(|_| KeyStatus::Active)
}
