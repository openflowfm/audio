use std::process::Command;
fn main() {
    for key in ["PROFILE", "OPT_LEVEL", "TARGET", "CARGO_ENCODED_RUSTFLAGS"] {
        println!(
            "cargo:rustc-env=HARNESS_{key}={}",
            std::env::var(key).unwrap_or_default()
        );
    }
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    println!(
        "cargo:rustc-env=HARNESS_RUSTC={}",
        version.unwrap_or_else(|| "unavailable".into())
    );
    println!("cargo:rerun-if-changed=build.rs");
}
