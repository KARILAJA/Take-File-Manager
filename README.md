# Take File Manager (TFM)

**Take File Manager** adalah aplikasi desktop download manager modern berbasis **Tauri 2 (Rust)** + **React / TypeScript (Tailwind CSS & Zustand)** untuk Windows x64. Mendukung akselerasi unduhan HTTP/HTTPS multi-segmen, FTP/FTPS, BitTorrent (Magnet link & file `.torrent`), Video & Streaming Grabber ala IDM (YouTube, TikTok, Instagram, HLS `.m3u8`, DASH `.mpd`), penangkap unduhan browser, penjadwal antrean, serta CLI & MCP Server untuk AI Agent.

---

## Fitur Utama

- **Akselerasi Multi-Segmen:** Unduhan paralel hingga 32 segmen dengan dukungan `Range` resume, crash/pause recovery via SQLite checkpoint, dan pembatas kecepatan global maupun per tugas.
- **Dukungan Protokol Lengkap:** HTTP/1.1, HTTP/2, FTP/FTPS, BitTorrent (`magnet:` link & file `.torrent` lokal/HTTP via `librqbit`).
- **Video Grabber Ala IDM:**
  - Tombol melayang **"Unduh Video (TFM)"** langsung di atas pemutar video halaman web + popup ekstensi browser.
  - Tab **Video Grabber** di dalam aplikasi desktop untuk menganalisis dan mengunduh video dari YouTube, TikTok, Instagram, Facebook, X/Twitter, Vimeo, HLS (`.m3u8`), DASH (`.mpd`), dan file video langsung (`.mp4`/`.webm`/`.mp3`).
  - Menampilkan semua resolusi beserta rincian ukuran (`Video + Audio ≈ Total`), codec (`H.264`, `VP9`, `AV1`), bitrate/FPS, serta auto-download `yt-dlp.exe` saat dibutuhkan dan merge otomatis menggunakan `ffmpeg`.
- **Integrasi Ekstensi Browser & AutoStart Background:**
  - Menu **Integrasi Browser** 1-klik untuk **Google Chrome**, **Microsoft Edge**, dan **Opera Browser** (plus registrasi otomatis Native Messaging Host; **Mozilla Firefox** bertanda *Soon on Store Addon*).
  - **AutoStart Windows (`--minimized`)**: Aplikasi otomatis berjalan di latar belakang (*System Tray*) saat komputer menyala dan menampilkan notifikasi Toast Windows native ketika unduhan selesai/gagal.
- **Batch, Pola URL, Site Grabber & Scheduler:** Ekspansi pola angka/alfabet (`[001-100]`), crawler situs web berfilter ekstensi, impor/ekspor (`TXT`, `CSV`, `JSON`), penjadwal antrean, aksi daya OS (*Sleep / Hibernate / Shutdown*), serta inspektor **Checksum SHA-256 / SHA-512**.
- **Antarmuka Modern & Dialog Native Windows:** Custom frameless title bar, mode Dark (*Nocturnal Cobalt*) & Light (*Clean Daylight*), serta dialog pemilih folder/file Windows Explorer modern (`IFileOpenDialog`).
- **CLI & MCP Server:** Otomatisasi lewat baris perintah `turboget` atau integrasi AI Agent (`turboget mcp`) dengan kontrol akses *Deny / Ask / Allowlist* dan proteksi SSRF.

---

## Prasyarat Sistem

### 1. Untuk Pengguna Akhir (Menggunakan Installer)
- **OS:** Windows 10 / Windows 11 (64-bit) dengan Microsoft Edge WebView2 Runtime (umumnya sudah terpasang di Windows 10/11).
- **`ffmpeg` (Opsional / Disarankan untuk Video Grabber):** Dibutuhkan untuk menggabungkan stream video+audio (YouTube/HLS/DASH). Dapat dipasang lewat WinGet:
  ```powershell
  winget install Gyan.FFmpeg
  ```
