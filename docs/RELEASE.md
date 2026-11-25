# TurboGet Build, Code Signing & Release Checklist

## 1. Build Binaries
1. Build frontend production assets:
   ```powershell
   npm install
   npm run build
   ```
2. Build companion binaries (`turboget-host` & `turboget` CLI/MCP):
   ```powershell
   cargo build --release --manifest-path native-host/Cargo.toml
   cargo build --release --manifest-path cli/Cargo.toml
   ```
3. Build Tauri Desktop Bundle (MSI + NSIS):
   ```powershell
   npm run tauri build
   ```
   Output bundles are placed in `src-tauri/target/release/bundle/nsis/` and `src-tauri/target/release/bundle/msi/`.

## 2. Windows Code Signing (Authenticode)
Sign `turboget.exe`, `turboget-host.exe`, `turboget-cli`, and installers before public distribution:
- **EV/OV Code Signing Certificate or Azure Trusted Signing**:
  - Configure `bundle.windows.certificateThumbprint` and `bundle.windows.timestampUrl` (`http://timestamp.digicert.com`) in `src-tauri/tauri.conf.json`, OR run `signtool.exe`:
    ```powershell
    signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /sha1 <THUMBPRINT> `
      src-tauri/target/release/turboget.exe `
      native-host/target/release/turboget-host.exe `
      cli/target/release/turboget.exe
    ```

## 3. Clean VM Verification Checklist (Windows 11)
1. Install NSIS `.exe` or `.msi` on clean Windows 11 VM.
2. Confirm HKCU registry key `Software\Google\Chrome\NativeMessagingHosts\com.turboget.host` points to installed `com.turboget.host.json`.
3. Download a sample `.zip`/`.exe` and confirm `Get-Item <file> -Stream Zone.Identifier` shows `ZoneId=3` (Mark-of-the-Web).
4. Verify Proxy / Network settings, Log Viewer (`INFO` & `AGENT_AUDIT`), and sustained queued download completion with zero memory leaks or leftover `.part` files.
