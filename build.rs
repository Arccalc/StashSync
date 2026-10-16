//! Bake window/taskbar icons from assets/icon.png and embed Win32 .ico.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let source = image::open("assets/icon.png")
        .expect("assets/icon.png missing — run icon generator first")
        .into_rgba8();

    bake_rgba(&source, &out_dir, 256);
    bake_rgba(&source, &out_dir, 32);

    #[cfg(windows)]
    {
        // Steam sign-in only works when this process and Steam share the same
        // user and integrity. Elevation made the game die and blocked Steam IPC.
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "StashSync");
        res.set("FileDescription", "StashSync inventory sync");
        res.set_manifest(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity
      version="0.2.5.0"
      processorArchitecture="*"
      name="StashSync"
      type="win32"
  />
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
      <supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"/>
      <supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"/>
      <supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"/>
    </application>
  </compatibility>
</assembly>
"#,
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=winres embed skipped: {e}");
        }
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.png");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}

fn bake_rgba(source: &image::RgbaImage, out_dir: &Path, size: u32) {
    let resized = image::imageops::resize(
        source,
        size,
        size,
        image::imageops::FilterType::Lanczos3,
    );
    let path = out_dir.join(format!("icon_{size}.rgba"));
    fs::write(&path, resized.into_raw())
        .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}
