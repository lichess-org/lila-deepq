// Copyright 2021 Lakin Wecker
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
//
//

use std::io::{Error as IoError, ErrorKind};
use std::result::Result as StdResult;
use std::str::FromStr;

// use log::debug;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncBufReadExt, time::Duration};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tokio_util::io::StreamReader;

use crate::error::{Error, Result};
use crate::irwin::api::Request;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeepAlive {
    #[serde(rename = "keepAlive")]
    pub keep_alive: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Msg {
    KeepAlive(KeepAlive),
    Request(Request),
}

impl FromStr for Msg {
    type Err = Error;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

#[allow(clippy::needless_question_mark)]
pub async fn listener(url: &str, api_key: &str) -> Result<impl Stream<Item = Result<Msg>>> {
    let client = reqwest::Client::builder()
        .tcp_keepalive(Duration::from_millis(1000))
        .build()?;
    let response = client
        .get(url)
        .header("User-Agent", "lila-deepq")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    let stream = response
        .bytes_stream()
        .map(|i| i.map_err(|e| IoError::new(ErrorKind::Other, e)));
    let stream = LinesStream::new(StreamReader::new(stream).lines());
    let stream = Box::new(stream.map(|line| {
        let line = line?;
        Ok(FromStr::from_str(&line)?)
    }));
    Ok(stream)
}

