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

use dotenv::dotenv;
use futures::stream::StreamExt;
use log::{error, info, warn};
use tokio::time::{delay_for, Duration};

use lila_deepq::{db, irwin};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
