use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct ProcessManager {
    child: Arc<Mutex<Option<Child>>>,
    command: Vec<String>,
}

impl ProcessManager {
    pub fn new(command: Vec<String>) -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            command,
        }
    }

    pub fn start(&self) -> Result<()> {
        let mut child_lock = self.child.lock();

        if child_lock.is_some() {
            return Ok(()); // Already running
        }

        if self.command.is_empty() {
            return Err(anyhow::anyhow!("No command specified"));
        }

        info!("Starting command: {:?}", self.command);

        let mut cmd = Command::new(&self.command[0]);

        if self.command.len() > 1 {
            cmd.args(&self.command[1..]);
        }

        // Try to resolve the command if it's not found
        let child = cmd
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdin(Stdio::null())
            .spawn()
            .with_context(|| {
                format!("Failed to start command: {:?}. Make sure the command is in your PATH and executable.", self.command)
            })?;

        info!("Started upstream process with PID: {}", child.id());
        *child_lock = Some(child);

        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        let mut child_lock = self.child.lock();

        if let Some(mut child) = child_lock.take() {
            info!("Stopping upstream process with PID: {}", child.id());

            #[cfg(unix)]
            {
                // Try graceful shutdown first
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGTERM);
                }

                // Wait a bit for graceful shutdown
                std::thread::sleep(std::time::Duration::from_millis(100));

                match child.try_wait() {
                    Ok(Some(status)) => {
                        info!("Process exited gracefully with status: {}", status);
                        return Ok(());
                    }
                    Ok(None) => {
                        warn!("Process didn't exit gracefully, force killing");
                        // Force kill if still running
                        unsafe {
                            libc::kill(child.id() as i32, libc::SIGKILL);
                        }
                    }
                    Err(e) => {
                        error!("Error checking process status: {}", e);
                    }
                }
            }

            #[cfg(windows)]
            {
                let _ = child.kill();
            }

            let _ = child.wait();
        }

        Ok(())
    }

    pub fn is_running(&self) -> bool {
        let mut child_lock = self.child.lock();

        if let Some(child) = child_lock.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Process has exited
                    *child_lock = None;
                    false
                }
                Ok(None) => {
                    // Process is still running
                    true
                }
                Err(_) => {
                    // Error checking status, assume not running
                    *child_lock = None;
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn get_pid(&self) -> Option<u32> {
        self.child.lock().as_ref().map(|child| child.id())
    }

    pub fn restart(&self) -> Result<()> {
        self.stop()?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.start()
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        if let Err(e) = self.stop() {
            error!("Error stopping process in drop: {}", e);
        }
    }
}

#[cfg(unix)]
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

#[cfg(unix)]
mod libc {
    pub const SIGTERM: i32 = 15;
    pub const SIGKILL: i32 = 9;

    pub unsafe fn kill(pid: i32, sig: i32) -> i32 {
        super::kill(pid, sig)
    }
}
