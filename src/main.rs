use galion::galion_main;
use std::process::exit;

fn main() {
    match galion_main() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{e}");
            exit(1)
        }
    }
}
