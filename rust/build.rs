fn main() {
    if cfg!(windows) {
        // Slint's compiler needs more stack than the default Windows main thread
        // as the editor's component tree grows.
        std::thread::Builder::new()
            .name("compile-editor-ui".to_owned())
            .stack_size(8 * 1024 * 1024)
            .spawn(compile_editor)
            .expect("start Slint compiler")
            .join()
            .expect("Slint compiler panicked");
    } else {
        compile_editor();
    }
}

fn compile_editor() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/editor.slint", config).expect("compile editor UI");
}
