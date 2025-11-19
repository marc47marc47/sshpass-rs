#[cfg(windows)]
fn main() {
    if let Err(err) = copy_conpty_artifacts() {
        println!("cargo:warning=Windows PTY runtime files not prepared: {err}");
    }
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn copy_conpty_artifacts() -> Result<(), String> {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    let profile = env::var("PROFILE").map_err(|e| e.to_string())?;

    let target_root = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("target"));
    let dest_dir = target_root.join(&profile);
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    let Some(src_dir) = locate_winpty_artifacts(&profile) else {
        return Err("unable to find winpty-rs build artifacts (conpty.dll)".to_string());
    };

    for artifact in ["conpty.dll", "OpenConsole.exe"] {
        let source = src_dir.join(artifact);
        if source.exists() {
            let destination = dest_dir.join(artifact);
            fs::copy(&source, &destination).map_err(|e| {
                format!(
                    "failed to copy {} from {} to {}: {}",
                    artifact,
                    source.display(),
                    destination.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

#[cfg(windows)]
fn locate_winpty_artifacts(profile: &str) -> Option<std::path::PathBuf> {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".cargo")))
        .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))?;

    let registry_src = cargo_home.join("registry").join("src");
    let registry_entries = fs::read_dir(registry_src).ok()?;

    for registry in registry_entries.flatten() {
        let registry_path = registry.path();
        if !registry_path.is_dir() {
            continue;
        }
        if let Ok(crate_entries) = fs::read_dir(&registry_path) {
            for entry in crate_entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let is_winpty = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|name| name.starts_with("winpty-rs-"))
                    .unwrap_or(false);
                if !is_winpty {
                    continue;
                }
                let dll_dir = path.join("target").join(profile);
                if dll_dir.join("conpty.dll").exists() {
                    return Some(dll_dir);
                }
            }
        }
    }

    None
}
