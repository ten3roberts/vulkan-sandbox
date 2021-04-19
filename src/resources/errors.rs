use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0} not found with name {1:?}")]
    NotFound(&'static str, String),
    #[error("{0} handle is not valid to dereference")]
    InvalidHandle(&'static str),
}
