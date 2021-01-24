// Copyright 2020 Lakin Wecker
//
// This file is part of lila-deepq.
//
// lila-deepq is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by
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

use std::net::SocketAddr;
use std::result::Result as StdResult;

use dotenv::dotenv;
use futures::stream::StreamExt;
use log::{debug, error, info, warn};
use structopt::StructOpt;
use tokio::time::{sleep, Duration};
use warp::Filter;

#[derive(Debug, StructOpt)]
#[structopt(name = "lila-deepq", about = "Analysis Queues for lila.")]
enum Command {
    DeepQWebserver(DeepQWebserver),
    IrwinJobListener(IrwinJobListener),
    FishnetNewKey(FishnetNewKey),
}

#[derive(Debug, StructOpt)]
#[structopt(about = "Runs the main lila-deepq webserver.")]
struct DeepQWebserver {
    #[structopt(short, long, env = "LILA_DEEPQ_WEBSERVER_HOST")]
    host: String,

    #[structopt(short, long, env = "LILA_DEEPQ_WEBSERVER_PORT")]
    port: u16,
}


#[derive(Debug, StructOpt)]
#[structopt(about = "Listens for irwin jobs from lila")]
struct IrwinJobListener {
    #[structopt(
        short,
        long,
        env = "LILA_DEEPQ_IRWIN_STREAM_URL",
        default_value = "https://lichess.org/api/stream/irwin"
    )]
    api_url: String,

    #[structopt(short, long, env = "LILA_DEEPQ_IRWIN_LICHESS_API_KEY")]
    lichess_api_key: String,
}

#[derive(Debug, StructOpt)]
#[structopt(about = "Create a new fishnet key.")]
struct FishnetNewKey {
    #[structopt(long)]
    name: String,

    #[structopt(long)]
    user: String,

    #[structopt(short, long)]
    deep_analysis: bool,

    #[structopt(short, long)]
    user_analysis: bool,

    #[structopt(short, long)]
    system_analysis: bool,
}

#[tokio::main]
async fn main() -> StdResult<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    debug!("Reading ...");
    dotenv().ok();

    let command = Command::from_args();
    match command {
        Command::DeepQWebserver(args) => deepq_web(&args).await?,
        Command::IrwinJobListener(args) => deepq_irwin_job_listener(&args).await?,
        Command::FishnetNewKey(args) => fishnet_new_key(&args).await?
    }

    Ok(())
}

async fn deepq_web(args: &DeepQWebserver) -> StdResult<(), Box<dyn std::error::Error>> {
    info!("Connecting to database...");
    let conn = db::connection().await?;

    info!("Mounting urls...");
    let app = fishnet::handlers::mount(conn.clone());

    info!("Starting server...");
    let address: SocketAddr =
        format!("{host}:{port}", host = args.host, port = args.port).parse()?;
    warp::serve(warp::path("fishnet").and(app))
        .run(address)
        .await;

    Ok(())
}

async fn deepq_irwin_job_listener(args: &IrwinJobListener) -> StdResult<(), Box<dyn std::error::Error>> {
    info!("Reading config...");
    dotenv().ok();

    let conn = db::connection().await?;

    info!("Starting up...");
    loop {
        info!("Connecting...");
        let mut stream = irwin::stream(&args.api_url, &args.lichess_api_key).await?;

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
        sleep(Duration::from_millis(5000)).await;
    }

}
async fn fishnet_new_key(_args: &FishnetNewKey) -> StdResult<(), Box<dyn std::error::Error>> {
    debug!("New fishnet key!");
    Ok(())
}

