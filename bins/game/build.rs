//! Gives the Windows executable its icon.

fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icon.ico");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=the icon could not be embedded: {error}");
        }
    }
}
