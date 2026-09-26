// Copyright (c) 2026 SurrealDB Ltd
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fmt;

/// Errors that can occur during skip list operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The arena is full and cannot allocate space for the new entry.
    ArenaFull,
    /// A record with the exact same key and version already exists.
    RecordExists,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ArenaFull => write!(f, "arena is full"),
            Error::RecordExists => write!(f, "record already exists"),
        }
    }
}

impl std::error::Error for Error {}

/// Specialized `Result` type for skip list operations.
pub type Result<T> = std::result::Result<T, Error>;
