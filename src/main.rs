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

mod db;
mod deepq;
mod error;
mod fishnet;
mod irwin;
mod lichess;

extern crate dotenv;
extern crate futures;
extern crate serde_json;
extern crate pretty_env_logger;
extern crate serde_with;
#[macro_use] extern crate log;

use dotenv::dotenv;
use std::env;
use mongodb::Client;
use crate::db::DbConn;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    pretty_env_logger::init();

    let mongo_uri = env::var("LILA_DEEPQ_MONGO_URI")?;
    let client = Client::with_uri_str(&mongo_uri).await?;

    let database_name = env::var("LILA_DEEPQ_MONGO_DATABASE")?;
    let database = client.database(&database_name);
    let db = DbConn{client: client, database: database};

    info!("Starting server");

    let app = fishnet::filters::mount(db.clone());

    warp::serve(app)
        .run(([127, 0, 0, 1], 3030))
        .await;

    Ok(())
}

