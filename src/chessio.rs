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

//--------------------------------------------------------------------------
// Implement the serde serialization/deserialization traits for Fen.
// We can't implement the trait for a struct from a different crate and for
// a Trait for a different crate. 
//
// So to get around this we'll use our own Fen class that will wrap
// the shakmaty one and we implement the traits for it.
//
// Oof, this was more code than expected for this stuff. /shrug
//--------------------------------------------------------------------------
use std::fmt;
use serde::{
    Serialize,
    Deserialize,
    Serializer,
    de::{self, Deserializer, Visitor}
};
use shakmaty::fen::{Fen as ShakmatyFen};

#[derive(Debug)]
pub struct Fen(pub ShakmatyFen);

impl Serialize for Fen {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

struct PositionVisitor;
impl<'de> Visitor<'de> for PositionVisitor {
    type Value = Fen;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("A valid FEN or X-FEN")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        // TODO: ensure this supports X-Fen
        match value.parse::<ShakmatyFen>() {
            Ok(fen) => Ok(Fen(fen)),
            Err(_) =>  Err(E::custom(format!("unable to parse fen from: {}", value)))
        }
    }
}

impl<'de> Deserialize<'de> for Fen {
    fn deserialize<D>(deserializer: D) -> Result<Fen, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(PositionVisitor)
    }
}
