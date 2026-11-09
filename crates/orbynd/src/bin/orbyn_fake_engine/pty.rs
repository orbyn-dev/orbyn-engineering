use std::io::BufRead;

/// A line-echoing terminal program. `exit` ends it.
pub(crate) fn pty() {
    println!("fake terminal ready");

    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        if line.contains("exit") {
            return;
        }

        println!("echo: {line}");
    }
}
