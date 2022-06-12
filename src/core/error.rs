use std::fmt::Display;

use super::object::Object;

/// Errors the are used in more then 1 module.
#[derive(Debug, PartialEq)]
pub(crate) enum Error {
    /// The function or form has the wrong number of arguments. First number is
    /// the expected number, second is the actual.
    ArgCount(u16, u16, String),
    /// Object was the wrong type.
    Type(Type, Type, String),
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ArgCount(exp, act, name) => {
                write!(
                    f,
                    "Expected {exp} argument(s) for `{name}', but found {act}"
                )
            }
            Error::Type(exp, act, print) => {
                write!(f, "expected {exp:?}, found {act:?}: {print}")
            }
        }
    }
}

impl Error {
    /// Get a type error from an object.
    pub(crate) fn from_object<'ob, T>(exp: Type, obj: T) -> Self
    where
        T: Into<Object<'ob>>,
    {
        let obj = obj.into();
        Error::Type(exp, obj.get_type(), obj.to_string())
    }

    pub(crate) fn arg_count<T: AsRef<str>>(exp: u16, actual: u16, name: T) -> Error {
        Error::ArgCount(exp, actual, name.as_ref().to_owned())
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Type {
    Int,
    True,
    Nil,
    Cons,
    Vec,
    HashTable,
    Sequence,
    String,
    Symbol,
    Float,
    Func,
    Number,
    List,
}
