//! Cloud sync folders and conflict copies (PRD Section 8).
//!
//! A sync program can replace or duplicate `library.sqlite` while it is being written.
//! Detection only looks at paths and file names: nothing is opened, changed, or sent.

use std::path::{Path, PathBuf};

/// Environment variables that OneDrive sets to its sync folders.
const ONEDRIVE_VARIABLES: [&str; 3] = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"];

/// True when the folder is inside a OneDrive, Dropbox, or Google Drive folder.
///
/// This is a best guess from the path: a sync folder with an unusual name that no
/// program announces is not recognised.
pub fn is_cloud_folder(folder: &Path) -> bool {
    let roots: Vec<PathBuf> = ONEDRIVE_VARIABLES
        .iter()
        .filter_map(std::env::var_os)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect();
    is_cloud_path(folder, &roots)
}

fn is_cloud_path(folder: &Path, roots: &[PathBuf]) -> bool {
    // Windows paths ignore upper and lower case.
    let parts = |path: &Path| -> Vec<String> {
        path.components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    };
    let folder_parts = parts(folder);
    if roots.iter().any(|root| folder_parts.starts_with(&parts(root))) {
        return true;
    }
    folder_parts.iter().any(|name| {
        // OneDrive: "OneDrive", "OneDrive - Company".
        name == "onedrive"
            || name.starts_with("onedrive - ")
            // Dropbox: "Dropbox", "Dropbox (Company)".
            || name == "dropbox"
            || name.starts_with("dropbox (")
            // Google Drive, including the "My Drive" folder of its virtual drive
            // (named "Drive Saya" in Indonesian).
            || name == "google drive"
            || name == "my drive"
            || name == "drive saya"
    })
}

/// True when the folder holds a second copy of the database made by a sync program.
pub fn has_conflict_copy(folder: &Path) -> bool {
    let in_cloud = is_cloud_folder(folder);
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .any(|entry| is_conflict_copy(&entry.file_name().to_string_lossy(), in_cloud))
}

/// Recognises conflict copies of `library.sqlite` by name:
/// - anywhere: names that say so, e.g. "library (Ana's conflicted copy 2026-11-25).sqlite";
/// - in a cloud folder only (outside one, such a name is more likely the user's own copy):
///   "library (1).sqlite" (Google Drive) and "library-LAPTOP.sqlite" (OneDrive).
///
/// SLReader's own migration backups ("library-v1-backup.sqlite") are never conflicts.
fn is_conflict_copy(file_name: &str, in_cloud: bool) -> bool {
    let name = file_name.to_lowercase();
    let Some(rest) = name
        .strip_suffix(".sqlite")
        .and_then(|stem| stem.strip_prefix("library"))
    else {
        return false;
    };
    if is_backup_suffix(rest) {
        return false;
    }
    if rest.contains("conflict") {
        return true;
    }
    in_cloud
        && ((rest.starts_with(" (") && rest.ends_with(')')) || (rest.len() > 1 && rest.starts_with('-')))
}

/// "-v1-backup", "-v12-backup-2", ...
fn is_backup_suffix(rest: &str) -> bool {
    let Some(after_v) = rest.strip_prefix("-v") else {
        return false;
    };
    let after_digits = after_v.trim_start_matches(|c: char| c.is_ascii_digit());
    after_digits.len() < after_v.len() && after_digits.starts_with("-backup")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_cloud_folders_by_name() {
        for path in [
            "C:/Users/a/OneDrive/review",
            "C:/Users/a/onedrive - Universitas/review",
            "C:/Users/a/Dropbox/review",
            "C:/Users/a/Dropbox (Lab)/review",
            "C:/Users/a/Google Drive/review",
            "G:/My Drive/review",
            "G:/Drive Saya/review",
        ] {
            assert!(is_cloud_path(Path::new(path), &[]), "{path}");
        }
    }

    #[test]
    fn leaves_ordinary_folders_alone() {
        for path in [
            "C:/Users/a/Documents/review",
            "C:/Users/a/OneDrive backup old/review",
            "C:/Users/a/dropbox-notes/review",
            "D:/Drive/review",
        ] {
            assert!(!is_cloud_path(Path::new(path), &[]), "{path}");
        }
    }

    #[test]
    fn recognises_the_folder_onedrive_announces() {
        let roots = [PathBuf::from("D:/Cloud")];
        assert!(is_cloud_path(Path::new("d:/cloud/review"), &roots));
        assert!(is_cloud_path(Path::new("D:/Cloud"), &roots));
        assert!(!is_cloud_path(Path::new("D:/CloudOther/review"), &roots));
    }

    #[test]
    fn recognises_conflict_copies() {
        for name in [
            "library (Ana's conflicted copy 2026-11-25).sqlite",
            "library.sync-conflict-20261125-101500.sqlite",
        ] {
            assert!(is_conflict_copy(name, false), "{name}");
        }
        for name in [
            "library (1).sqlite",
            "library (salinan bentrok Ana).sqlite",
            "library-LAPTOP-AB12.sqlite",
            "Library-DESKTOP.sqlite",
        ] {
            assert!(is_conflict_copy(name, true), "{name}");
            assert!(!is_conflict_copy(name, false), "{name} outside a cloud folder");
        }
    }

    #[test]
    fn slreaders_own_files_are_not_conflicts() {
        for name in [
            "library.sqlite",
            "library.sqlite-journal",
            "library-v1-backup.sqlite",
            "library-v12-backup-3.sqlite",
            "libraryold.sqlite",
            "other.sqlite",
            "library (1).pdf",
        ] {
            assert!(!is_conflict_copy(name, true), "{name}");
        }
    }

    #[test]
    fn finds_a_conflict_copy_in_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Dropbox").join("review");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("library.sqlite"), "x").unwrap();
        assert!(!has_conflict_copy(&folder));
        std::fs::write(folder.join("library (1).sqlite"), "x").unwrap();
        assert!(has_conflict_copy(&folder));
        assert!(!has_conflict_copy(&folder.join("missing")));
    }
}
