//! Compile the Vue UI into `ui/dist` before embedding.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=ui/src");
    println!("cargo:rerun-if-changed=ui/index.html");
    println!("cargo:rerun-if-changed=ui/package.json");
    println!("cargo:rerun-if-changed=ui/package-lock.json");
    println!("cargo:rerun-if-changed=ui/vite.config.ts");
    println!("cargo:rerun-if-changed=ui/tsconfig.json");

    // Cross-builds and constrained deployment hosts may already carry a
    // verified ui/dist artifact. Skipping is explicit and never the default.
    if std::env::var_os("GATEWAY_KIT_SKIP_UI_BUILD").is_some() {
        return;
    }

    let ui = Path::new("ui");
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    if !ui.join("node_modules").join("vite").exists() {
        let install_ok = if ui.join("package-lock.json").exists() {
            run(npm, &["ci"], ui)
        } else {
            run(npm, &["install"], ui)
        };
        if !install_ok {
            panic!(
                "npm ci/install failed in crates/gateway-app/ui (Node ≥ 20 required to build gateway-app)"
            );
        }
    }
    if !run(npm, &["run", "build"], ui) {
        panic!("npm run build failed in crates/gateway-app/ui");
    }
}

fn run(npm: &str, args: &[&str], dir: &Path) -> bool {
    Command::new(npm)
        .args(args)
        .current_dir(dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
