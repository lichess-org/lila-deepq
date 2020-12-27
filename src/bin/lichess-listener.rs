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
use std::io::{Error, ErrorKind};

use dotenv::dotenv;
use futures::StreamExt;
use log::{info, error};
use serde::{Serialize, Deserialize};
use tokio::io::AsyncBufReadExt;
use tokio::io::stream_reader;
use tokio::time::{delay_for, Duration};

use lila_deepq::lichess::api::{IrwinRequest, add_to_queue};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct KeepAlive {
    keepAlive: bool
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();
    info!("Reading config...");
    dotenv().ok();

    let client = reqwest::Client::builder()
        .tcp_keepalive(Duration::from_millis(1000))
        .build()?;
    let api_url = env::var("LILA_DEEPQ_IRWIN_STREAM_URL")
        .unwrap_or("https://lichess.org/api/stream/irwin".to_string());
    let api_key = env::var("LILA_DEEPQ_IRWIN_LICHESS_API_KEY")?;
    info!("Starting up...");

    loop {
        info!("Connecting...");
        let response = client.get(api_url.as_str())
            .header("User-Agent", "Irwin")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await?;
        let stream = stream_reader(
            response.bytes_stream().map(|i| i.map_err(|e| Error::new(ErrorKind::Other, e)))
        );
        let mut lines = stream.lines();

        info!("Reading stream...");
        while let Some(Ok(line)) = lines.next().await {
            match (
                serde_json::from_str::<KeepAlive>(line.trim().into()),
                serde_json::from_str::<IrwinRequest>(line.trim().into())
            ) {
                (Ok(KeepAlive{keepAlive: _}), _) => info!("keepAlive received"),
                (_, Ok(request)) => {
                    info!(
                        "{} report: {} for {} games",
                        request.origin,
                        request.user.id.0,
                        request.games.len()
                    );
                    add_to_queue(request)
                },
                (_, Err(e)) => error!("Unexpected message: {:?} from lichess:\n{}", line.trim(), e)
            }
        }


        info!("Disconnected, sleeping for 5s...");
        delay_for(Duration::from_millis(5000)).await;
    }
}
