use mongodb::{Client, Database};

#[derive(Clone)]
pub struct DbConn {
    pub client: Client,
    pub database: Database,
}

