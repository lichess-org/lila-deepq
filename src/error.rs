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

use mongodb::error::{Error as _MongoDBError};
use mongodb::bson::de::{Error as _BsonDeError};
use mongodb::bson::ser::{Error as _BsonSeError};
//use serde::de::{Error as _SerdeDeError};

use warp::reject;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // #[error("Serde Deserialization Error")]
    // SerdeDeserializationError(#[from] _SerdeDeError),
    #[error("I am somehow unable to create a record in the database.")]
    CreateError,

    #[error("BSON Error")]
    BsonSerializationError(#[from] _BsonSeError),

    #[error("BSON Error")]
    BsonDeserializationError(#[from] _BsonDeError),

    #[error("Mongo Database Error")]
    MongoDBError(#[from] _MongoDBError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("unknown data store error")]
    Unknown,

    #[error("I haven't implemented this yet")]
    Unimplemented,
}

impl reject::Reject for Error {}

impl From<Error> for reject::Rejection {
    fn from(e: Error) -> Self {
        reject::custom(e)
    }
}
