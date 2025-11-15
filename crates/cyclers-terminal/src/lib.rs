#![cfg(any(
    not(target_family = "wasm"),
    all(target_os = "wasi", target_env = "p2")
))]

pub use self::driver::{
    ReadLineError, ReadLineErrorKind, TerminalCommand, TerminalDriver, TerminalSource,
};

mod driver;
mod sys;
