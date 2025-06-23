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
    CustomError = String;
    ManualError = crate::error::ManualError;
    FragmentError = crate::error::FragmentError;
    IoError = std::io::Error;
    DecodeError = std::array::TryFromSliceError
}

impl global::Error {
    pub fn custom(str: impl AsRef<str>) -> Self {
        global::Inner::CustomError(str.as_ref().to_string()).into()
    }
}

pub type Result<T> = ::std::result::Result<T, global::Error>;

use std::marker::PointeeSized;
pub use global::Error;

#[derive(Debug, Clone)]
pub enum ManualError {
    BackingObjectMissing,
}

impl std::error::Error for ManualError {}

impl std::fmt::Display for ManualError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}



#[derive(Debug, Clone)]
pub enum FragmentError {
    NoFound(crate::FragmentID),
    MissingRootFragment,
    InvalidFragmentTable,
    InvalidMagic,
    InvalidTable,
    LengthExceedsCapacity,
    FailedToCreateNewFragmentTablePart,
}

impl std::error::Error for FragmentError {}

impl std::fmt::Display for FragmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl FragmentError {
    pub fn not_found<T>(id: crate::FragmentID) -> Result<T> {
        Err(Self::NoFound(id).into())
    }
    
    pub fn invalid_fragment_table<T>() -> Result<T> {
        Err(Self::InvalidFragmentTable.into())
    }
    
}