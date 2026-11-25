fn main() {
    println!("cargo:rerun-if-changed=ui/app.slint");
    println!("cargo:rerun-if-changed=assets/ferrostb.svg");
    println!("cargo:rerun-if-changed=assets/ferrostb.ico");

    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);

    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Failed to compile ui/app.slint");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/ferrostb.ico");
        res.compile().expect("Failed to embed Windows icon resource");
    }
}
