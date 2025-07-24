pub use self::driver::{
    TerminalCommand, TerminalDriver, TerminalReadError, TerminalReadErrorKind, TerminalSource,
};

mod driver;
mod sys;
