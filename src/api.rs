use std::convert::From;
use std::error::Error as StdError;
use std::fmt;
use std::time;

use hyper::{Body, Response, StatusCode};
use serde::export::fmt::Display;
use serde::Serialize;

use crate::context::debug;
use crate::database::CreationError;

pub type Request = hyper::Request<hyper::Body>;
pub type Result = std::result::Result<hyper::Response<hyper::Body>, Error>;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    #[serde(rename = "type")]
    kind: &'static str,
    pub message: String,
    pub status_code: u16,
}


impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl StdError for Error {}

impl Error {
    pub fn new<T: ToString>(message: T, status: StatusCode) -> Error {
        Error {
            kind: "error",
            message: message.to_string(),
            status_code: status.as_u16(),
        }
    }

    pub fn not_found() -> Error {
        Error::new("Not found requested resources.", StatusCode::NOT_FOUND)
    }

    pub fn internal() -> Error {
        Error::new("Server internal error.", StatusCode::INTERNAL_SERVER_ERROR)
    }

    pub fn bad_request() -> Error {
        Error::new("Bad request.", StatusCode::BAD_REQUEST)
    }

    pub fn method_not_allowed() -> Error {
        Error::new("Method not allowed", StatusCode::METHOD_NOT_ALLOWED)
    }

    pub fn unexpected(e: &dyn StdError) -> Error {
        let mut error = Error::internal();
        if debug() {
            error.message = e.to_string();
        }
        error
    }

    pub fn build(&self) -> Response<Body> {
        let bytes = serde_json::to_vec(self)
            .unwrap_or_else(|_| serde_json::to_vec(&Error::internal()).unwrap());
        let status = StatusCode::from_u16(self.status_code)
            .expect("invalid struct code");
        Response::builder()
            .status(status)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Body::from(bytes))
            .expect("failed to build response")
    }
}

impl From<CreationError> for Error {
    fn from(e: CreationError) -> Error {
        match e {
            CreationError::AlreadyExists => Error::new("This record already exists.", StatusCode::CONFLICT),
            CreationError::ValidationFail(message) => Error::new(message, StatusCode::FORBIDDEN),
            e => Error::unexpected(&e),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Return<'a, T: Serialize> {
    value: &'a T,
    #[serde(rename = "type")]
    kind: &'static str,
    status_code: u16,
    delta: Option<f64>,
}

impl<'a, T: Serialize> Return<'a, T> {
    pub fn new(value: &'a T) -> Return<'a, T> {
        Return {
            value,
            kind: "return",
            status_code: 200,
            delta: None,
        }
    }

    pub fn status(self, s: StatusCode) -> Return<'a, T> {
        let status_code = s.as_u16();
        Return { status_code, ..self }
    }

    pub fn start_at(self, t: time::SystemTime) -> Return<'a, T> {
        let now = time::SystemTime::now();
        let delta = Some(now.duration_since(t).unwrap().as_secs_f64());
        Return { delta, ..self }
    }

    pub fn build(&self) -> Result {
        let bytes = serde_json::to_vec(self)
            .map_err(|_| Error::bad_request())?;

        Response::builder()
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .status(StatusCode::from_u16(self.status_code).unwrap())
            .body(Body::from(bytes))
            .map_err(|_| Error::internal())
    }
}
