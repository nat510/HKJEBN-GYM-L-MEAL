use crate::model::*;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

pub fn data_dir_for(exe: &Path) -> Result<PathBuf, String> {
    Ok(exe.parent().ok_or("無法確定程式資料夾")?.join("gymL_data"))
}
pub fn data_dir() -> Result<PathBuf, String> {
    data_dir_for(&std::env::current_exe().map_err(|_| "無法確定程式位置")?)
}
fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(Some(0)).collect()
}
pub fn read_json<T: DeserializeOwned>(dir: &Path, name: &str) -> Result<T, String> {
    let file = File::open(dir.join(name)).map_err(|_| "尚未有已存資料")?;
    let mut bytes = vec![];
    file.take(8_000_001)
        .read_to_end(&mut bytes)
        .map_err(|_| "讀取快取失敗")?;
    if bytes.len() > 8_000_000 {
        return Err("快取超過大小上限".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "快取格式損壞；將重新擷取".into())
}
pub fn write_json<T: Serialize>(dir: &Path, name: &str, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "資料編碼失敗")?;
    atomic_write(dir, name, &bytes)
}
pub fn atomic_write(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
    fs::create_dir_all(dir).map_err(|_| "無法建立 gymL_data；請將程式移至可寫入的資料夾")?;
    let temporary = dir.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "無法寫入 gymL_data")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "儲存資料失敗；保留原有快取")?;
        drop(file);
        let from = wide(temporary.as_os_str());
        let to = wide(dir.join(name).as_os_str());
        // Same-directory replacement keeps readers from seeing partial JSON.
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err("快取替換失敗；保留原有資料".into());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
pub fn load_catalog(dir: &Path) -> Result<Catalog, String> {
    let c: Catalog = read_json(dir, "catalog.json")?;
    c.validate()?;
    Ok(c)
}
pub fn save_catalog(dir: &Path, catalog: &Catalog) -> Result<(), String> {
    catalog.validate()?;
    write_json(dir, "catalog.json", catalog)
}
pub fn load_gallery(dir: &Path, kind: GalleryKind) -> Result<Gallery, String> {
    let g: Gallery = read_json(dir, kind.file())?;
    g.validate()?;
    Ok(g)
}
pub fn save_gallery(dir: &Path, kind: GalleryKind, gallery: &Gallery) -> Result<(), String> {
    gallery.validate()?;
    write_json(dir, kind.file(), gallery)
}
pub fn load_preferences(dir: &Path) -> Preferences {
    let mut p: Preferences = read_json(dir, "preferences.json").unwrap_or_default();
    p.category = p.category.min(7);
    p.search = p.search.chars().take(200).collect();
    p.gallery_keywords = p
        .gallery_keywords
        .into_iter()
        .take(100)
        .map(|s| s.chars().take(100).collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    p
}
pub fn image_key(url: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for b in url.bytes() {
        hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}.img")
}
pub fn cached_image(dir: &Path, url: &str) -> Option<Vec<u8>> {
    let path = dir.join("images").join(image_key(url));
    let meta = fs::metadata(&path).ok()?;
    if meta.len() > 16_000_000 {
        return None;
    }
    // Cached images are refreshed after seven days, on selection only.
    if meta.modified().ok()?.elapsed().ok()?.as_secs() > 7 * 86400 {
        return None;
    }
    fs::read(path).ok()
}
pub fn save_image(dir: &Path, url: &str, bytes: &[u8]) -> Result<(), String> {
    let images = dir.join("images");
    fs::create_dir_all(&images).map_err(|_| "圖片快取無法寫入")?;
    let mut files: Vec<_> = fs::read_dir(&images)
        .map_err(|_| "圖片快取無法讀取")?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            (m.is_file() && e.path().extension().is_some_and(|x| x == "img"))
                .then(|| (e.path(), m.len(), m.modified().ok()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.2);
    for (path, size, _) in files {
        if total + bytes.len() as u64 <= 32_000_000 {
            break;
        }
        if fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    atomic_write(&images, &image_key(url), bytes)
}

pub const CACHE_MAX_AGE: u64 = 30 * 24 * 60 * 60;
#[derive(Default, Debug)]
pub struct CleanupReport {
    pub removed: usize,
    pub errors: usize,
}
pub fn cleanup_fetched(dir: &Path, at: u64) -> CleanupReport {
    let mut report = CleanupReport::default();
    // Never traverse a linked cache directory or touch user-created files.
    if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return report;
    }
    for name in ["catalog.json", "coupons.json", "promotions.json"] {
        let path = dir.join(name);
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() || meta.file_type().is_symlink() {
            continue;
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let captured = read_json::<serde_json::Value>(dir, name)
            .ok()
            .and_then(|v| v.get("fetched_at").and_then(|v| v.as_u64()))
            .or(modified);
        if captured.is_some_and(|t| at.saturating_sub(t) >= CACHE_MAX_AGE) {
            match fs::remove_file(path) {
                Ok(()) => report.removed += 1,
                Err(_) => report.errors += 1,
            }
        }
    }
    let images = dir.join("images");
    if fs::symlink_metadata(&images).is_ok_and(|m| m.file_type().is_symlink()) {
        return report;
    }
    if let Ok(entries) = fs::read_dir(images) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_file()
                || meta.file_type().is_symlink()
                || path.extension().is_none_or(|e| e != "img")
            {
                continue;
            }
            let age = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| at.saturating_sub(d.as_secs()));
            if age.is_some_and(|d| d >= CACHE_MAX_AGE) {
                match fs::remove_file(path) {
                    Ok(()) => report.removed += 1,
                    Err(_) => report.errors += 1,
                }
            }
        }
    }
    report
}
