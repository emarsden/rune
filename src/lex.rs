#![allow(dead_code)]
use std::str;
use std::fmt;

pub struct Lexer<'a> {
    slice: &'a str,
    start: *const u8,
}

#[derive(PartialEq, Debug)]
pub enum Token<'a> {
    Symbol(&'a str),
    String(&'a str),
    Integer(&'a str),
    Float(&'a str),
    OpenParen(&'a str),
    CloseParen(&'a str),
    Quote(&'a str),
    QuasiQuote(&'a str),
    MacroEval(&'a str),
    MacroSplice(&'a str),
}

impl<'a> Token<'a> {
    fn inner(&self) -> &str {
        use Token::*;
        match self {
            Symbol(x) | String(x) | Integer(x) | Float(x) |
            OpenParen(x) | CloseParen(x) | Quote(x) |
            QuasiQuote(x) | MacroEval(x) | MacroSplice(x) => x
        }
    }

    pub fn len(&self) -> usize {
        self.inner().len()
    }

    pub fn start(&self) -> *const u8 {
        self.inner().as_ptr()
    }

    /// Classifies the identifier as a Symbol, Integer, or Float. Based on
    /// [this documentation](https://www.gnu.org/software/emacs/manual/html_node/elisp/Symbol-Type.html).
    fn classify(token: &'a str) -> Token<'a> {
        use Token::*;
        let mut chars = token.chars();
        let mut point_found = false;
        match chars.next() {
            None => return Symbol(token),
            Some(chr) => {
                match chr {
                    '.' => point_found = true,
                    '0'..='9' | '+' | '-' => {},
                    _ => return Symbol(token)
                }
            }
        };

        while let Some(chr) = chars.next() {
            match chr {
                '.' if point_found => return Symbol(token),
                '.' => point_found = true,
                '0'..='9' => {},
                _ => return Symbol(token),
            }
        }
        if point_found {
            Float(token)
        } else {
            Integer(token)
        }
    }
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner())
    }
}

impl<'a> Lexer<'a> {
    pub fn new(slice: &'a str) -> Self {
        Lexer {
            slice,
            start: slice.as_ptr(),
        }
    }

    fn clear(&mut self) {
        self.slice = &self.slice[self.slice.len()..];
    }

    fn advance(&mut self, amount: usize) {
        self.slice = &self.slice[amount..];
    }

    fn get_symbol(&mut self, beg: usize, mut chars: str::CharIndices) -> &'a str {
        let mut escaped = false;
        while let Some((end, chr)) = chars.next() {
            if escaped || chr == '\\' {
                escaped = !escaped;
                chars.next();
            } else if !symbol_char(chr) {
                return &self.slice[beg..end];
            }
        }
        &self.slice[beg..]
    }

    fn get_string(&mut self, beg: usize, mut chars: str::CharIndices) -> &'a str {
        let mut escaped = false;
        while let Some((end, chr)) = chars.next() {
            if escaped || chr == '\\' {
                escaped = !escaped;
                chars.next();
            } else if chr == '"' {
                return &self.slice[beg..end+1];
            }
        }
        &self.slice[beg..]
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut chars = self.slice.char_indices();
        let mut in_comment = false;

        let mut symbol_start = |chr: char| {
            if in_comment {
                if chr == '\n' { in_comment = false; }
                false
            } else if chr.is_ascii_whitespace() {
                false
            } else if chr == ';' {
                in_comment = true;
                false
            } else {
                true
            }
        };

        let (idx, chr) = chars.find(|x| symbol_start(x.1))?;

        let token = match chr {
            c if symbol_char(c) => Token::classify(self.get_symbol(idx, chars)),
            '"' => Token::String(self.get_string(idx, chars)),
            '(' => Token::OpenParen(&self.slice[idx..idx+1]),
            ')' => Token::CloseParen(&self.slice[idx..idx+1]),
            '`' => Token::QuasiQuote(&self.slice[idx..idx+1]),
            '\'' => Token::Quote(&self.slice[idx..idx+1]),
            x => { panic!("unknown token: {}", x); }
        };
        self.advance(idx + token.len());
        Some(token)
    }
}

fn symbol_char(chr: char) -> bool {
    match chr {
        '\x00'..=' ' |
        '(' | ')' | '[' | ']' |
        '#' | ',' | '`' | ';' |
        '"' | '\'' => false,
        _ => true,
    }
}

pub fn run() {
    let mut lexer = Lexer::new(r#"(foo (bar) -2.3 'word) +1 "this is a string ; \" with stuff in " ; comment"#);
    while let Some(s) = lexer.next() {
        println!("\"{:?}\"", s);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn size() {
        assert_eq!(24, std::mem::size_of::<Token>());
    }

    #[test]
    fn symbols() {
        let symbols: Vec<Token> = Lexer::new("(foo (bar) baz 'word) bob").collect();

        let golden = vec![
            Token::OpenParen("("),
            Token::Symbol("foo"),
            Token::OpenParen("("),
            Token::Symbol("bar"),
            Token::CloseParen(")"),
            Token::Symbol("baz"),
            Token::Quote("'"),
            Token::Symbol("word"),
            Token::CloseParen(")"),
            Token::Symbol("bob")
        ];

        assert_eq!(golden, symbols);
    }

    #[test]
    fn string() {
        let symbols: Vec<Token> = Lexer::new(r#"before "string with \" stuff" after"#).collect();
        let golden = vec![
            Token::Symbol("before"),
            Token::String(r#""string with \" stuff""#),
            Token::Symbol("after"),
        ];

        assert_eq!(golden, symbols);
    }

    #[test]
    fn comments() {
        let symbols: Vec<Token> = Lexer::new("before ;; comment \n after").collect();
        let golden = vec![
            Token::Symbol("before"),
            Token::Symbol("after"),
        ];
        assert_eq!(golden, symbols);
    }

    #[test]
    fn numbers() {
        let symbols: Vec<Token> = Lexer::new("+1 1+ 8. -1 \\-1 .1 2.0 3.0.0 --1").collect();
        let golden = vec![
            Token::Integer("+1"),
            Token::Symbol("1+"),
            Token::Float("8."),
            Token::Integer("-1"),
            Token::Symbol("\\-1"),
            Token::Float(".1"),
            Token::Float("2.0"),
            Token::Symbol("3.0.0"),
            Token::Symbol("--1"),
        ];
        assert_eq!(golden, symbols);
    }
}
