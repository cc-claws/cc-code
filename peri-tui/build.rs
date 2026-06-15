fn main() {
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let icon_path = std::path::PathBuf::from(&manifest_dir)
            .join("assets")
            .join("icon.ico");

        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon_path.to_str().unwrap());
        res.set("FileVersion", "0.2.0.0");
        res.set("ProductVersion", "0.2.0.0");
        res.set("ProductName", "Peri");
        res.set("FileDescription", "Peri Code");
        res.set("CompanyName", "Peri");
        res.set("LegalCopyright", "Copyright (c) 2026 Peri");
        res.set_manifest(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>"#,
        );
        res.compile().expect("failed to embed windows resource");
    }
}
