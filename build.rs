fn main() {
    println!("cargo:rerun-if-changed=ui/app.slint");
    println!("cargo:rerun-if-changed=assets/ferrostb.svg");

    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);

    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Failed to compile ui/app.slint");
}
