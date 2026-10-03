fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

        let mut res = winres::WindowsResource::new();
        res.set("OriginalFilename", "BarePDF.Thumbnail.dll");
        res.set("InternalName", "barepdf_thumbnail");
        res.compile()?;
    }

    Ok(())
}
