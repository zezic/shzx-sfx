/// Errors at input, device, and control boundaries.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid symbol: {0}")]
    InvalidSymbol(&'static str),
    #[error("invalid {name}: {value}; expected {expected}")]
    InvalidParameter {
        name: &'static str,
        value: f64,
        expected: &'static str,
    },
    #[error("audio backend: {0}")]
    Audio(String),
    #[error("audio command queue is full")]
    QueueFull,
    #[error("audio engine has stopped")]
    Stopped,
    #[error("audio thread panicked")]
    ThreadPanicked,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
