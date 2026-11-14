//! `echo` plugin agent: returns its input. The smallest plugin there is, and
//! the one to copy when writing a new one.

use kit::*;

struct Echo;

impl Guest for Echo {
    fn run(input: Vec<Message>) -> Reply {
        Ok(text(&input).to_string())
    }
}

kit::export!(Echo);
