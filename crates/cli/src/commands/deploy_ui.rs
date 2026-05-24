use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use colored::{Color, Colorize};
use comfy_table::presets::ASCII_BORDERS_ONLY_CONDENSED;
use comfy_table::{Cell, CellAlignment, Table};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleType {
    Commands,
    Effects,
    Projectors,
}

impl ModuleType {
    pub fn as_str(self) -> &'static str {
        match self {
            ModuleType::Commands => "commands",
            ModuleType::Effects => "effects",
            ModuleType::Projectors => "projectors",
        }
    }

    pub fn color(self) -> Color {
        match self {
            ModuleType::Commands => Color::Cyan,
            ModuleType::Effects => Color::Magenta,
            ModuleType::Projectors => Color::Blue,
        }
    }

    pub fn all() -> [ModuleType; 3] {
        [
            ModuleType::Commands,
            ModuleType::Effects,
            ModuleType::Projectors,
        ]
    }
}

#[derive(Debug)]
pub enum Status {
    Built { dur: Duration },
    Unchanged,
    Deployed,
    Bumped { from: String, to: String },
    Failed { msg: String },
}

#[derive(Default, Debug, Clone, Copy)]
pub struct Stats {
    pub deployed: usize,
    pub bumped: usize,
    pub unchanged: usize,
    pub failed: usize,
}

struct Row {
    bar: ProgressBar,
    version: String,
}

pub struct DeployUi {
    mp: MultiProgress,
    rows: Mutex<BTreeMap<(ModuleType, String), Row>>,
    name_width: usize,
    // Display width (chars) reserved for the version segment so the trailing
    // status text aligns the same on bumped and unchanged rows alike.
    version_col_width: usize,
}

impl DeployUi {
    pub fn new(name_width: usize, max_version_len: usize) -> Self {
        // Widest possible version segment is "v{from} → v{to}" which is
        // 1 + N + 3 + 1 + N = 2N + 5 visible chars (where N = max version len).
        let version_col_width = (2 * max_version_len) + 5;
        Self {
            mp: MultiProgress::new(),
            rows: Mutex::new(BTreeMap::new()),
            name_width: name_width.max(20),
            version_col_width,
        }
    }

    pub fn println(&self, msg: impl AsRef<str>) {
        let _ = self.mp.println(msg.as_ref());
    }

    pub fn begin_phase(&self, label: &str, count: usize) {
        self.println("");
        self.println(format!(
            "{} {} {}",
            "[+]".bold().blue(),
            label.bold(),
            format!("{count} module(s)").dimmed()
        ));
        self.println("");
    }

    pub fn add_category_header(&self, ty: ModuleType, count: usize) {
        let header = self.mp.add(ProgressBar::new(0));
        header.set_style(ProgressStyle::with_template("{msg}").unwrap());
        header.finish_with_message(format!(
            "  {} {}",
            ty.as_str().color(ty.color()).bold(),
            format!("({count})").dimmed()
        ));
    }

    pub fn register(&self, ty: ModuleType, name: &str, version: &str, action: &str) {
        let color = color_name(ty.color());
        let bar = self.mp.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::with_template(&format!(
                "    {{spinner:.{color}}} {{prefix}} {{msg}}"
            ))
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
        );
        bar.set_prefix(format!("{:<width$}", name, width = self.name_width));
        bar.set_message(self.format_version_line(
            &format!("v{version}"),
            &action.dimmed().to_string(),
        ));
        bar.enable_steady_tick(Duration::from_millis(80));

