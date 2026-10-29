fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint_build::compile("ui/app.slint")?;
    println!("cargo:rerun-if-changed=../../icon/icon.ico");
    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("../../icon/icon.ico");
        resource.set("ProductName", "OpenCanvas");
        resource.set("FileDescription", "OpenCanvas");
        resource.compile()?;
    }
    Ok(())
}
