macro_rules! multi_error {
    ($name:ident($($manual:ident),*); $($err:ident = $obj:ty);*) => {
        pub mod $name {
            use backtrace::Backtrace;

            #[derive(Debug)]
            pub enum Inner {
                $($err($obj),)*
                $($manual),*
            }

            impl std::fmt::Display for Inner { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { std::fmt::Debug::fmt(self, f) } }
            impl std::error::Error for Inner {}

            $(impl From<$obj> for Inner { fn from(value: $obj) -> Self { Self::$err(value) } })*

            pub struct Error {
                inner: Inner,
                backtrace: Backtrace
            }

            impl<Err> From<Err> for Error where Err: Into<Inner> {
                fn from(err: Err) -> Self {
                    Self {
                        inner: err.into(),
                        backtrace: Backtrace::new()
                    }
                }
            }

            impl std::error::Error for Error {}
            impl std::fmt::Display for Error {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { std::fmt::Debug::fmt(self, f) }
            }

            impl std::fmt::Debug for Error {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}\n", &self.inner)?;
                    match std::env::var("RUST_BACKTRACE").as_ref().map(|i| i.as_ref()) {
                        Ok("full") => write!(f, "{:#?}", self.backtrace),
                        Ok("1") => write!(f, "{:?}", self.backtrace),
                        _ => write!(f, ""),
                    }
                }
            }
        }
    }
}

multi_error! { global();
    ManualError = crate::error::ManualError;
    TokenError = crate::error::TokenError;
    SerdeJsonError = serde_json::error::Error;
    ReqwestError = reqwest::Error;
    OsError = rand::rand_core::OsError;
    IoError = std::io::Error
}

pub type Result<T> = ::std::result::Result<T, global::Error>;
pub use global::Error;

#[derive(Debug, Clone)]
pub enum ManualError {
    AppStateMissing,
}

impl std::error::Error for ManualError {}
impl std::fmt::Display for ManualError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Clone)]
pub enum TokenError {
    MissingToken,
    InvalidToken,
    ExpiredToken,
    NoUser,
}

impl std::error::Error for TokenError {}
impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Clone)]
pub enum AppError {
    MissingToken,
    InvalidToken,
    ExpiredToken,
    NoApp,
}

impl std::error::Error for AppError {}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}