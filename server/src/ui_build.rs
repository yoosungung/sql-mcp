//! `semantic-ui/dist` freshness used by `build.rs` before `npm run build`.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

const ROOT_SOURCES: &[&str] = &[
    "package.json",
    "package-lock.json",
    "vite.config.ts",
    "tsconfig.json",
    "index.html",
];

pub fn dist_is_stale(ui_root: &Path) -> bool {
    let dist = ui_root.join("dist/index.html");
    let Ok(meta) = fs::metadata(&dist) else {
        return true;
    };
    let dist_mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    match newest_source_mtime(ui_root) {
        Some(src) => src > dist_mtime,
        None => false,
    }
}

fn newest_source_mtime(ui_root: &Path) -> Option<SystemTime> {
    let mut newest = None;
    for rel in ROOT_SOURCES {
        consider(&ui_root.join(rel), &mut newest);
    }
    walk_dir(&ui_root.join("src"), &mut newest);
    newest
}

fn walk_dir(dir: &Path, newest: &mut Option<SystemTime>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, newest);
        } else if !is_ignored_source(&path) {
            consider(&path, newest);
        }
    }
}

fn consider(path: &Path, newest: &mut Option<SystemTime>) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    let Ok(mtime) = meta.modified() else {
        return;
    };
    match newest {
        Some(cur) if *cur >= mtime => {}
        _ => *newest = Some(mtime),
    }
}

fn is_ignored_source(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.ends_with(".test.ts") || name.ends_with(".test.tsx") || name == "test-setup.ts"
}

#[cfg(test)]
mod tests {
    use super::dist_is_stale;
    use std::fs;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    fn pause() {
        thread::sleep(Duration::from_millis(20));
    }

    #[test]
    fn missing_dist_is_stale() {
        let dir = TempDir::new().unwrap();
        write(&dir.path().join("src/App.tsx"), "export {}\n");
        assert!(dist_is_stale(dir.path()));
    }

    #[test]
    fn dist_newer_than_src_is_fresh() {
        let dir = TempDir::new().unwrap();
        write(&dir.path().join("src/App.tsx"), "export {}\n");
        write(&dir.path().join("package.json"), "{}\n");
        pause();
        write(&dir.path().join("dist/index.html"), "<html></html>\n");
        assert!(!dist_is_stale(dir.path()));
    }

    #[test]
    fn src_newer_than_dist_is_stale() {
        let dir = TempDir::new().unwrap();
        write(&dir.path().join("src/App.tsx"), "export {}\n");
        write(&dir.path().join("dist/index.html"), "<html></html>\n");
        pause();
        write(&dir.path().join("src/App.tsx"), "export const x = 1\n");
        assert!(dist_is_stale(dir.path()));
    }

    #[test]
    fn test_source_newer_does_not_stale() {
        let dir = TempDir::new().unwrap();
        write(&dir.path().join("src/App.tsx"), "export {}\n");
        pause();
        write(&dir.path().join("dist/index.html"), "<html></html>\n");
        pause();
        write(&dir.path().join("src/App.test.tsx"), "test\n");
        write(&dir.path().join("src/test-setup.ts"), "export {}\n");
        assert!(!dist_is_stale(dir.path()));
    }
}
