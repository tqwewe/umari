use std::io::IsTerminal;

pub fn print_banner() {
    const ART: &[&str] = &[
        r" _   _  __  __      _    ____   ___ ",
        r"| | | ||  \/  |   / \   |  _ \ |_ _|",
        r"| | | || |\/| |  / _ \  | |_) | | | ",
        r"| |_| || |  | | / ___ \ |  _ <  | | ",
        r" \___/ |_|  |_|/_/   \_\|_| \_\|___|",
    ];

    const PAD: &str = "    ";
    const SUBTITLE: &str = "  wasm-native event sourcing";

    println!();
    if std::io::stdout().is_terminal() {
        for line in ART {
            println!("{PAD}\x1b[1m\x1b[36m{line}\x1b[0m");
        }
        println!("{PAD}\x1b[2m{SUBTITLE}\x1b[0m");
    } else {
        for line in ART {
            println!("{PAD}{line}");
        }
        println!("{PAD}{SUBTITLE}");
    }
    println!();
}
