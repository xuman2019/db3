//
//
// mod.rs
// Copyright (C) 2022 rtstore.io Author imotai <codego.me@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

use std::io::{Error as IoError, ErrorKind};
use thiserror::Error;

/// The error system for rtstore
#[derive(Debug, Error)]
pub enum RTStoreError {
    #[error("table with name {tname} was not found")]
    TableNotFoundError { tname: String },
    #[error("file with {path} is invalid")]
    FSInvalidFileError { path: String },
    #[error("filesystem io error:{0}")]
    FSIoError(IoError),
}

/// convert io error to rtstore error
impl From<IoError> for RTStoreError {
    fn from(error: IoError) -> Self {
        RTStoreError::FSIoError(error)
    }
}

impl From<RTStoreError> for IoError {
    fn from(error: RTStoreError) -> Self {
        match error {
            RTStoreError::FSIoError(e) => e,
            _ => IoError::from(ErrorKind::Other),
        }
    }
}

/// The Result for rtstore
pub type Result<T> = std::result::Result<T, RTStoreError>;