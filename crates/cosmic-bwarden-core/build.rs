fn get_year_month_seconds_passed(epoch_secs: u64) -> (u32, u32, u64) {
    let mut seconds_left = epoch_secs;
    let mut year = 1970;
    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        let secs_in_year = days_in_year * 24 * 3600;
        if seconds_left >= secs_in_year {
            seconds_left -= secs_in_year;
            year += 1;
        } else {
            break;
        }
    }
    
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mut month = 1;
    let days_in_months = [
        31,
        if is_leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    
    for &days in &days_in_months {
        let secs_in_month = days * 24 * 3600;
        if seconds_left >= secs_in_month {
            seconds_left -= secs_in_month;
            month += 1;
        } else {
            break;
        }
    }
    
    (year, month, seconds_left)
}

fn main() {
    // Re-run if git HEAD changes so we update the short git id
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Ok(git_dir) = std::fs::canonicalize(".git") {
        if let Ok(head_ref) = std::fs::read_to_string(git_dir.join("HEAD")) {
            if head_ref.starts_with("ref:") {
                let ref_path = head_ref.split_whitespace().nth(1).unwrap_or("");
                println!("cargo:rerun-if-changed=.git/{}", ref_path);
            }
        }
    }

    let target_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let version_file = target_dir.join("build_version.txt");
    
    let mut use_existing = false;
    let mut version_str = String::new();
    
    if let Ok(metadata) = std::fs::metadata(&version_file) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                // If it was written less than 30 seconds ago, reuse it to ensure unity across build process
                if elapsed.as_secs() < 30 {
                    if let Ok(content) = std::fs::read_to_string(&version_file) {
                        version_str = content.trim().to_string();
                        use_existing = !version_str.is_empty();
                    }
                }
            }
        }
    }
    
    if !use_existing {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (year, month, secs_passed) = get_year_month_seconds_passed(now);
        
        let git_id = std::process::Command::new("git")
            .args(&["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() {
                    String::from_utf8(out.stdout).ok().map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "unknown".to_string());
            
        version_str = format!("{}.{:02}-{}-{}", year, month, secs_passed, git_id);
        
        let _ = std::fs::create_dir_all(&target_dir);
        let _ = std::fs::write(&version_file, &version_str);
    }
    
    println!("cargo:rustc-env=COSMIC_BWARDEN_VERSION={}", version_str);
}
