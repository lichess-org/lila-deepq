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

use std::convert::Into;

use async_trait::async_trait;

use serde::de::DeserializeOwned;
use serde::Serialize;

use mongodb::{
    bson::{Bson, doc, from_document, to_document, Document},
    Client, Collection, Database,
};

use crate::error::{Error, Result};

#[derive(Clone)]
pub struct ConnectionOpts {
    pub mongo_uri: String,
    pub mongo_database: String,
}

#[derive(Clone)]
pub struct DbConn {
    pub client: Client,
    pub database: Database,
}

pub async fn connection(opts: &ConnectionOpts) -> Result<DbConn> {
    let client = Client::with_uri_str(&opts.mongo_uri).await?;
    let database = client.database(&opts.mongo_database);
    Ok(DbConn { client, database })
}

#[async_trait]
pub trait Queryable {
    type ID: Into<Bson> + Sync + Send;
    type CreateRecord: Sync + Send;
    type Record : From<Self::CreateRecord> + DeserializeOwned + Serialize + Sync + Send;

    fn coll(db: DbConn) -> Collection<Document>;

    async fn insert(db: DbConn, create_record: Self::CreateRecord) -> Result<Self::Record> {
        let record: Self::Record = create_record.into();
        Self::coll(db)
            .insert_one(to_document(&record)?, None)
            .await?
            .inserted_id
            .as_object_id()
            .ok_or(Error::CreateError)?;
        Ok(record)
    }

    async fn by_id(db: DbConn, id: Self::ID,) -> Result<Option<Self::Record>> {
        let filter = doc! { "_id": { "$eq": id.into() } };
        Ok(Self::coll(db.clone())
            .find_one(filter, None)
            .await?
            .map(from_document)
            .transpose()?)
    }

    // TODO: add more of the usually candidates for apis here:
    // add: findOne -> Document - > Result<Option<Record>>
    // add: find -> Document - > Result<Vec<Record>>
    // add: find -> Filter - > Result<Vec<Record>>
    //              ^ This needs to be defined somehow
    // add: insert -> CreateRecord -> Result<Record>
    // add: upsert -> Document -> CreateRecord -> Result<Record>
}
