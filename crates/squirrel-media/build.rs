//! With the `vips` feature, tells the linker where the system libvips and
//! GLib live (the `libvips` crate only names them). Homebrew keeps them
//! outside the default search path; on Debian `pkg-config` answers `/usr/lib/…`,
//! which is harmless.

use std::process::Command;

const LIBRARIES: [&str; 3] = ["vips", "glib-2.0", "gobject-2.0"];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    if std::env::var_os("CARGO_FEATURE_VIPS").is_none() {
        return;
    }
    for library in LIBRARIES {
        // `--variable` reads the .pc file alone: `--libs` fails on a keg-only
        // dependency (Homebrew's libarchive) that linking never needs.
        let output = Command::new("pkg-config")
            .args(["--variable=libdir", library])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let dir = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                if !dir.is_empty() {
                    println!("cargo:rustc-link-search=native={dir}");
                }
            }
            _ => println!(
                "cargo:warning=pkg-config does not know {library}: relying on the default library path"
            ),
        }
    }
}
