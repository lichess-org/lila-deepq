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
extern crate log;
extern crate pretty_env_logger;
extern crate serde_json;
extern crate serde_with;

use std::env;
use std::result::Result as StdResult;

use clap::{App, Arg};
use dotenv::dotenv;
use futures::stream::StreamExt;
use log::{error, info, warn, debug};
use tokio::time::{delay_for, Duration};
use warp::Filter;

use crate::error::Error;

#[tokio::main]
async fn main() -> StdResult<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .subcommand(App::new("webserver"))
        .subcommand(App::new("irwin-job-listener"))
        .subcommand(
            App::new("new-fishnet-key")
                .arg(
                    Arg::with_name("name")
                        .short("n")
                        .long("name")
                        .help("Sets the name of the key")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("user")
                        .short("u")
                        .long("user")
                        .help("Sets the username for the key")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("analysis-user")
                        .long("analysis-user")
                        .help("Allows this key to be used for user analysis"),
                )
                .arg(
                    Arg::with_name("analysis-system")
                        .long("analysis-system")
                        .help("Allows this key to be used for system analysis"),
                )
                .arg(
                    Arg::with_name("analysis-deep")
                        .long("analysis-deep")
                        .help("Allows this key to be used for deep analysis"),
                )
                .about("Creates a new fishnet key"),
        )
        .get_matches();

    if let Some(_matches) = matches.subcommand_matches("webserver") {
        return deepq_web().await;
    }
    if let Some(_matches) = matches.subcommand_matches("irwin-job-listener") {
        return deepq_irwin_job_listener().await;
    }
    if let Some(_matches) = matches.subcommand_matches("new-fishnet-key") {
        println!("WUAT");
        let is_deep = _matches.is_present("analysis-deep");
        let is_user = _matches.is_present("analysis-user");
        let is_system = _matches.is_present("analysis-system");
        debug!("deep: {}, user: {}, system: {}", is_deep, is_user, is_system);
        if !is_deep && !is_user && !is_system {
            error!("The key needs to include at least one analysis permissions level, such as --analysis-{{deep|user|system}}");
            return Err(Box::new(Error::InvalidCommandLineArguments))
        }
        println!("Wuat");
    }
    Ok(())
}

async fn deepq_web() -> StdResult<(), Box<dyn std::error::Error>> {
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
