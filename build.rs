// SPDX-License-Identifier: GPL-3.0-only

fn main() {
    println!("cargo:rerun-if-changed=c");

    cc::Build::new()
        .include("c")
        .file("c/frame.c")
        .file("c/geometry.c")
        .file("c/keys.c")
        .file("c/pixels.c")
        .file("c/shape.c")
        .file("c/xcb.c")
        .std("c11")
        .warnings(true)
        .flag("-Wpedantic")
        .flag_if_supported("-Werror")
        .compile("slate_x11");

    println!("cargo:rustc-link-lib=xcb");
    println!("cargo:rustc-link-lib=xcb-shape");
}
