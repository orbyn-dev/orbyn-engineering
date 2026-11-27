//! A program a test can run in a terminal to make the service's block recorder see
//! OSC 133 marks without a shell integration: two finished blocks, or one finished
//! block and one that keeps running.

use std::io::{Read, Write};

fn emit(bytes: &[u8]) {
    std::io::stdout().write_all(bytes).unwrap();
    std::io::stdout().flush().unwrap();
}

/// Two finished blocks: `first\n` and `second\n`, with commands and exit codes.
pub(crate) fn finished() {
    emit(
        b"\x1b]633;E;echo first\x07\x1b]133;C\x07first\n\x1b]133;D;0\x07\
          \x1b]633;E;echo second\x07\x1b]133;C\x07second\n\x1b]133;D;1\x07",
    );
}

/// One finished block (`done\n`) and one that stays running (`still-running\n`)
/// until the terminal closes.
pub(crate) fn running() {
    emit(
        b"\x1b]633;E;echo done\x07\x1b]133;C\x07done\n\x1b]133;D;0\x07\
          \x1b]633;E;sleep\x07\x1b]133;C\x07still-running\n",
    );
    let mut sink = Vec::new();
    let _ = std::io::stdin().lock().read_to_end(&mut sink);
}
