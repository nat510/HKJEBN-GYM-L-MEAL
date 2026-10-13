use gyml::storage::*;
use std::{
    fs,
    time::{Duration, SystemTime},
};

#[test]
fn cleanup_removes_only_old_fetched_cache_after_operation_and_keeps_preferences() {
    let dir = std::env::temp_dir().join(format!("gyml-cleanup-{}", std::process::id()));
    fs::create_dir_all(dir.join("images")).unwrap();
    fs::write(dir.join("catalog.json"), r#"{"fetched_at":1000}"#).unwrap();
    fs::write(dir.join("coupons.json"), r#"{"fetched_at":3000000}"#).unwrap();
    fs::write(dir.join("preferences.json"), "{}").unwrap();
    fs::write(dir.join("personal.txt"), "keep").unwrap();
    let old = dir.join("images/old.img");
    fs::write(&old, b"image").unwrap();
    fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1000))
        .unwrap();
    fs::write(dir.join("images/new.img"), b"image").unwrap();
    let report = cleanup_fetched(&dir, 3000000);
    assert_eq!(report.removed, 2);
    assert!(!dir.join("catalog.json").exists());
    assert!(!old.exists());
    assert!(dir.join("coupons.json").exists());
    assert!(dir.join("preferences.json").exists());
    assert!(dir.join("personal.txt").exists());
    assert!(dir.join("images/new.img").exists());
    fs::remove_dir_all(dir).unwrap();
}