- **`yt-dlp`:** Tidak perlu dipasang manual; aplikasi akan otomatis mengunduh `yt-dlp.exe` ke `%APPDATA%\turboget\bin\` saat pertama kali menganalisis video dari situs seperti YouTube/TikTok (atau dapat diletakkan sendiri di folder tersebut / `PATH`).

### 2. Untuk Developer (Build dari Source Code)
- **Node.js** `v20+` (disarankan LTS/terbaru; juga digunakan oleh `yt-dlp` sebagai JS runtime jika terpasang)
- **Rust Toolchain** (`rustup` / `cargo` stable MSVC `x86_64-pc-windows-msvc`)
- **Build Tools Visual Studio (Desktop development with C++)** & Windows SDK
- **Tauri 2 Prerequisites** (WebView2, WiX / NSIS diunduh otomatis oleh Tauri CLI saat build)
- **Python 3** (opsional, hanya jika ingin mengemas ulang `.xpi` Firefox dengan `scripts/build-firefox-xpi.py`)

---

## Cara Instalasi dari Source Code (Build & Run)

### 1. Clone Repository & Pasang Dependensi Frontend
```powershell
git clone <URL_REPOSITORY_ANDA>
cd <NAMA_FOLDER_REPO>
npm install
```

### 2. Build Companion Binaries (`native-host` & `cli`)
`turboget-host.exe` diperlukan agar ekstensi browser dapat berkomunikasi dengan aplikasi desktop melalui Chrome/Edge/Firefox Native Messaging:
```powershell
cargo build --release --manifest-path native-host/Cargo.toml
cargo build --release --manifest-path cli/Cargo.toml
```

Salin `turboget-host.exe` ke direktori target rilis aplikasi desktop (jika membangun bundle):
```powershell
New-Item -ItemType Directory -Force -Path src-tauri\target\release | Out-Null
Copy-Item -Force native-host\target\release\turboget-host.exe src-tauri\target\release\turboget-host.exe
```

### 3. Menjalankan Mode Development
```powershell
npm run tauri dev
```

### 4. Membangun Aplikasi & Installer Produksi (`.exe` / `.msi`)
```powershell
npm run tauri build
```
Hasil build akan tersedia di:
- Binary utama: `src-tauri\target\release\turboget.exe`
- Installer NSIS: `src-tauri\target\release\bundle\nsis\Take File Manager_0.1.0_x64-setup.exe`
- Installer MSI: `src-tauri\target\release\bundle\msi\Take File Manager_0.1.0_x64_en-US.msi`

*(Catatan: Jika menemui kendala memori saat kompilasi paralel di Windows, jalankan dengan `$env:CARGO_BUILD_JOBS="2"` sebelum `npm run tauri build`.)*

---

## Cara Menggunakan Aplikasi & Integrasi Browser

### 1. Menjalankan Aplikasi & AutoStart Latar Belakang
1. Jalankan `turboget.exe` (atau buka **Take File Manager** dari Start Menu setelah menginstal).
2. Secara default, **AutoStart Saat Komputer Menyala (Latar Belakang)** aktif dan mendaftarkan:
   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Take File Manager` → `"<path>\turboget.exe" --minimized`.
3. Ketika komputer dinyalakan ulang, aplikasi otomatis berjalan diam di **System Tray** (pojok kanan bawah taskbar) dan siap menangkap unduhan browser serta mengirim notifikasi desktop native saat unduhan selesai.
4. Anda dapat mengaktifkan/menonaktifkan AutoStart kapan saja melalui **Pengaturan (⚙) → Tab Jaringan & Startup**.

### 2. Memasang Ekstensi Browser (Chrome / Edge / Opera)
> **Peringatan Penting:** Untuk browser berbasis Chromium (**Google Chrome**, **Microsoft Edge**, **Opera**), Anda wajib mengaktifkan **Developer Mode (Mode Pengembang)** pada halaman Ekstensi browser agar ekstensi lokal (`extension/`) dapat dimuat.

#### Cara Cepat dari Dalam Aplikasi:
1. Di toolbar atas Take File Manager, klik tombol **Ekstensi** (ikon puzzle) atau masuk ke **Pengaturan (⚙) → Integrasi Browser**.
2. Pilih browser Anda:
   - **Microsoft Edge / Opera:** Klik **Pasang Otomatis (1-Klik)** (jika browser sedang tidak berjalan, aplikasi meluncurkan browser dengan `--load-extension` sehingga ekstensi langsung dimuat otomatis; jika browser sedang terbuka, klik **Tutup Browser & Pasang Otomatis** atau ikuti panduan 3 langkah).
   - **Google Chrome:** Klik **Integrasikan Sekarang**. Aplikasi akan mendaftarkan Native Messaging Host, menyalin path folder `extension/` ke clipboard, membuka halaman panduan di Chrome, serta membuka File Explorer yang menyorot folder `extension`.
     1. Buka tab baru di Chrome, kunjungi `chrome://extensions`.
     2. Aktifkan sakelar **Developer mode** di pojok kanan atas.
     3. Klik **Load unpacked** (lalu tempel path folder yang sudah disalin, atau cukup **seret & jatuhkan / drag & drop** folder `extension` ke halaman `chrome://extensions`).
