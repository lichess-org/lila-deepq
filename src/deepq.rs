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

pub mod api {
    use chrono::prelude::*;
    use crate::lichess::api::{UserID, GameID};
    use crate::error::Error;

    use serde::{
        Serialize,
        Deserialize,
    };

    #[derive(Serialize, Deserialize, Debug)]
    pub enum ReportOrigin {
        Moderator,
        Random,
        Tournament,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Report {
        id: UserID,
        date_requested: DateTime<Utc>,
        date_completed: Option<DateTime<Utc>>,
        origin: ReportOrigin,
        precedence: u64,
        required_game_ids: Vec<GameID>,
        processed_game_ids: Vec<GameID>,
    }


    #[derive(Serialize, Deserialize, Debug)]
    pub struct ReportGame {
        id: GameID,
        report_id: String,
        precedence: u64,
        owner: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Blurs {
        nb: u64,
        bits: String, // TODO: why string?!
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct MoveAnalysis {
        cp: Option<u64>,
        mate: Option<u64>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Game {
        id: String,
        emts: Vec<u64>, // TODO: maybe a smaller datatype is more appropriate? u32? u16?
        pgn: String,
        black: Option<String>,
        #[serde(rename = "blackBlurs")]
        black_blurs: Blurs,
        white: Option<String>,
        #[serde(rename = "whiteBlurs")]
        white_blurs: Blurs,
        analyzed: bool,
        analysis: Vec<MoveAnalysis>
    }

    pub fn add_report(report: Report) -> Result<String, Error> {
        Ok("TODO".to_string())
    }

}
