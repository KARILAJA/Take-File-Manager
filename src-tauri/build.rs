fn main() {
    #[cfg(windows)]
    {
        if std::env::var("RC").is_err() {
            let possible_paths = [
                r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\rc.exe",
                r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\rc.exe",
                r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\rc.exe",
            ];
            for p in &possible_paths {
                if std::path::Path::new(p).exists() {
                    std::env::set_var("RC", p);
                    break;
                }
            }
        }
    }
    tauri_build::build()
}
