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
        println!("cargo:warning=UI directory not found, skipping frontend build");
        return;
    }

    // If ui/dist already exists, we can skip the build process
    if Path::new(ui_dist_dir).exists() {
        println!("cargo:warning=UI dist directory found, using existing assets");
        return;
    }

    // Check if npm is available
    if !is_npm_available() {
        println!("cargo:warning=npm not found, skipping frontend build. Frontend will show fallback page.");
        println!("cargo:warning=To build with frontend: install Node.js/npm and run 'cd ui && npm install && npm run build' before building");
        return;
    }
    
    // Check if node_modules exists, if not run npm install
    if !Path::new("ui/node_modules").exists() {
        println!("cargo:warning=Installing npm dependencies...");
        let output = Command::new("npm")
            .args(["install"])
            .current_dir(ui_dir)
            .output();
            
        match output {
            Ok(output) => {
                if !output.status.success() {
                    println!("cargo:warning=npm install failed: {}", String::from_utf8_lossy(&output.stderr));
                    println!("cargo:warning=Continuing without frontend assets");
                    return;
                }
            }
            Err(e) => {
                println!("cargo:warning=Failed to run npm install: {}", e);
                println!("cargo:warning=Continuing without frontend assets");
                return;
            }
        }
    }
    
    // Run npm run build
    println!("cargo:warning=Building frontend assets...");
    let output = Command::new("npm")
        .args(["run", "build"])
        .current_dir(ui_dir)
        .output();
        
    match output {
        Ok(output) => {
            if !output.status.success() {
                println!("cargo:warning=npm run build failed: {}", String::from_utf8_lossy(&output.stderr));
                println!("cargo:warning=Continuing without frontend assets");
                return;
            }
            println!("cargo:warning=Frontend build completed successfully");
        }
        Err(e) => {
            println!("cargo:warning=Failed to run npm run build: {}", e);
            println!("cargo:warning=Continuing without frontend assets");
            return;
        }
    }
    
    // Verify that dist directory now exists
    if !Path::new(ui_dist_dir).exists() {
        println!("cargo:warning=UI dist directory not found after build, frontend assets will not be available");
    }
}

fn is_npm_available() -> bool {
    Command::new("npm")
        .args(["--version"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

