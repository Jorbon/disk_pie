extern crate winapi;

use std::os::windows::ffi::OsStrExt;
use std::io::Error;
use winapi::um::fileapi::GetCompressedFileSizeW;

fn get_disk_size<P: AsRef<std::path::Path>>(path: P) -> Result<u64, Error> {
    let mut long_path: Vec<u16> = r"\\?\".encode_utf16().collect();
    long_path.extend(path.as_ref().as_os_str().encode_wide());
    long_path.push(0);
    let mut high: u32 = 0;
    let low = unsafe { GetCompressedFileSizeW(long_path.as_ptr(), &mut high) };
    if low == u32::MAX {
        Err(Error::last_os_error())
    } else {
        Ok(((high as u64) << 32) | low as u64)
    }
}


enum DirEntry {
    File(String, u64),
    Dir(Vec<DirEntry>, String, u64)
}


use std::os::windows::fs::MetadataExt;

fn scan_dir(path: &std::path::PathBuf) -> (Vec<DirEntry>, u64) {
    match std::fs::read_dir(path) {
        Ok(dir) => {
            let mut size = 0;
            
            let dir_entries = dir.map(|entry| {
                let entry = entry.unwrap();
                if entry.metadata().unwrap().is_dir() {
                    let (subdir_entries, subdir_size) = scan_dir(&entry.path());
                    size += subdir_size;
                    DirEntry::Dir(subdir_entries, entry.file_name().into_string().unwrap(), subdir_size)
                } else {
                    let file_size = get_disk_size(entry.path()).unwrap_or_else(|_| entry.metadata().unwrap().file_size());
                    size += file_size;
                    DirEntry::File(entry.file_name().into_string().unwrap(), file_size)
                }
            }).collect();
            
            (dir_entries, size)
        }
        Err(e) => {
            println!("{e} : {}", path.display());
            (vec![], 0)
        }
    }
}

fn main() {
    let drive = scan_dir(&std::path::PathBuf::from("C:\\"));
    println!("{}", drive.1);
}
