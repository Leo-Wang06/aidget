use std::process::Command;

#[allow(dead_code)]
#[path = "src/runtime/command_names.rs"]
mod command_names;

/// Cross-target type-check hook.
///
/// `AIDGET_CHECK_TARGET=1` skips `tauri_build`, whose Windows resource step
/// needs `llvm-rc` and whose Linux step needs GTK dev packages, so a host can
/// type-check another platform's `cfg` branches with `cargo check --lib`. Real
/// builds never set it and stay strict.
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-env-changed=AIDGET_CHECK_TARGET");
    if std::env::var_os("AIDGET_CHECK_TARGET").is_some() {
        return;
    }
    println!("cargo:rerun-if-changed=ui/src");
    println!("cargo:rerun-if-changed=widgets");
    println!("cargo:rerun-if-changed=ui/assets");
    println!("cargo:rerun-if-changed=ui/widget.html");
    println!("cargo:rerun-if-changed=ui/main.html");
    println!("cargo:rerun-if-changed=ui/vite.config.ts");

    // The UI is embedded via include_dir, so every rebuild must refresh
    // ui/dist. `.taurignore` keeps those writes from retriggering the watcher.
    // On Windows `pnpm` is a `.cmd` batch shim, which `CreateProcess` will not
    // resolve from a bare program name.
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "pnpm"]);
        command
    };
    #[cfg(not(windows))]
    let mut command = Command::new("pnpm");
    let status = command
        .args(["run", "build:ui"])
        .current_dir(&manifest)
        .status()
        .expect("failed to run `pnpm run build:ui` for the UI");
    assert!(status.success(), "pnpm run build:ui failed");
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(command_names::ALL_COMMANDS)),
    )
    .expect("cannot build application command permissions")
}
