//! Hands the linker the image's memory layout on a bare-metal target: the layout of the QEMU
//! machine that runs the image, from `boards/` (`netduinoplus2.x` for `thumbv7em-none-eabihf`,
//! `virt.x` for `riscv32imac-unknown-none-elf`), copied into `OUT_DIR` as the `memory.x` the
//! runtime's `link.x` includes, and `-Tlink.x` for the bin. Adapted from rusty-bacnet's
//! `firmware/link/build.rs` (same owner).
//!
//! A host build gets nothing: its bin is the host reference runner and links like any other.

use std::env;
use std::fs;
use std::path::PathBuf;

/// The bare-metal targets the image builds for, and the layout in `boards/` each one links with.
const LAYOUTS: [(&str, &str); 2] = [
    ("thumbv7em-none-eabihf", "boards/netduinoplus2.x"),
    ("riscv32imac-unknown-none-elf", "boards/virt.x"),
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=boards");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("none") {
        return;
    }
    let target = env::var("TARGET").expect("cargo sets TARGET");
    let Some((_, layout)) = LAYOUTS.iter().find(|(triple, _)| *triple == target) else {
        panic!(
            "oce-firmware-link has no memory layout for {target}; it builds for {}",
            LAYOUTS.map(|(triple, _)| triple).join(" and ")
        );
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::copy(layout, out.join("memory.x"))
        .unwrap_or_else(|error| panic!("copy {layout} into OUT_DIR: {error}"));
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
}
