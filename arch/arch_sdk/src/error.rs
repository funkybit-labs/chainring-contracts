use std::{array::TryFromSliceError, string::FromUtf8Error};

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum SDKError {
    #[error("signing and sending transaction failed")]
    SignAndSendFailed,

    #[error("get processed transaction failed")]
    GetProcessedTransactionFailed,

    #[error("elf path cannot be found")]
    ElfPathNotFound,

    #[error("send transaction failed")]
    SendTransactionFailed,

    #[error("returned invalid response type")]
    InvalidResponseType,

    #[error("deserialization error")]
    DeserializationError,

    #[error("from hex error")]
    FromHexError,

    #[error("from slice error")]
    FromSliceError,

    #[error("from utf8 error")]
    FromUtf8Error,

    #[error("from str error: {0}")]
    FromStrError(String),
}

impl From<hex::FromHexError> for SDKError {
    fn from(_e: hex::FromHexError) -> Self {
        SDKError::FromHexError
    }
}

impl From<TryFromSliceError> for SDKError {
    fn from(_e: TryFromSliceError) -> Self {
        SDKError::FromSliceError
    }
}

impl From<FromUtf8Error> for SDKError {
    fn from(_e: FromUtf8Error) -> Self {
        SDKError::FromUtf8Error
    }
}

impl From<anyhow::Error> for SDKError {
    fn from(e: anyhow::Error) -> Self {
        SDKError::FromStrError(e.to_string())
    }
}
