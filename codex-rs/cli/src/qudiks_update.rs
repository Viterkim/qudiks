use anyhow::Context;
use std::env;
use std::process::Command;

pub fn run() -> anyhow::Result<()> {
    anyhow::ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "Qudiks binary updates are only available on Linux x86_64. Use the source installer on this platform."
    );
    let executable = env::current_exe().context("Cannot locate the running Qudiks binary")?;
    anyhow::ensure!(
        executable
            .file_name()
            .is_some_and(|name| name == "qudiks-bin"),
        "Run the installed qudiks launcher to update; rebuild development binaries from source."
    );
    let bin_dir = executable
        .parent()
        .context("Cannot locate the Qudiks install directory")?;
    let status = Command::new("bash")
        .args([
            "-c",
            include_str!("../assets/qudiks-update.sh"),
            "qudiks-update",
        ])
        .arg(bin_dir)
        .current_dir(env::temp_dir())
        .status()
        .context("Cannot start the Qudiks updater")?;
    anyhow::ensure!(
        status.success(),
        "Qudiks update failed with status {status}. Your session can be resumed after relaunching."
    );
    println!("Qudiks updated. Relaunch qudiks to use the new version.");
    Ok(())
}
