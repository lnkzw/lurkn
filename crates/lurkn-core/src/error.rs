use thiserror::Error;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("not a valid Twitch login: {reason}")]
    InvalidLogin { reason: &'static str },
}
