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
use derive_more::{Display, From};
use mongodb::{
    bson::{doc, from_document, oid::ObjectId, Bson, DateTime},
    options::FindOneOptions,
    Collection,
};
use serde::{Deserialize, Serialize};

use crate::db::DbConn;
use crate::deepq::model::{GameId, UserId};
use crate::error::Result;

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

    pub fn seconds_since_created(&self) -> i64 {
        Utc::now().timestamp() - self.date_last_updated.timestamp()
    }

    pub async fn acquired_jobs(db: DbConn, analysis_type: AnalysisType) -> Result<i64> {
        let filter = doc! {
            "owner": { "$ne": Bson::Null },
            "analysis_type": { "$eq": analysis_type },
        };
        Ok(Job::coll(db.clone()).count_documents(filter, None).await?)
    }

    pub async fn queued_jobs(db: DbConn, analysis_type: AnalysisType) -> Result<i64> {
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
