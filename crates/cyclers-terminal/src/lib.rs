#[cfg(all(
    target_family = "wasm",
    not(all(target_os = "wasi", target_env = "p2"))
))]
compile_error!("wasm targets other than wasm32-wasip2 are not supported");

pub use self::driver::{
    ReadLineError, ReadLineErrorKind, TerminalCommand, TerminalDriver, TerminalSource,
};

mod driver;
mod sys;
