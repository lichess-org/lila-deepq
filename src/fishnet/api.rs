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
use chrono::prelude::*;
use futures::future::Future;
use std::convert::TryInto;
use std::iter;

use mongodb::bson::{
    doc, from_document, oid::ObjectId, to_document, Bson, DateTime as BsonDateTime,
};
use mongodb::options::{FindOneAndUpdateOptions, UpdateModifications};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::Serialize;

use crate::db::DbConn;
use crate::deepq::model::{GameId, UserId, GameAnalysis};
use crate::deepq::api::{find_analysis_for_job};
use crate::error::{Error, Result};
use crate::fishnet::model as m;

#[derive(Debug, Clone)]
pub struct CreateApiUser {
    pub user: Option<UserId>,
    pub name: String,
    pub perms: Vec<m::AnalysisType>,
}

impl From<CreateApiUser> for m::ApiUser {
    fn from(job: CreateApiUser) -> m::ApiUser {
        let mut rng = thread_rng();
        let key: String = iter::repeat(())
            .map(|()| rng.sample(Alphanumeric))
            .map(char::from)
            .take(7)
            .collect();
        m::ApiUser {
            _id: ObjectId::new(),
            key: key.into(),
            user: job.user,
            name: job.name,
            perms: job.perms,
        }
    }
}

pub async fn create_api_user(db: DbConn, create: CreateApiUser) -> Result<m::ApiUser> {
    let col = m::ApiUser::coll(db);
    let api_user: m::ApiUser = create.into();
    col
        .insert_one(to_document(&api_user)?, None)
        .await?
        .inserted_id
        .as_object_id()
        .ok_or(Error::CreateError)?;
    Ok(api_user)
}

pub async fn get_api_user(db: DbConn, key: m::Key) -> Result<Option<m::ApiUser>> {
    let col = m::ApiUser::coll(db);
    Ok(col
        .find_one(doc! {"key": key.0.clone()}, None)
        .await?
        .map(from_document)
        .transpose()?)
}

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

pub async fn unassign_job(db: DbConn, api_user: m::ApiUser, id: ObjectId) -> Result<()> {
    m::Job::coll(db)
        .update_one(
            doc! { "_id": id, "owner": api_user.key.clone() },
            UpdateModifications::Document(doc! {"owner": Bson::Null}),
            None,
        )
        .await?;
    Ok(())
}

pub async fn game_id_for_job_id(db: DbConn, id: ObjectId) -> Result<Option<GameId>>  {
    Ok(m::Job::coll(db)
        .find_one(doc! {"_id": id}, None)
        .await?
        .map(from_document)
        .transpose()?
        .map(|d: m::Job| d.game_id))
}


pub async fn is_job_completed(db: DbConn, id: ObjectId) -> Result<Option<bool>>  {
    let job: m::Job = m::Job::coll(db.clone())
        .find_one(doc! {"_id": id}, None)
        .await?
        .map(from_document)
        .transpose()?
        .ok_or(Error::NotFoundError)?;

    Ok(
        find_analysis_for_job(db, job._id.clone()).await?
        .map(|a: GameAnalysis| {
            a.analysis.iter().filter(|o| o.is_none()).count() == 0_usize
        })
    )
}

pub async fn delete_job(db: DbConn, id: ObjectId) -> Result<()> {
    m::Job::coll(db)
        .delete_one(doc! { "_id": id }, None)
        .await?;
    Ok(())
}

pub async fn get_user_job(db: DbConn, id: ObjectId, user: m::ApiUser) -> Result<Option<m::Job>> {
    Ok(m::Job::coll(db)
        .find_one(doc! {"_id": id, "owner": user.key}, None)
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
        .await?
        .try_into()?;
    let queued = m::Job::queued_jobs(db.clone(), analysis_type.clone())
        .await?
        .try_into()?;
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
