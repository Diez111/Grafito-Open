use std::env;
use std::fs;
use std::io;
use std::path::Path;

const WINDOWS_MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" processorArchitecture="*" name="Grafito.Grafito" type="win32" />
  <description>Grafito</description>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}" />
      <supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}" />
      <supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}" />
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}" />
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2,PerMonitor</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() -> io::Result<()> {
    let icon_source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/grafito-icon-256x256.png");
    println!("cargo:rerun-if-changed={}", icon_source.display());

    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return Ok(());
    }

    let icon_path =
        Path::new(&env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo")).join("grafito.ico");
    write_png_icon(&icon_source, &icon_path)?;

    let icon_path = icon_path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 icon path"))?;
    let package_version = env::var("CARGO_PKG_VERSION").expect("Cargo package version is set");
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(icon_path)
        .set_manifest(WINDOWS_MANIFEST)
        .set("ProductName", "Grafito")
        .set(
            "FileDescription",
            "Grafito - interactive mathematical graphing",
        )
        .set("InternalName", "grafito")
        .set("OriginalFilename", "grafito.exe")
        .set("CompanyName", "Grafito Contributors")
        .set(
            "LegalCopyright",
            "Copyright (c) 2024-2026 Grafito Contributors",
        )
        .set("FileVersion", &package_version)
        .set("ProductVersion", &package_version);

    if !env::var("CARGO_PKG_VERSION_PRE")
        .unwrap_or_default()
        .is_empty()
    {
        resource.set_version_info(
            winresource::VersionInfo::FILEFLAGS,
            winresource::VersionInfo::VS_FF_PRERELEASE,
        );
    }

    resource.compile()
}

fn write_png_icon(source: &Path, destination: &Path) -> io::Result<()> {
    let png = fs::read(source)?;
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Windows icon source is not a PNG",
        ));
    }

    let width = u32::from_be_bytes(png[16..20].try_into().expect("validated PNG header"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("validated PNG header"));
    if width != 256 || height != 256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Windows icon source must be 256x256",
        ));
    }

    let png_len = u32::try_from(png.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Windows icon source is too large",
        )
    })?;
    let mut ico = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&0_u16.to_le_bytes());
    ico.extend_from_slice(&1_u16.to_le_bytes());
    ico.extend_from_slice(&1_u16.to_le_bytes());
    ico.extend_from_slice(&[0, 0, 0, 0]);
    ico.extend_from_slice(&1_u16.to_le_bytes());
    ico.extend_from_slice(&32_u16.to_le_bytes());
    ico.extend_from_slice(&png_len.to_le_bytes());
    ico.extend_from_slice(&22_u32.to_le_bytes());
    ico.extend_from_slice(&png);
    fs::write(destination, ico)
}
