#[derive(Debug)]
pub(crate) enum Error {
    InvalidArgs,
    VerifySmtFail,
}

impl Error {
    pub fn error_code(&self) -> i8 {
        match self {
            Error::InvalidArgs => 1,
            Error::VerifySmtFail => 2,
        }
    }
}
