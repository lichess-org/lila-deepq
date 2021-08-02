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
// Provide some simple cryptography related utilities. 
//
// These should always be wrappers around third party providers, we'll
// never do the crypto ourselves, but this wraps up proper crypto in an interface
// that we'll use often.

use std::iter;

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

pub fn random_alphanumeric_string(size: usize) -> String {
    iter::repeat(())
        .map(|()| thread_rng().sample(Alphanumeric))
        .map(char::from)
        .take(size)
        .collect()
}
