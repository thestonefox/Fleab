fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../assets/fleab.ico");
        resource.set("ProductName", "Fleab");
        resource.set("FileDescription", "Fleab build deployer");
        resource.set("OriginalFilename", "fleab.exe");
        resource.set("InternalName", "Fleab");
        resource.compile().expect("Windows application resources");
    }
    println!("cargo:rerun-if-changed=../assets/fleab.ico");
}
