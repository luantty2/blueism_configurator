#[cfg(windows)]
fn main() {
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/icons/app-icon.ico");
    resource.compile().expect("failed to compile Windows resources");
}

#[cfg(not(windows))]
fn main() {}
