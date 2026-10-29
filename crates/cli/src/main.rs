//! Thin binary wrapper: the library owns parsing, exit codes and all work.

fn main() {
    let code = open_canvas_cli::run(std::env::args().skip(1));
    if code != 0 {
        std::process::exit(code);
    }
}
