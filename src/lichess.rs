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
        pub cp: Option<u64>,
        pub mate: Option<u64>,
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
