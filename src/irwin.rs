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
//
pub mod model {
    use serde::{Serialize, Deserialize};

    use crate::deepq::model::{
        UserId,
        GameId,
        Eval,
        ReportOrigin,
        ReportType,
        CreateReport,
        CreateGame,
    };

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct User {
        pub id: UserId,
        pub titled: bool,
        pub engine: bool,
        pub games: u64,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Game {
        pub id: GameId,
        pub white: UserId,
        pub black: UserId,
        pub emts: Option<Vec<u64>>,
        pub pgn: String, // TODO: this should be more strongly typed.
        pub analysis: Option<Vec<Eval>>,
    }

    impl From<&Game> for CreateGame {
        fn from(g: &Game) -> CreateGame {
            let g = g.clone();
            CreateGame {
                game_id: g.id,
                emts: g.emts.unwrap_or(Vec::new()),
                pgn: g.pgn,
                black: Some(g.black),
                white: Some(g.white),
            }
        }
    }


    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Request {
        pub t: String,
        pub origin: ReportOrigin,
        pub user: User,
        pub games: Vec<Game>
    }

    impl From<Request> for CreateReport {
        fn from(request: Request) -> CreateReport {
            CreateReport {
                user_id: request.user.id,
                origin: request.origin,
                report_type: ReportType::Irwin,
                games: request.games.iter().map(|g| g.id.clone()).collect()
            }
        }
    }


    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct KeepAlive {
        #[serde(rename = "keepAlive")]
        pub keep_alive: bool
    }

}

pub mod api {
    use crate::db::DbConn;
    use crate::error::Error;
    use crate::irwin::model;

    use crate::deepq;

    pub async fn add_to_queue(db: DbConn, request: model::Request) -> Result<(), Error> {
        //request.games.iter()
            //.map(Into::into)
            //.try_for_each(async |g| deepq::api::create_game(db, g).await)?;
        let _report = deepq::api::insert_one_report(db, request.into()).await?;
        Ok(())
    }
}
