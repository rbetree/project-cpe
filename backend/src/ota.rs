use crate::models::{OtaMeta, OtaStatusResponse, OtaUploadResponse, OtaValidation};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;

const OTA_STAGING_DIR: &str = "/tmp/ota_staging";
const OTA_BINARY_PATH: &str = "/home/root/udx710";
const OTA_WWW_PATH: &str = "/home/root/www";

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn get_current_commit() -> String {
    option_env!("GIT_COMMIT").unwrap_or("unknown").to_string()
}

pub fn get_ota_status() -> OtaStatusResponse {
    let pending_meta = read_pending_meta();

    OtaStatusResponse {
        current_version: CURRENT_VERSION.to_string(),
        current_commit: get_current_commit(),
        pending_update: pending_meta.is_some(),
        pending_meta,
    }
}

fn read_pending_meta() -> Option<OtaMeta> {
    let meta_path = Path::new(OTA_STAGING_DIR).join("meta.json");
    fs::read_to_string(meta_path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

pub fn handle_ota_upload(data: &[u8]) -> Result<OtaUploadResponse, String> {
    let _ = fs::remove_dir_all(OTA_STAGING_DIR);
    fs::create_dir_all(OTA_STAGING_DIR)
        .map_err(|e| format!("Failed to create staging dir: {}", e))?;

    let is_zip = detect_zip_format(data);

    if is_zip {
        let zip_path = Path::new(OTA_STAGING_DIR).join("update.zip");
        let mut file = fs::File::create(&zip_path)
            .map_err(|e| format!("Failed to create zip file: {}", e))?;
        file.write_all(data)
            .map_err(|e| format!("Failed to write zip file: {}", e))?;

        let output = Command::new("unzip")
            .args(["-o", zip_path.to_str().unwrap_or(""), "-d", OTA_STAGING_DIR])
            .output()
            .map_err(|e| format!("Failed to extract zip: {}. Make sure 'unzip' is installed.", e))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to extract zip: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let _ = fs::remove_file(&zip_path);
    } else {
        let tar_path = Path::new(OTA_STAGING_DIR).join("update.tar.gz");
        let mut file = fs::File::create(&tar_path)
            .map_err(|e| format!("Failed to create tar file: {}", e))?;
        file.write_all(data)
            .map_err(|e| format!("Failed to write tar file: {}", e))?;

        let output = Command::new("tar")
            .args(["-xzf", tar_path.to_str().unwrap_or(""), "-C", OTA_STAGING_DIR])
            .output()
            .map_err(|e| format!("Failed to extract tar: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to extract tar: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let _ = fs::remove_file(&tar_path);
    }

    fix_file_permissions(OTA_STAGING_DIR)?;

    let meta_path = Path::new(OTA_STAGING_DIR).join("meta.json");
    let meta_content = fs::read_to_string(&meta_path)
        .map_err(|_| "meta.json not found in OTA package".to_string())?;

    let meta: OtaMeta = serde_json::from_str(&meta_content)
        .map_err(|e| format!("Invalid meta.json: {}", e))?;

    let validation = validate_ota_package(&meta)?;

    Ok(OtaUploadResponse { meta, validation })
}

fn validate_ota_package(meta: &OtaMeta) -> Result<OtaValidation, String> {
    let binary_path = Path::new(OTA_STAGING_DIR).join("udx710");
    let www_path = Path::new(OTA_STAGING_DIR).join("www");

    if !binary_path.exists() {
        return Ok(OtaValidation {
            valid: false,
            is_newer: false,
            binary_md5_match: false,
            frontend_md5_match: false,
            arch_match: false,
            error: Some("Binary file not found in package".to_string()),
        });
    }

    if !www_path.exists() {
        return Ok(OtaValidation {
            valid: false,
            is_newer: false,
            binary_md5_match: false,
            frontend_md5_match: false,
            arch_match: false,
            error: Some("Frontend directory not found in package".to_string()),
        });
    }

    let binary_md5 = calculate_file_md5(&binary_path)?;
    let binary_md5_match = binary_md5 == meta.binary_md5;
    let frontend_md5 = calculate_directory_md5(&www_path)?;
    let frontend_md5_match = frontend_md5 == meta.frontend_md5;
    let arch_match = meta.arch == "aarch64-unknown-linux-musl";
    let is_newer = compare_versions(&meta.version, CURRENT_VERSION);

    let valid = binary_md5_match && frontend_md5_match && arch_match;

    let error = if !valid {
        let mut errors = Vec::new();
        if !binary_md5_match {
            errors.push(format!(
                "Binary MD5 mismatch: expected={}, actual={}",
                meta.binary_md5, binary_md5
            ));
        }
        if !frontend_md5_match {
            errors.push(format!(
                "Frontend MD5 mismatch: expected={}, actual={}",
                meta.frontend_md5, frontend_md5
            ));
        }
        if !arch_match {
            errors.push(format!(
                "Arch mismatch: expected=aarch64-unknown-linux-musl, actual={}",
                meta.arch
            ));
        }
        Some(errors.join("; "))
    } else {
        None
    };

    Ok(OtaValidation {
        valid,
        is_newer,
        binary_md5_match,
        frontend_md5_match,
        arch_match,
        error,
    })
}

fn calculate_file_md5(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|e| format!("Failed to open file {}: {}", path.display(), e))?;

    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;

    Ok(format!("{:x}", md5::compute(&contents)))
}

fn collect_directory_hashes(path: &Path, hashes: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let entry_path = entry.path();

        if entry_path.is_dir() {
            collect_directory_hashes(&entry_path, hashes)?;
        } else {
            hashes.push(calculate_file_md5(&entry_path)?);
        }
    }

    Ok(())
}

fn calculate_directory_md5(path: &Path) -> Result<String, String> {
    let mut hashes = Vec::new();
    collect_directory_hashes(path, &mut hashes)?;
    hashes.sort();

    let mut payload = hashes.join("\n");
    if !payload.is_empty() {
        payload.push('\n');
    }

    Ok(format!("{:x}", md5::compute(payload.as_bytes())))
}

fn compare_versions(v1: &str, v2: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    let v1_parts = parse(v1);
    let v2_parts = parse(v2);

    for i in 0..std::cmp::max(v1_parts.len(), v2_parts.len()) {
        let p1 = v1_parts.get(i).unwrap_or(&0);
        let p2 = v2_parts.get(i).unwrap_or(&0);
        if p1 > p2 {
            return true;
        } else if p1 < p2 {
            return false;
        }
    }
    false
}

pub fn apply_ota_update(restart_now: bool) -> Result<String, String> {
    let meta = read_pending_meta()
        .ok_or_else(|| "No pending update".to_string())?;
    let validation = validate_ota_package(&meta)?;
    if !validation.valid {
        return Err(validation
            .error
            .unwrap_or_else(|| "OTA package validation failed".to_string()));
    }

    let staging_binary = Path::new(OTA_STAGING_DIR).join("udx710");
    let staging_www = Path::new(OTA_STAGING_DIR).join("www");

    fs::copy(&staging_binary, OTA_BINARY_PATH)
        .map_err(|e| format!("Failed to copy binary: {}", e))?;

    Command::new("chmod")
        .args(["755", OTA_BINARY_PATH])
        .output()
        .map_err(|e| format!("Failed to chmod binary: {}", e))?;

    let _ = fs::remove_dir_all(OTA_WWW_PATH);
    copy_dir_recursive(staging_www.to_str().unwrap_or(""), OTA_WWW_PATH)?;
    fix_file_permissions("/home/root")?;
    crate::config::ensure_loader_hooks_init()?;

    let _ = fs::remove_dir_all(OTA_STAGING_DIR);

    if restart_now {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let _ = Command::new("reboot").spawn();
        });
    }

    Ok(format!("Update to version {} applied successfully", meta.version))
}

