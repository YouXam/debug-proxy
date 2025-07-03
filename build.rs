use std::path::Path;
use std::process::Command;

fn main() {
    let ui_dir = "ui";
    let ui_dist_dir = "ui/dist";
    
    // Tell cargo to rerun if ui source files change
    println!("cargo:rerun-if-changed=ui/src");
    println!("cargo:rerun-if-changed=ui/package.json");
    println!("cargo:rerun-if-changed=ui/package-lock.json");
    
    // Check if ui directory exists
    if !Path::new(ui_dir).exists() {
        println!("cargo:warning=UI directory not found, creating minimal fallback");
        return;
    }
    
    // Check if node_modules exists, if not run npm install
    if !Path::new("ui/node_modules").exists() {
        println!("cargo:warning=Installing npm dependencies...");
        let output = Command::new("npm")
            .args(&["install"])
            .current_dir(ui_dir)
            .output();
            
        match output {
            Ok(output) => {
                if !output.status.success() {
                    println!("cargo:warning=npm install failed: {}", String::from_utf8_lossy(&output.stderr));
                    return;
                }
            }
            Err(e) => {
                println!("cargo:warning=Failed to run npm install: {}", e);
                return;
            }
        }
    }
    
    // Run npm run build if dist directory doesn't exist or is older than src
    let should_build = if !Path::new(ui_dist_dir).exists() {
        true
    } else {
        // Check if src is newer than dist
        let src_modified = get_dir_modified_time("ui/src").unwrap_or(0);
        let dist_modified = get_dir_modified_time(ui_dist_dir).unwrap_or(0);
        src_modified > dist_modified
    };
    
    if should_build {
        println!("cargo:warning=Building frontend assets...");
        let output = Command::new("npm")
            .args(&["run", "build"])
            .current_dir(ui_dir)
            .output();
            
        match output {
            Ok(output) => {
                if !output.status.success() {
                    println!("cargo:warning=npm run build failed: {}", String::from_utf8_lossy(&output.stderr));
                    return;
                }
                println!("cargo:warning=Frontend build completed successfully");
            }
            Err(e) => {
                println!("cargo:warning=Failed to run npm run build: {}", e);
                return;
            }
        }
    }
    
    // Verify that dist directory now exists
    if !Path::new(ui_dist_dir).exists() {
        println!("cargo:warning=UI dist directory not found after build, frontend assets will not be available");
    }
}

fn get_dir_modified_time(dir: &str) -> Option<u64> {
    use std::fs;
    
    let mut latest = 0u64;
    
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        latest = latest.max(duration.as_secs());
                    }
                }
            }
            
            // Recursively check subdirectories
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    if let Some(sub_time) = get_dir_modified_time(&entry.path().to_string_lossy()) {
                        latest = latest.max(sub_time);
                    }
                }
            }
        }
    }
    
    if latest > 0 { Some(latest) } else { None }
}