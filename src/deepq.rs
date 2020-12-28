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

pub mod model {
    use std::convert::TryFrom;

    use chrono::prelude::*;
    use derive_more::{From, Display};
    use serde::{Serialize, Deserialize};
    use mongodb::bson::{
        doc,
        Bson,
        Document,
        oid::ObjectId
    };

    use crate::error::Error;

    #[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
    pub struct UserId(pub String);

    impl From<UserId> for Bson {
        fn from(ui: UserId) -> Bson {
            Bson::String(ui.to_string())
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
    pub struct GameId(pub String);

    impl From<GameId> for Bson {
        fn from(gi: GameId) -> Bson {
            Bson::String(gi.to_string())
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, From, Display)]
    #[serde(rename_all = "lowercase")]
    pub enum ReportOrigin {
        Moderator,
        Random,
        Tournament,
    }

    impl From<ReportOrigin> for Bson {
        fn from(ro: ReportOrigin) -> Bson {
            Bson::String(ro.to_string())
        }
    }

    pub fn precedence_for_origin(origin: ReportOrigin) -> u64 {
        match origin {
            Moderator => 1_000_000u64,
            Tournament => 100u64,
            Random => 10u64,
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, strum_macros::ToString)]
    pub enum ReportType {
        Irwin,
        CR,
        PGNSPY,
    }

    impl From<ReportType> for Bson {
        fn from(rt: ReportType) -> Bson {
            Bson::String(rt.to_string())
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct CreateReport {
        pub user_id: UserId,
        pub origin: ReportOrigin,
        pub report_type: ReportType,
        pub games: Vec<GameId>,
    }

    impl From<CreateReport> for Document {
        fn from(report: CreateReport) -> Document {
            doc! {
                "user_id": report.user_id.0,
                "origin": report.origin,
                "report_type": report.report_type,
                "games": report.games,
                "date_requested": Utc::now(),
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Report {
        pub _id: ObjectId,
        pub user_id: UserId,
        pub date_requested: DateTime<Utc>,
        pub date_completed: Option<DateTime<Utc>>,
        pub origin: ReportOrigin,
        pub report_type: ReportType,
        pub games: Vec<GameId>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, strum_macros::ToString)]
    pub enum AnalysisType {
        Fishnet,
        Deep,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct FishnetJobQ {
        pub game_id: GameId,
        pub analysis_type: AnalysisType,
        pub precedence: u64,
        pub owner: Option<String>, // TODO: this should be the key from the database
        pub date_last_updated: DateTime<Utc>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Blurs {
        pub nb: u64,
        pub bits: String, // TODO: why string?!
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Eval {
        pub cp: Option<i64>,
        pub mate: Option<i64>,
    }

    // TODO: this should come directly from the lila db, why store this more than once?
    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GameCache {
        pub game_id: GameId,
        pub emts: Vec<u64>, // TODO: maybe a smaller datatype is more appropriate? u32? u16?
        pub pgn: String,
        pub black: Option<String>,
        #[serde(rename = "blackBlurs")]
        pub black_blurs: Blurs,
        pub white: Option<String>,
        #[serde(rename = "whiteBlurs")]
        pub white_blurs: Blurs,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GameAnalysis {
        pub game_id: GameId,
        pub analysis: Vec<Eval>, // TODO: we should be able to compress this.
        pub pvs: u8,
        pub depth: Option<u64>,
        pub nodes: Option<u64>,
    }
}

pub mod api {
    use mongodb::bson::{
        doc,
        de::from_document

    };

    use crate::db::DbConn;
    use crate::error::Error;
    use crate::deepq::model;

    pub async fn create_report(db :DbConn, report: model::CreateReport) -> Result<model::Report, Error> {
        let reports = db.database.collection("deepq_reports");
        let result = reports.insert_one(report.into(), None).await?;
        reports.find_one(doc!{ "_id": result.inserted_id }, None).await?
            .map(from_document)
            .transpose()?
            .ok_or(Error::CreateError)
    }

}
