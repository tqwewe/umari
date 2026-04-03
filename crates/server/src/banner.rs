use std::io::IsTerminal;

pub fn print_banner() {
    const ART: &[&str] = &[
        r"██╗   ██╗ ███╗   ███╗  █████╗  ██████╗  ██╗",
        r"██║   ██║ ████╗ ████║ ██╔══██╗ ██╔══██╗ ██║",
        r"██║   ██║ ██╔████╔██║ ███████║ ██████╔╝ ██║",
        r"██║   ██║ ██║╚██╔╝██║ ██╔══██║ ██╔══██╗ ██║",
        r"╚██████╔╝ ██║ ╚═╝ ██║ ██║  ██║ ██║  ██║ ██║",
        r" ╚═════╝  ╚═╝     ╚═╝ ╚═╝  ╚═╝ ╚═╝  ╚═╝ ╚═╝",
    ];

    const PAD: &str = "    ";
    const SUBTITLE: &str = "wasm-native event sourcing";

    println!();
    const COLORS: &[&str] = &[
        "\x1b[38;5;171m",
        "\x1b[38;5;135m",
        "\x1b[38;5;99m",
        "\x1b[38;5;93m",
        "\x1b[38;5;57m",
        "\x1b[38;5;54m",
    ];

    if std::io::stdout().is_terminal() {
        for (line, color) in ART.iter().zip(COLORS) {
            println!("{PAD}\x1b[1m{color}{line}\x1b[0m");
        }
        let art_width = ART[0].chars().count();
        let content_width = 3 + 2 + SUBTITLE.len() + 2 + 3;
        let offset = " ".repeat((art_width.saturating_sub(content_width)) / 2);
        let tl = "\x1b[38;5;57m╌\x1b[38;5;99m─\x1b[38;5;171m━\x1b[0m";
        let tr = "\x1b[38;5;171m━\x1b[38;5;99m─\x1b[38;5;57m╌\x1b[0m";
        println!("{PAD}{offset}{tl}  \x1b[2m{SUBTITLE}\x1b[0m  {tr}");
    } else {
        for line in ART {
            println!("{PAD}{line}");
        }
        println!("{PAD}{SUBTITLE}");
    }
    println!();
}
