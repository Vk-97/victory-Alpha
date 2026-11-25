//! Build-script helper: locates the Microsoft DirectX 12 Agility SDK
//! (`D3D12Core.dll` and optional `d3d12SDKLayers.dll`) outside the repository,
//! stages it next to built binaries/tests under `target/<profile>/D3D12/`, and
//! emits the linker exports (`D3D12SDKVersion`, `D3D12SDKPath`) required on
//! Windows 10 for Shader Model 6.6 and dynamic descriptor heaps.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn configure(stage_bins: bool, stage_tests: bool) {
    println!("cargo:rustc-check-cfg=cfg(victory_agility_sdk)");
    println!("cargo:rerun-if-env-changed=VICTORY_AGILITY_SDK");
    println!("cargo:rerun-if-env-changed=VICTORY_CONFIG");
    println!("cargo:rerun-if-env-changed=VICTORY_CACHE_DIR");
    println!("cargo:rerun-if-env-changed=VICTORY_DATA_DIR");

    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace_root = find_workspace_root(&manifest_dir).unwrap_or(manifest_dir);
    let config_path = env::var_os("VICTORY_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("victory.toml"));
    if config_path.is_file() {
        println!("cargo:rerun-if-changed={}", config_path.display());
    }

    let Some(sdk_bin_dir) = find_sdk_bin_dir(&config_path) else {
        return;
    };
    let core_dll = sdk_bin_dir.join("D3D12Core.dll");
    let Some(version) = read_sdk_version(&core_dll) else {
        return;
    };

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let Some(profile_dir) = out_dir.ancestors().nth(3) else {
        return;
    };

    if stage_bins {
        stage_dlls(&sdk_bin_dir, &profile_dir.join("D3D12"));
    }
    if stage_tests {
        stage_dlls(&sdk_bin_dir, &profile_dir.join("deps").join("D3D12"));
    }
    if stage_bins || stage_tests {
        println!("cargo:rustc-link-arg=/EXPORT:D3D12SDKVersion,DATA");
        println!("cargo:rustc-link-arg=/EXPORT:D3D12SDKPath,DATA");
    }

    println!("cargo:rustc-env=VICTORY_AGILITY_SDK_VERSION={version}");
    println!("cargo:rustc-cfg=victory_agility_sdk");
    println!("cargo:rerun-if-changed={}", core_dll.display());
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".victory-root").is_file())
        .map(Path::to_path_buf)
}

fn find_sdk_bin_dir(config_path: &Path) -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("VICTORY_AGILITY_SDK").filter(|v| !v.is_empty())
        && let Some(found) = resolve_sdk_bin(Path::new(&explicit))
    {
        return Some(found);
    }

    let mut roots = Vec::new();
    for var in ["VICTORY_CACHE_DIR", "VICTORY_DATA_DIR"] {
        if let Some(p) = env::var_os(var).filter(|v| !v.is_empty()) {
            push_agility_candidates(Path::new(&p), &mut roots);
        }
    }
    if let Ok(text) = fs::read_to_string(config_path) {
        for key in ["cache_dir", "data_dir"] {
            if let Some(p) = parse_toml_string(&text, key) {
                push_agility_candidates(Path::new(&p), &mut roots);
            }
        }
    }
    if let Some(local) = env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        roots.push(Path::new(&local).join("victory").join("agility-sdk"));
    }

    let mut candidates = Vec::new();
    for root in roots {
        if let Some(found) = resolve_sdk_bin(&root) {
            candidates.push(found);
        }
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.filter_map(Result::ok) {
                if let Some(found) = resolve_sdk_bin(&entry.path()) {
                    candidates.push(found);
                }
            }
        }
    }
    candidates.sort();
    candidates.pop()
}

fn push_agility_candidates(dir: &Path, out: &mut Vec<PathBuf>) {
    out.push(dir.join("agility-sdk"));
    if let Some(parent) = dir.parent() {
        out.push(parent.join("agility-sdk"));
    }
}

fn resolve_sdk_bin(dir: &Path) -> Option<PathBuf> {
    let direct = dir.to_path_buf();
    if direct.join("D3D12Core.dll").is_file() {
        return Some(direct);
    }
    let nuget = dir.join("build").join("native").join("bin").join("x64");
    if nuget.join("D3D12Core.dll").is_file() {
        return Some(nuget);
    }
    None
}

fn parse_toml_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        let Some((lhs, rhs)) = trimmed.split_once('=') else {
            continue;
        };
        if lhs.trim() != key {
            continue;
        }
        let val = rhs.trim();
        if let Some(inner) = val.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            return Some(inner.to_owned());
        }
        if let Some(inner) = val.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return Some(inner.replace("\\\\", "\\"));
        }
    }
    None
}

/// Reads the Agility SDK minor version from `D3D12Core.dll`'s `VS_FIXEDFILEINFO`.
fn read_sdk_version(dll: &Path) -> Option<u32> {
    let bytes = fs::read(dll).ok()?;
    let magic = 0xFEEF_04BD_u32.to_le_bytes();
    let pos = bytes.windows(4).position(|w| w == magic)?;
    let ms_bytes: [u8; 4] = bytes.get(pos + 8..pos + 12)?.try_into().ok()?;
    let file_version_ms = u32::from_le_bytes(ms_bytes);
    let minor = file_version_ms & 0xFFFF;
    (minor > 0).then_some(minor)
}

fn stage_dlls(src_dir: &Path, dst_dir: &Path) {
    if fs::create_dir_all(dst_dir).is_err() {
        return;
    }
    for name in ["D3D12Core.dll", "d3d12SDKLayers.dll"] {
        let src = src_dir.join(name);
        if !src.is_file() {
            continue;
        }
        let dst = dst_dir.join(name);
        if needs_copy(&src, &dst) {
            let _ = fs::copy(&src, &dst);
        }
    }
}

fn needs_copy(src: &Path, dst: &Path) -> bool {
    let Ok(sm) = fs::metadata(src) else {
        return false;
    };
    let Ok(dm) = fs::metadata(dst) else {
        return true;
    };
    sm.len() != dm.len() || sm.modified().ok() != dm.modified().ok()
}
