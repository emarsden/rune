use std::fmt::Display;

use crate::object::Object;

/// Errors the are used in more then 1 module.
#[derive(Debug, PartialEq)]
pub(crate) enum Error {
    /// The function or form has the wrong number of arguments. First number is
    /// the expected number, second is the actual.
    ArgCount(u16, u16),
    /// Object was the wrong type.
    Type(Type, Type, String),
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ArgCount(exp, act) => write!(f, "Expected {} arg(s), found {}", exp, act),
            Error::Type(exp, act, print) => {
                write!(f, "expected {:?}, found {:?}: {}", exp, act, print)
            }
        }
    }
}

impl Error {
    /// Get a type error from an object.
    pub(crate) fn from_object(exp: Type, obj: Object) -> Self {
        Error::Type(exp, obj.get_type(), obj.to_string())
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Type {
    Int,
    True,
    Nil,
    Cons,
    Vec,
    Sequence,
    String,
    Symbol,
    Float,
    Func,
    Number,
    List,
}
