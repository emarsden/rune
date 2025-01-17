use crate::core::object::{GcObj, Object};
use anyhow::{bail, ensure, Result};
use fn_macros::defun;
use std::{fmt::Write as _, io::Write};

#[defun]
fn message(format_string: &str, args: &[GcObj]) -> Result<String> {
    let message = format(format_string, args)?;
    println!("MESSAGE: {message}");
    std::io::stdout().flush()?;
    Ok(message)
}

defvar!(MESSAGE_NAME);
defvar!(MESSAGE_TYPE, "new message");

#[defun]
fn format(string: &str, objects: &[GcObj]) -> Result<String> {
    let mut result = String::new();
    let mut iter = objects.iter();
    // "%%" inserts a single "%" in the output
    for segment in string.split("%%") {
        let mut last_end = 0;
        let mut escaped = false;
        let is_format_char = |c: char| {
            if escaped {
                escaped = false;
                false
            } else if c == '\\' {
                escaped = true;
                false
            } else {
                c == '%'
            }
        };
        for (start, _) in segment.match_indices(is_format_char) {
            result.push_str(&segment[last_end..start]);
            // TODO: currently handles all format types the same. Need to check the modifier characters.
            let Some(val) = iter.next() else {bail!("Not enough objects for format string")};
            match val.untag() {
                Object::String(s) => result.push_str(s.try_into()?),
                obj => write!(result, "{obj}")?,
            }
            last_end = start + 2;
        }
        result.push_str(&segment[last_end..segment.len()]);
        result.push_str("%");
    }
    result.pop();  // the last "%"
    ensure!(
        iter.next().is_none(),
        "Too many arguments for format string"
    );
    Ok(result)
}

#[defun]
fn format_message(string: &str, objects: &[GcObj]) -> Result<String> {
    let formatted = format(string, objects)?;
    // TODO: implement support for `text-quoting-style`.
    Ok(formatted
        .chars()
        .map(|c| if matches!(c, '`' | '\'') { '"' } else { c })
        .collect())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_format() {
        assert_eq!(&format("%s", &[1.into()]).unwrap(), "1");
        assert_eq!(&format("foo-%s", &[2.into()]).unwrap(), "foo-2");
        assert_eq!(
            &format("foo-%s %s", &[3.into(), 4.into()]).unwrap(),
            "foo-3 4"
        );
        let sym = crate::core::env::sym::FUNCTION.into();
        assert_eq!(&format("%s", &[sym]).unwrap(), "function");

        assert!(&format("%s", &[]).is_err());
        assert!(&format("%s", &[1.into(), 2.into()]).is_err());
    }
}
