use tokio::time::{delay_for, Duration};
use log::{info, error};
use futures::StreamExt;

use dotenv::dotenv;
use std::env;

use lila_deepq::lichess::api::{IrwinRequest, add_to_queue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let client = reqwest::Client::new();
    let api_url = env::var("LILA_DEEPQ_IRWIN_STREAM_URL")
        .unwrap_or("https://lichess.org/api/stream/irwin".to_string());
    let api_key = env::var("LILA_DEEPQ_IRWIN_LICHESS_API_KEY")?;

    loop {
        let mut stream = client.post(api_url.as_str())
            .header("User-Agent", "Irwin")
            .header("Authorization", format!("Bearer {}", api_key))
            .body("universe64,samfrommy")
            .send()
            .await?
            .bytes_stream();

        while let Some(Ok(bytes)) = stream.next().await { match &bytes[..] {
                b"\n" => info!("Ping received"),
                v => match serde_json::from_slice::<IrwinRequest>(v) {
                    Ok(request) => {
                        info!("IrwinRequest -> Username:{} with {} games", request.user.id.0, request.games.len());
                        add_to_queue(request)
                    },
                    Err(e) => error!("Unexpected message from lichess: {}", e)
                }
            }
        }
        delay_for(Duration::from_millis(5000)).await;
    }
}
