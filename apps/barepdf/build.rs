fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR")
                .ok_or_else(|| std::io::Error::other("CARGO_MANIFEST_DIR missing"))?,
        );
        let ico_path = manifest_dir.join("../../assets/app.ico");
        assert!(
            ico_path.is_file(),
            "Required application icon is missing: {}",
            ico_path.display()
        );
        println!("cargo:rerun-if-changed={}", ico_path.display());

        let mut res = winres::WindowsResource::new();
        let ico_path = ico_path
            .to_str()
            .ok_or_else(|| std::io::Error::other("Icon path is not UTF-8"))?;
        res.set_icon(ico_path);
        res.set_manifest(
            r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 & 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#,
        );
        res.compile()?;
    }

    Ok(())
}
