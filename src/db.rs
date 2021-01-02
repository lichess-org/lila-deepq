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

use std::env;

use mongodb::{Client, Database};

use crate::error::Result;

#[derive(Clone)]
pub struct DbConn {
    pub client: Client,
    pub database: Database,
}

pub async fn connection() -> Result<DbConn> {
    let mongo_uri = env::var("LILA_DEEPQ_MONGO_URI")?;
    let client = Client::with_uri_str(&mongo_uri).await?;

    let database_name = env::var("LILA_DEEPQ_MONGO_DATABASE")?;
    let database = client.database(&database_name);

    Ok(DbConn { client, database })
}
