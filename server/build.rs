#[path = "src/ui_build.rs"]
mod ui_build;

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=SQL_MCP_SKIP_UI_BUILD");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ui = manifest.join("../semantic-ui");
    let ui = ui.canonicalize().unwrap_or(ui);
    for rel in [
        "src",
        "package.json",
        "package-lock.json",
        "vite.config.ts",
        "tsconfig.json",
        "index.html",
    ] {
        println!("cargo:rerun-if-changed={}", ui.join(rel).display());
    }

    if skip_ui_build() {
        return;
    }
    if !ui_build::dist_is_stale(&ui) {
        return;
    }
    if !ui.join("package.json").is_file() {
        panic!(
            "semantic-ui/package.json missing at {} (needed for GET /ui)",
            ui.display()
        );
    }
    ensure_node_modules(&ui);
    run_npm(&ui, &["run", "build"]);
}

fn skip_ui_build() -> bool {
    match std::env::var("SQL_MCP_SKIP_UI_BUILD") {
        Ok(v) => v != "0" && v != "false" && !v.is_empty(),
        Err(_) => false,
    }
}

fn npm() -> Command {
    if cfg!(windows) {
        Command::new("npm.cmd")
    } else {
        Command::new("npm")
    }
}

fn ensure_node_modules(ui: &Path) {
    if ui.join("node_modules").is_dir() {
        return;
    }
    run_npm(ui, &["ci"]);
}

fn run_npm(ui: &Path, args: &[&str]) {
    let status = npm()
        .current_dir(ui)
        .args(args)
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "npm {} failed to start in {}: {e}",
                args.join(" "),
                ui.display()
            )
        });
    if !status.success() {
        panic!(
            "npm {} failed in {} with {status}",
            args.join(" "),
            ui.display()
        );
    }
}
