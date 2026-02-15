# Building PDF Manager on Windows

Cross-compiling for Windows from Linux is not feasible due to native poppler/cairo dependencies. Build natively on a Windows machine instead.

## Prerequisites

1. **Install Rust** via [rustup](https://rustup.rs/) (select the MSVC toolchain)

2. **Install Visual Studio Build Tools** (or full Visual Studio) with the "Desktop development with C++" workload

3. **Install vcpkg** and the required native libraries:

   ```powershell
   git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
   cd C:\vcpkg
   .\bootstrap-vcpkg.bat
   .\vcpkg install poppler:x64-windows cairo:x64-windows glib:x64-windows
   .\vcpkg integrate install
   ```

4. **Set environment variables** so pkg-config can find the libraries:

   ```powershell
   $env:PKG_CONFIG_PATH = "C:\vcpkg\installed\x64-windows\lib\pkgconfig"
   $env:PATH += ";C:\vcpkg\installed\x64-windows\bin"
   ```

## Build

```powershell
cargo build --release
```

The resulting `.exe` will be at `target\release\pdf-manager.exe`.

- The console window is suppressed automatically (`#![windows_subsystem = "windows"]`)
- The app icon and metadata are embedded via `winresource` in `build.rs`

## Runtime dependencies

The `.exe` will need the following DLLs alongside it (or on the system PATH):

- `poppler-glib-8.dll` (and its dependencies)
- `cairo.dll`
- `glib-2.0-0.dll`, `gobject-2.0-0.dll`

Copy them from `C:\vcpkg\installed\x64-windows\bin\` next to the `.exe` for a portable distribution.
