use crate::arena::Arena;
use crate::compile::compile;
use crate::data::Environment;
use crate::eval::Routine;
use crate::reader::{Error, Reader};
use fn_macros::lisp_fn;

use anyhow::{anyhow, Result};

use std::fs;

pub fn read_from_string<'ob>(
    contents: &str,
    arena: &'ob Arena,
    env: &mut Environment<'ob>,
) -> Result<bool> {
    let mut pos = 0;
    loop {
        println!("reading");
        let (obj, new_pos) = match Reader::read(&contents[pos..], arena) {
            Ok((obj, pos)) => (obj, pos),
            Err(Error::EmptyStream) => return Ok(true),
            Err(e) => return Err(anyhow!(e)),
        };
        println!("-----read-----\n {}", &contents[pos..(new_pos + pos)]);
        println!("compiling");
        // this will go out of scope
        let exp = compile(obj, arena)?;
        println!("running");
        println!("codes: {:?}", exp.op_codes);
        println!("const: {:?}", exp.constants);
        Routine::execute(&exp, env, arena)?;
        assert_ne!(new_pos, 0);
        pos += new_pos;
    }
}

#[lisp_fn]
#[allow(clippy::ptr_arg)]
fn load<'ob>(file: &String, arena: &'ob Arena, env: &mut Environment<'ob>) -> Result<bool> {
    let file_contents = fs::read_to_string(file)?;
    read_from_string(&file_contents, arena, env)
}

defsubr!(load);

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_load() {
        let arena = &Arena::new();
        let env = &mut Environment::default();
        read_from_string("(setq foo 1) (setq bar 2) (setq baz 1.5)", arena, env).unwrap();
        println!("{:?}", env);
        println!("{:?}", arena);

        let obj = Reader::read("(+ foo bar baz)", arena).unwrap().0;
        let func = compile(obj, arena).unwrap();
        let val = Routine::execute(&func, env, arena).unwrap();
        assert_eq!(val, arena.add(4.5));
    }
}