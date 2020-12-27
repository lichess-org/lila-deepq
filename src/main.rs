mod fishnet;
mod lichess;
mod deepq;
mod db;
mod error;
mod chessio;

extern crate dotenv;
extern crate futures;
extern crate serde_json;
extern crate pretty_env_logger;
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

