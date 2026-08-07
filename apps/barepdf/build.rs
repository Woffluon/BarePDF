use std::path::Path;

fn main() {
    #[cfg(target_os = "windows")]
    {
        let ico_path = Path::new("../../assets/app.ico");

        let mut res = winres::WindowsResource::new();
        if ico_path.exists() {
            res.set_icon("../../assets/app.ico");
        }
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
        let _ = res.compile();
    }
}
