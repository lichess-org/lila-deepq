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
    use serde::{
        Serialize,
        Deserialize
    };

    // TODO: use newtypes for ids, like UserId(pub String) and GameID(pub String)
    //       just not sure about how serde works with them right now, so not
    //       solving that yet.
    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct UserID(pub String);

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GameID(pub String);

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct User {
        pub id: UserID,
        pub titled: bool,
        pub engine: bool,
        pub games: u64,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Eval {
        pub cp: Option<i64>,
        pub mate: Option<i64>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Game {
        pub id: GameID,
        pub white: UserID,
        pub black: UserID,
        pub emts: Option<Vec<u64>>,
        pub pgn: String, // TODO: this should be more strongly typed.
        pub analysis: Option<Vec<Eval>>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct IrwinRequest {
        pub t: String,
        pub origin: String,
        pub user: User,
        pub games: Vec<Game>
    }

    pub fn add_to_queue(request: IrwinRequest) {
    }

}
