extern crate winres;

fn main() {
    // Tell Cargo to rerun build script if icon changes
    println!("cargo:rerun-if-changed=src/icon.ico");
    // `core::release` reads these with option_env! to decide which forge a
    // copy updates from. rustc records env! reads in its dep-info, but a
    // runner that shares one target directory between a Forgejo-flavoured and
    // a GitHub-flavoured build is exactly where "usually" is not enough.
    println!("cargo:rerun-if-env-changed=EDEN_REPO_URL");
    println!("cargo:rerun-if-env-changed=EDEN_RELEASES_API");

    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();

        res.set_icon("src/icon.ico");

        res.set("FileDescription", "EdenExplorer");
        res.set("ProductName", "EdenExplorer");
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set("OriginalFilename", "EdenExplorer.exe");

        res.set_manifest(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>"#,
        );

        res.compile().expect("Failed to compile Windows resources");
    }
}