fn copy_dir_recursive(src: &str, dst: &str) -> Result<(), String> {
    fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create dir {}: {}", dst, e))?;

    let entries = fs::read_dir(src)
        .map_err(|e| format!("Failed to read src dir {}: {}", src, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = Path::new(dst).join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(
                src_path.to_str().unwrap_or(""),
                dst_path.to_str().unwrap_or(""),
            )?;
        } else {
            fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy file {}: {}", src_path.display(), e))?;
        }
    }

    Ok(())
}

pub fn cancel_pending_update() -> Result<(), String> {
    if Path::new(OTA_STAGING_DIR).exists() {
        fs::remove_dir_all(OTA_STAGING_DIR)
            .map_err(|e| format!("Failed to remove staging dir: {}", e))?;
    }
    Ok(())
}

fn detect_zip_format(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }

    data[0] == 0x50 && data[1] == 0x4B && data[2] == 0x03 && data[3] == 0x04
}

fn fix_file_permissions(root: &str) -> Result<(), String> {
    let binary_path = format!("{}/udx710", root);
    let www_path = format!("{}/www", root);

    if Path::new(&binary_path).exists() {
        Command::new("chmod")
            .args(["755", &binary_path])
            .output()
            .map_err(|e| format!("Failed to chmod binary {}: {}", binary_path, e))?;
    }

    if Path::new(&www_path).exists() {
        let _ = Command::new("find")
            .args([&www_path, "-type", "d", "-exec", "chmod", "755", "{}", "+"])
            .output();

        let _ = Command::new("find")
            .args([&www_path, "-type", "f", "-exec", "chmod", "644", "{}", "+"])
            .output();
    }

    Ok(())
}
