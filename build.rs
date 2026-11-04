//! Puts the icon and the version into the Windows program, where Explorer and the uninstall list
//! read them. The icon is `packaging/windows/pearl.ico`, drawn by `cargo run --example icon`.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=packaging/windows/pearl.ico");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("packaging/windows/pearl.ico").set("ProductName", "Pearl").set("FileDescription", "Pearl");
        // A program without its icon still runs: `tests/windows_program.rs` is what catches it.
        if let Err(error) = resource.compile() {
            println!("cargo:warning=no icon or version in the program: {error}");
        }
    }
}
