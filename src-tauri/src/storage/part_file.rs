use std::fs::{File, OpenOptions};
use std::io::Result;
use std::path::Path;

#[cfg(windows)]
use std::os::windows::fs::FileExt;
#[cfg(unix)]
use std::os::unix::fs::FileExt;

/// Utility to create and pre-allocate `.part` files without truncating existing data
pub fn create_part_file<P: AsRef<Path>>(path: P, total_bytes: u64) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;

    if total_bytes > 0 && file.metadata()?.len() != total_bytes {
        file.set_len(total_bytes)?;
    }
    Ok(file)
}

/// Thread-safe concurrent write at an absolute offset without shared cursor locking
pub fn write_all_at(file: &File, mut offset: u64, mut data: &[u8]) -> Result<()> {
    #[cfg(windows)]
    {
        while !data.is_empty() {
            let written = file.seek_write(data, offset)?;
            if written == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            data = &data[written..];
            offset += written as u64;
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        file.write_all_at(data, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::NamedTempFile;

    #[test]
    fn test_create_and_preallocate() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();
        let size = 1024 * 1024; // 1 MB
        let f = create_part_file(path, size).unwrap();
        let meta = f.metadata().unwrap();
        assert_eq!(meta.len(), size);
    }

    #[test]
    fn test_write_all_at_offsets() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();
        let size = 1024;
        let f = create_part_file(path, size).unwrap();

        write_all_at(&f, 100, b"HELLO").unwrap();
        write_all_at(&f, 200, b"WORLD").unwrap();

        let mut read_back = File::open(path).unwrap();
        let mut buf = vec![0u8; size as usize];
        read_back.read_exact(&mut buf).unwrap();

        assert_eq!(&buf[100..105], b"HELLO");
        assert_eq!(&buf[200..205], b"WORLD");
    }
}
