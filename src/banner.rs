use std::env;
use std::io::{self, Write};

fn banner_text(suffix: &str) -> String {
    let no_color = env::var("NO_COLOR").is_ok();
    let (c, r) = if no_color {
        ("", "")
    } else {
        ("\x1b[96m", "\x1b[0m")
    };
    let logo = concat!(
        " _   _                      _____ _           _          _____  _____  _      \n",
        "| \\ | |                    /  __ \\ |         (_)        |  _  \\/  ___|| |     \n",
        "|  \\| | ___ _   _ _ __ ___ | /  \\/ |__   __ _ _ _ __    | | | |\\ `--. | |     \n",
        "| . ` |/ _ \\ | | | '__/ _ \\| |   | '_ \\ / _` | | '_ \\   | | | | `--. \\| |     \n",
        "| |\\  |  __/ |_| | | | (_) | \\__/\\ | | | (_| | | | | |  | |/ / /\\__/ /| |____ \n",
        "\\_| \\_/\\___|\\__,_|_|  \\___/ \\____/_| |_|\\__,_|_|_| |_|  |___/  \\____/ \\_____/\n",
    );
    format!("\n{c}{logo}{r}🌐 {suffix}\n")
}

pub fn print_banner() {
    let _ = io::stdout().write_all(
        banner_text("Welcome to NeuroChain CLI – built for AI, logic and elegance").as_bytes(),
    );
}

pub fn print_banner_stderr() {
    let _ = io::stderr().write_all(
        banner_text("Welcome to NeuroChain CLI – built for AI, logic and elegance").as_bytes(),
    );
}

pub fn print_server_banner() {
    let _ = io::stdout().write_all(
        banner_text("Welcome to NeuroChain API – built for AI, logic and elegance").as_bytes(),
    );
}
