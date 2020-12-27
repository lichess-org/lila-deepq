use mongodb::error::{Error as _MongoDBError};
use mongodb::bson::de::{Error as _BsonError};
//use serde::de::{Error as _SerdeDeError};

use warp::reject;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // #[error("Serde Deserialization Error")]
    // SerdeDeserializationError(#[from] _SerdeDeError),

    #[error("BSON Error")]
    BsonError(#[from] _BsonError),

    #[error("Mongo Database Error")]
    MongoDBError(#[from] _MongoDBError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("unknown data store error")]
    Unknown,
}

impl reject::Reject for Error {}


pub fn into_rejection<T>(result: Result<T, Error>) -> Result<T, reject::Rejection> {
    result.map_err(|e| reject::custom(e))
}
