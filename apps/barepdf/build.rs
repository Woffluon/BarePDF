use std::path::Path;

fn create_valid_ico(png_bytes: &[u8]) -> Vec<u8> {
    let mut ico = Vec::with_capacity(22 + png_bytes.len());
    // ICO Header: reserved = 0, type = 1 (icon), image_count = 1
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    // ICONDIRENTRY for 256x256 32-bit PNG icon
    ico.push(0); // 0 means 256px width
    ico.push(0); // 0 means 256px height
    ico.push(0); // color count
    ico.push(0); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // color planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    ico.extend_from_slice(&(png_bytes.len() as u32).to_le_bytes()); // byte size
    ico.extend_from_slice(&22u32.to_le_bytes()); // image data offset
    ico.extend_from_slice(png_bytes);
    ico
}

fn main() {
    #[cfg(target_os = "windows")]
    {
        let ico_path = Path::new("../../assets/app.ico");
        let png_path = Path::new("../../assets/icon-dark.png");

        if png_path.exists() {
            if let Ok(png_bytes) = std::fs::read(png_path) {
                let ico_bytes = create_valid_ico(&png_bytes);
                let _ = std::fs::write(ico_path, ico_bytes);
            }
        }

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