3. **Mozilla Firefox:** Saat ini bertanda **Soon on Store Addon** di menu Integrasi Browser. Bagi developer/pengguna lanjutan yang ingin mengemas `.xpi` lokal untuk diunggah/ditandatangani di Mozilla Add-ons (`addons.mozilla.org`) atau dimuat di Firefox Developer Edition/Nightly:
   ```powershell
   python scripts/build-firefox-xpi.py
   ```
   Skrip tersebut menghasilkan folder `extension-firefox/` dan paket `take-file-manager-firefox.xpi`.

*(Jika memindahkan folder repo atau menginstal dari installer secara manual, Anda juga dapat mendaftarkan Native Messaging Host secara terpisah lewat: `./native-host/register-host.ps1`)*

### 3. Mengunduh File, Torrent, dan Video
- **Unduhan Reguler / Torrent:** Klik **Tambah URL**, tempel link `http(s)://`, `ftp://`, `magnet:?xt=...`, atau pilih file `.torrent` lokal → pilih folder simpan → **Mulai Sekarang**.
- **Video Grabber di Browser:** Saat memutar video (YouTube, TikTok, Instagram, HLS/DASH, dll.), klik pill melayang **"Unduh Video (TFM)"** di sudut pemutar video atau klik ikon ekstensi **Take File Manager** di toolbar browser, pilih resolusi yang diinginkan, lalu unduh.
- **Video Grabber di Aplikasi:** Klik **Grabber → Tab Video Grabber**, tempel URL video, klik **Analisis Video** untuk melihat seluruh resolusi dan rincian ukuran, lalu klik **Unduh Video**.

---

## Penggunaan CLI & MCP Server (AI Agent)

Pastikan aplikasi utama **Take File Manager** sudah berjalan di latar belakang sebelum menggunakan CLI atau MCP:

```powershell
# Menambah unduhan
turboget add https://example.com/file.zip --dir D:\Downloads --segments 8 --json

# Melihat daftar dan status unduhan
turboget list --status downloading --json
turboget status <ID_UNDUHAN> --json

# Jeda, lanjut, atau batalkan unduhan
turboget pause <ID_UNDUHAN>
turboget resume <ID_UNDUHAN>
turboget cancel <ID_UNDUHAN> --delete-file

# Mengatur batas kecepatan global (KB/s) & memeriksa URL
turboget limit 1024
turboget probe https://example.com/file.zip --json
```

Untuk menghubungkan ke klien MCP (seperti Claude Desktop), tambahkan ke konfigurasi MCP Anda:
```json
{
  "mcpServers": {
    "take-file-manager": {
      "command": "C:\\Path\\To\\cli\\target\\release\\turboget.exe",
      "args": ["mcp"]
    }
  }
}
```
Atur izin folder tujuan pada **Pengaturan (⚙) → Akses Agent (MCP/CLI)** ke mode **Allowlist** agar agen hanya dapat menyimpan ke folder yang Anda izinkan.

---

## Menjalankan Test & Linter

```powershell
# Test Frontend (Vitest) & TypeScript/Vite build
npm test
npm run build

# Test Rust Core (Unit + Integration Test M1-M11) & Clippy
cargo test --manifest-path src-tauri/Cargo.toml -j 2
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --tests -j 2 -- -D warnings

# Test Native Host
cargo test --manifest-path native-host/Cargo.toml
```

---

## Struktur Proyek

```text
├── src/                  # Frontend React, Zustand store, i18n, komponen UI & Vitest
├── src-tauri/            # Core engine Rust (multi-segmen, FTP, BitTorrent, HLS/DASH/yt-dlp, SQLite, LocalApi, Tray, Windows Dialog/Toast)
├── extension/            # Ekstensi Browser MV3 (Chrome / Edge / Opera)
├── native-host/          # Native Messaging Host (turboget-host) & skrip register-host.ps1
├── cli/                  # Companion CLI & MCP Server
├── scripts/              # Skrip bantu (termasuk build-firefox-xpi.py)
├── docs/                 # Panduan pengguna, keputusan arsitektur & checklist rilis
└── test-server/          # Mock HTTP server untuk integration test
```
