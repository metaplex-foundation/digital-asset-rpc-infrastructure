use hyper::StatusCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Parameter Invalid")]
    ParameterInvalid,
    #[error("Request Error {status:?}, reason {msg:?} ")]
    ResponseError { status: StatusCode, msg: String },
}
