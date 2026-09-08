use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("terminal error: {0}")]
    Terminal(#[from] std::io::Error),
}
