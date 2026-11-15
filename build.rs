// SPDX-License-Identifier: GPL-3.0-only

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    let output = Command::new("pkg-config")
        .args(["--libs", "libcfgd"])
        .output()
        .expect("failed to run pkg-config");
    if !output.status.success() {
        panic!(
            "pkg-config could not find libcfgd, set PKG_CONFIG_PATH: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for flag in String::from_utf8_lossy(&output.stdout).split_whitespace() {
        if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        } else if let Some(name) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={name}");
        }
    }
}
