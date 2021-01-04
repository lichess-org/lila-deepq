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

pub mod db;
pub mod deepq;
pub mod error;
pub mod fishnet;
pub mod http;
pub mod irwin;
pub mod lichess;

extern crate clap;
extern crate dotenv;
extern crate futures;
extern crate pretty_env_logger;
extern crate serde_json;
extern crate serde_with;
extern crate log;

use std::env;
use std::result::Result as StdResult;

use clap::{App, SubCommand};
use dotenv::dotenv;
use futures::stream::StreamExt;
use log::{error, info, warn};
use tokio::time::{delay_for, Duration};
use warp::Filter;

#[tokio::main]
async fn main() -> StdResult<(), Box<dyn std::error::Error>> {
    let matches = App::new(env!("CARGO_PKG_NAME"))
                      .version(env!("CARGO_PKG_VERSION"))
                      .author(env!("CARGO_PKG_AUTHORS"))
                      .about(env!("CARGO_PKG_DESCRIPTION"))
                      .subcommand(SubCommand::with_name("deepq_webserver"))
                      .subcommand(SubCommand::with_name("deepq_irwin_job_listener"))
                      .get_matches();

    if let Some(_matches) = matches.subcommand_matches("deepq_webserver") {
        return deepq_web().await;
    }
    if let Some(_matches) = matches.subcommand_matches("deepq_irwin_job_listener") {
        return deepq_irwin_job_listener().await;
    }
    Ok(())
}

async fn deepq_web() -> StdResult<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();
    info!("Reading config...");
    dotenv().ok();

    info!("Connecting to database...");
    let conn = db::connection().await?;

    info!("Mounting urls...");
    let app = fishnet::handlers::mount(conn.clone());

    info!("Starting server...");
    warp::serve(warp::path("fishnet").and(app))
        .run(([127, 0, 0, 1], 3030))
        .await;

    Ok(())
}

async fn deepq_irwin_job_listener() -> StdResult<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();
    info!("Reading config...");
    dotenv().ok();

    let api_url = env::var("LILA_DEEPQ_IRWIN_STREAM_URL")
        .unwrap_or_else(|_| "https://lichess.org/api/stream/irwin".to_string());
    let api_key = env::var("LILA_DEEPQ_IRWIN_LICHESS_API_KEY")?;

    let conn = db::connection().await?;

    info!("Starting up...");
    loop {
        info!("Connecting...");
        let mut stream = irwin::stream(&api_url, &api_key).await?;

        info!("Reading stream...");
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(irwin::StreamMsg::KeepAlive(_)) => info!("keepAlive received"),
                Ok(irwin::StreamMsg::Request(request)) => {
                    info!(
                        "{:?} report: {} for {} games",
                        request.origin,
                        request.user.id.0,
                        request.games.len()
                    );
                    irwin::add_to_queue(conn.clone(), request).await?;
                }
                Err(e) => error!("Error parsing message from lichess:\n{:?}", e),
            }
        }

        warn!("Disconnected, sleeping for 5s...");
        delay_for(Duration::from_millis(5000)).await;
    }
}
