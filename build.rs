fn main() {
    println!("cargo:rerun-if-changed=assets/SecondScreen.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/SecondScreen.ico")
            .set("ProductName", "SecondScreen")
            .set("FileDescription", "SecondScreen")
            .compile()
            .expect("Compile application icon resources");
    }
}
