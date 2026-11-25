fn main() {
    slint_build::compile("ui/app.slint").expect("Failed to compile ui/app.slint");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/ferrostb.ico");
        res.compile().expect("Failed to embed Windows icon resource");
    }
}
