# TurboGet User Guide

## 1. Memulai Cepat (Quick Start)
- Klik **Tambah URL** di toolbar atas atau gunakan **Batch / Site Grabber**.
- Klik **Cek URL** untuk memeriksa ukuran file, nama otomatis, dan dukungan resume HTTP `Range`.
- Klik **Mulai Sekarang** untuk mengunduh dengan mesin multi-segmen (hingga 32 koneksi paralel).

## 2. Antrean, Penjadwal & Pembatas Kecepatan
- Unduhan otomatis dikelompokkan ke kategori (`Compressed`, `Documents`, `Music`, `Programs`, `Video`, `Images`, `Lainnya`).
- Gunakan `set_speed_limit` untuk membatasi bandwidth global atau per unduhan.
- Jadwal otomatis mendukung aktivasi harian, waktu mulai/henti, dan aksi pasca-selesai (`none`, `exit`, `shutdown`, `sleep`, `hibernate`).

## 3. Integrasi Browser (Chrome / Edge / Brave)
1. Compile `turboget-host`:
   ```powershell
   cargo build --release --manifest-path native-host/Cargo.toml
   ```
2. Muat folder `extension/` melalui **Extensions -> Developer mode -> Load unpacked**.
3. Daftarkan Native Messaging Host menggunakan Extension ID:
   ```powershell
   ./native-host/register-host.ps1 -ExtensionId "<CHROME_EXTENSION_ID>"
   ```

## 4. Video Grabber (HLS / DASH / yt-dlp)
- Extension `webRequest` mendeteksi stream `.m3u8`, `.mpd`, video, dan audio di tab aktif lalu menampilkannya di popup ekstensi.
- Sisi backend menggunakan parser native HLS/DASH paralel + `ffmpeg` untuk menggabungkan audio & video ke `.mp4`, serta fallback `yt-dlp` untuk halaman web umum.

## 5. CLI & Integrasi AI Agent (MCP Server)
### Perintah CLI `turboget`
```bash
turboget add https://example.com/file.zip --dir D:\Downloads --segments 8 --json
turboget list --status downloading --json
turboget status <id> --json
turboget pause <id>
turboget resume <id>
turboget cancel <id> --delete-file
turboget limit 500
turboget probe https://example.com/file.zip --json
```
Exit codes: `0` (sukses), `2` (argumen salah), `3` (TurboGet tidak berjalan), `4` (ditolak kebijakan izin).

### Konfigurasi MCP Server (Claude Desktop / Claude Code)
Tambahkan ke `claude_desktop_config.json`:
```json
{
  "mcpServers": {
    "turboget": {
      "command": "C:\\Program Files\\turboget\\turboget.exe",
      "args": ["mcp"]
    }
  }
}
```
Pastikan di **Pengaturan → Akses Agent** Anda mengatur mode ke `Otomatis untuk folder diizinkan` (`allowlist`) dan menambahkan folder target yang diizinkan.