        let mut rows = self.rows.lock().unwrap();
        rows.insert(
            (ty, name.to_string()),
            Row {
                bar,
                version: version.to_string(),
            },
        );
    }

    pub fn set_action(&self, ty: ModuleType, name: &str, action: &str) {
        let rows = self.rows.lock().unwrap();
        if let Some(row) = rows.get(&(ty, name.to_string())) {
            row.bar.set_message(self.format_version_line(
                &format!("v{}", row.version),
                &action.dimmed().to_string(),
            ));
        }
    }

    pub fn finish(&self, ty: ModuleType, name: &str, status: Status) {
        let row = {
            let rows = self.rows.lock().unwrap();
            match rows.get(&(ty, name.to_string())) {
                Some(r) => Row {
                    bar: r.bar.clone(),
                    version: r.version.clone(),
                },
                None => return,
            }
        };

        let (glyph, message) = match &status {
            Status::Built { dur } => (
                "✓".green().bold().to_string(),
                self.format_version_line(
                    &format!("v{}", row.version),
                    &format_dur(*dur).dimmed().to_string(),
                ),
            ),
            Status::Unchanged => (
                "─".dimmed().to_string(),
                self.format_version_line(
                    &format!("v{}", row.version),
                    &"unchanged".dimmed().to_string(),
                ),
            ),
            Status::Deployed => (
                "✓".green().bold().to_string(),
                self.format_version_line(
                    &format!("v{}", row.version),
                    &"deployed".green().to_string(),
                ),
            ),
            Status::Bumped { from, to } => {
                let plain = format!("v{from} → v{to}");
                let colored = format!(
                    "v{} {} {}",
                    from,
                    "→".yellow(),
                    format!("v{to}").yellow().bold(),
                );
                (
                    "↑".yellow().bold().to_string(),
                    self.format_version_line_with_widths(
                        &colored,
                        plain.chars().count(),
                        &"bumped & deployed".yellow().to_string(),
                    ),
                )
            }
            Status::Failed { msg } => (
                "✗".red().bold().to_string(),
                self.format_version_line(
                    &format!("v{}", row.version),
                    &msg.red().to_string(),
                ),
            ),
        };

        row.bar.disable_steady_tick();
        row.bar
            .set_style(ProgressStyle::with_template("    {prefix} {msg}").unwrap());
        row.bar.set_prefix(format!(
            "{} {:<width$}",
            glyph,
            name,
            width = self.name_width
        ));
        row.bar.finish_with_message(message);
    }

    /// Pad the version segment to `version_col_width` so the trailing status
    /// text aligns across rows. `plain_version` is the version string
    /// without ANSI codes (for width measurement).
    fn format_version_line(&self, plain_version: &str, tail: &str) -> String {
        self.format_version_line_with_widths(
            plain_version,
            plain_version.chars().count(),
            tail,
        )
    }

    fn format_version_line_with_widths(
        &self,
        version_display: &str,
        version_visible_len: usize,
        tail: &str,
    ) -> String {
        let pad = self.version_col_width.saturating_sub(version_visible_len);
        format!("{version_display}{}  {tail}", " ".repeat(pad))
    }

    pub fn print_summary(
        &self,
        stats: &BTreeMap<ModuleType, Stats>,
        elapsed: Duration,
        title: &str,
        total: usize,
    ) {
        // Use plain eprintln (not mp.println) so we don't trigger a redraw of
        // every finished bar between each printed line — that interleaves the
        // table with bar contents.
        eprintln!();

        let total_failed: usize = stats.values().map(|s| s.failed).sum();
        let header_glyph = if total_failed == 0 {
            "[✓]".bold().green().to_string()
        } else {
            "[✗]".bold().red().to_string()
        };

        eprintln!(
            "{} {} {} {} {}",
            header_glyph,
            title.bold(),
            format!("{total} module(s)").bold(),
            "in".dimmed(),
            format_dur(elapsed).bold(),
        );
        eprintln!();

        let mut table = Table::new();
        table
            .load_preset(ASCII_BORDERS_ONLY_CONDENSED)
            .set_header(vec![
                Cell::new("category"),
                Cell::new("deployed").set_alignment(CellAlignment::Right),
                Cell::new("bumped").set_alignment(CellAlignment::Right),
                Cell::new("unchanged").set_alignment(CellAlignment::Right),
                Cell::new("failed").set_alignment(CellAlignment::Right),
            ]);

        for ty in ModuleType::all() {
            let Some(s) = stats.get(&ty) else { continue };
            let deployed_cell = cell_num(s.deployed, comfy_table::Color::Green);
            let bumped_cell = cell_num(s.bumped, comfy_table::Color::Yellow);
            let failed_cell = cell_num(s.failed, comfy_table::Color::Red);
            table.add_row(vec![
                Cell::new(ty.as_str()).fg(to_comfy_color(ty.color())),
                deployed_cell,
                bumped_cell,
                Cell::new(s.unchanged.to_string()).set_alignment(CellAlignment::Right),
                failed_cell,
            ]);
        }

        for line in table.to_string().lines() {
            eprintln!("  {line}");
        }
        eprintln!();
    }
}

fn cell_num(n: usize, color: comfy_table::Color) -> Cell {
    let cell = Cell::new(n.to_string()).set_alignment(CellAlignment::Right);
    if n > 0 { cell.fg(color) } else { cell }
}

fn format_dur(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 1.0 {
        format!("{}ms", d.as_millis())
    } else if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        let mins = (secs / 60.0).floor() as u64;
        let rem = secs - (mins as f64 * 60.0);
        format!("{mins}m{rem:.1}s")
    }
}

fn color_name(c: Color) -> &'static str {
    match c {
        Color::Black => "black",
        Color::Red => "red",
        Color::Green => "green",
        Color::Yellow => "yellow",
        Color::Blue => "blue",
        Color::Magenta => "magenta",
        Color::Cyan => "cyan",
        Color::White => "white",
        Color::BrightBlack => "bright.black",
        Color::BrightRed => "bright.red",
        Color::BrightGreen => "bright.green",
        Color::BrightYellow => "bright.yellow",
        Color::BrightBlue => "bright.blue",
        Color::BrightMagenta => "bright.magenta",
        Color::BrightCyan => "bright.cyan",
        Color::BrightWhite => "bright.white",
        _ => "white",
    }
}

fn to_comfy_color(c: Color) -> comfy_table::Color {
    match c {
        Color::Black => comfy_table::Color::Black,
        Color::Red => comfy_table::Color::Red,
        Color::Green => comfy_table::Color::Green,
        Color::Yellow => comfy_table::Color::Yellow,
        Color::Blue => comfy_table::Color::Blue,
        Color::Magenta => comfy_table::Color::Magenta,
        Color::Cyan => comfy_table::Color::Cyan,
        Color::White => comfy_table::Color::White,
        _ => comfy_table::Color::Reset,
    }
}
