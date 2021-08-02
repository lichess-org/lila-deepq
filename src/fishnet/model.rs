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
use chrono::prelude::*;
use derive_more::{Display, From, Into};
use futures::stream::{Stream, StreamExt};
use log::warn;
use mongodb::{
    bson::{doc, from_document, oid::ObjectId, Bson, DateTime, Document},
    options::FindOneOptions,
    Collection,
};
use serde::{Deserialize, Serialize};

use crate::db::{ DbConn, Queryable };
use crate::deepq::model::{GameId, UserId, ReportId};
use crate::error::Result;
use crate::crypto;

#[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
pub struct Key(pub String);

impl From<Key> for Bson {
    fn from(k: Key) -> Bson {
        Bson::String(k.to_string().to_lowercase())
    }
}

// TODO: not sure how I should model this.
//       I'd like it if Irwin and CR were unified, and user/system
//       analysis should also be unified. but it  might be easier
//       to deal with very specific analysis requests.
#[derive(Serialize, Deserialize, Debug, Clone, strum_macros::ToString)]
#[serde(rename_all = "lowercase")]
pub enum AnalysisType {
    UserAnalysis,   // User requested analysis, single-pv
    SystemAnalysis, // System requested analysis, single-pv
    Deep,           // Irwin analysis, multipv, complete game, deeper
}

impl From<AnalysisType> for Bson {
    fn from(at: AnalysisType) -> Bson {
        Bson::String(at.to_string().to_lowercase())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, From, Into, Display)]
pub struct ApiUserId(pub ObjectId);

impl From<ApiUserId> for Bson {
    fn from(i: ApiUserId) -> Bson {
        Bson::ObjectId(i.0)
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiUser {
    pub _id: ApiUserId,
    pub key: Key,
    pub user: Option<UserId>,
    pub name: String,
    pub perms: Vec<AnalysisType>,
}

#[derive(Debug, Clone)]
pub struct CreateApiUser {
    pub user: Option<UserId>,
    pub name: String,
    pub perms: Vec<AnalysisType>,
}

impl From<CreateApiUser> for ApiUser {
    fn from(create_user: CreateApiUser) -> ApiUser {
        ApiUser {
            _id: ApiUserId(ObjectId::new()),
            key: Key(crypto::random_alphanumeric_string(7)),
            user: create_user.user,
            name: create_user.name,
            perms: create_user.perms,
        }
    }
}

impl Queryable for ApiUser {
    type ID = ApiUserId;
    type CreateRecord = CreateApiUser;
    type Record = ApiUser;

    fn coll(db: DbConn) -> Collection<Document> {
        db.database.collection("deepq_apiuser")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, From, Into, Display)]
pub struct JobId(pub ObjectId);

impl From<JobId> for Bson {
    fn from(i: JobId) -> Bson {
        Bson::ObjectId(i.0)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Job {
    pub _id: JobId,
    pub game_id: GameId,
    pub analysis_type: AnalysisType,
    pub precedence: i32,
    pub owner: Option<String>, // TODO: this should be the key from the database
    pub date_last_updated: DateTime,
    pub report_id: Option<ReportId>,
    pub is_complete: bool, // Denormalized cache of completion state.
}

#[derive(Debug, Clone)]
pub struct CreateJob {
    pub game_id: GameId,
    pub report_id: Option<ReportId>,
    pub analysis_type: AnalysisType,
    pub precedence: i32,
}

impl From<CreateJob> for Job {
    fn from(job: CreateJob) -> Job {
        Job {
            _id: JobId(ObjectId::new()),
            game_id: job.game_id,
            report_id: job.report_id,
            analysis_type: job.analysis_type,
            precedence: job.precedence,
            owner: None,
            date_last_updated: Utc::now().into(),
            is_complete: false
        }
    }
}

impl Queryable for Job {
    type ID = JobId;
    type CreateRecord = CreateJob;
    type Record = Job;

    fn coll(db: DbConn) -> Collection<Document> {
        db.database.collection("deepq_fishnetjobs")
    }
}

impl Job {
    pub fn seconds_since_created(&self) -> i64 {
        (Utc::now().timestamp_millis() - self.date_last_updated.timestamp_millis())/1000_i64
    }

    pub async fn acquired_jobs(db: DbConn, analysis_type: AnalysisType) -> Result<u64> {
        let filter = doc! {
            "owner": { "$ne": Bson::Null },
            "analysis_type": { "$eq": analysis_type },
        };
        Ok(Job::coll(db.clone()).count_documents(filter, None).await?)
    }

    pub async fn find_by_report(
        db: DbConn,
        report_id: ReportId,
    ) -> Result<impl Stream<Item = Result<Job>>> {
        let p = "Job::find_by_report >";
        let filter = doc! {
            "report_id": { "$eq": report_id.0.clone() }
        };
        Ok(Job::coll(db.clone())
            .find(filter, None)
            .await?
            .filter_map(move |doc_result| async move {
                match doc_result.is_ok() {
                    false => {
                        warn!(
                            "{} error processing cursor of jobs: {:?}.",
                            p,
                            doc_result.expect_err("silly rabbit")
                        );
                        None
                    },
                    true => Some(doc_result.expect("silly rabbit"))
                }
            })
            .map(from_document::<Job>)
            .map(|i| i.map_err(|e| e.into()))
            .boxed()
        )
    }

    pub async fn queued_jobs(db: DbConn, analysis_type: AnalysisType) -> Result<u64> {
        let filter = doc! {
            "owner": { "$eq": Bson::Null },
            "analysis_type": { "$eq": analysis_type },
        };
        Ok(Job::coll(db.clone()).count_documents(filter, None).await?)
    }

    pub async fn oldest_job(db: DbConn, analysis_type: AnalysisType) -> Result<Option<Job>> {
        let filter = doc! {
            "owner": { "$eq": Bson::Null },
            "analysis_type": { "$eq": analysis_type },
        };
        let options = FindOneOptions::builder()
            .sort(doc! { "date_last_updated": -1 })
            .build();
        Ok(Job::coll(db.clone())
            .find_one(filter, options)
            .await?
            .map(from_document::<Job>)
            .transpose()?)
    }
}
