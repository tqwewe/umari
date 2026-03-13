use colored::Colorize;
use comfy_table::{Cell, Table};
use umari_types::{ActiveModuleInfo, ModuleDetailsResponse, ModuleSummary, VersionDetailsResponse};

pub fn print_modules_table(modules: &[ModuleSummary]) {
    if modules.is_empty() {
        println!("no modules found");
        return;
    }

    let mut table = Table::new();
    table.set_header(vec!["NAME", "ACTIVE VERSION", "VERSIONS"]);

    for module in modules {
        table.add_row(vec![
            Cell::new(&module.name),
            Cell::new(module.active_version.as_deref().unwrap_or("-")),
            Cell::new(
                module
                    .versions
                    .iter()
                    .map(|v| v.version.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ]);
    }

    println!("{table}");
}

pub fn print_module_details(details: &ModuleDetailsResponse) {
    println!();
    println!("{}: {}", "MODULE".bold(), details.name);
    println!("{}: {}", "Type".bold(), details.module_type);
    println!(
        "{}: {}",
        "Active Version".bold(),
        details.active_version.as_deref().unwrap_or("-")
    );

    if !details.versions.is_empty() {
        println!();
        let mut table = Table::new();
        table.set_header(vec!["VERSION", "ACTIVE"]);

        for version in &details.versions {
            table.add_row(vec![
                Cell::new(&version.version),
                Cell::new(if version.active { "Yes" } else { "No" }),
            ]);
        }

        println!("{table}");
    }
}

pub fn print_version_details(details: &VersionDetailsResponse) {
    println!();
    println!("{}: {} v{}", "MODULE".bold(), details.name, details.version);
    println!("{}: {}", "Type".bold(), details.module_type);
    println!(
        "{}: {}",
        "Active".bold(),
        if details.active { "Yes" } else { "No" }
    );
    println!("{}: {}", "SHA256".bold(), details.sha256);
}

pub fn print_active_modules(modules: &[ActiveModuleInfo]) {
    if modules.is_empty() {
        println!("no active modules");
        return;
    }

    let mut table = Table::new();
    table.set_header(vec!["TYPE", "NAME", "VERSION"]);

    for module in modules {
        table.add_row(vec![
            Cell::new(&module.module_type),
            Cell::new(&module.name),
            Cell::new(&module.version),
        ]);
    }

    println!("{table}");
}
