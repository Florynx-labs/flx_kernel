//! Host runner tool for FLX Kernel.
//!
//! Handles building the kernel ELF binary, packaging it into a UEFI bootable disk image
//! using the bootloader crate, and launching QEMU with OVMF firmware.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<()> {
    println!("==================================================");
    println!(" FLX Kernel Runner (Florynx Labs)");
    println!("==================================================");

    // 1. Locate repository root
    let root_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .context("Failed to locate workspace root")?;

    let is_release = std::env::args().any(|arg| arg == "--release");
    let no_qemu = std::env::args().any(|arg| arg == "--no-run");

    // 2. Build kernel ELF with cargo
    println!("[1/4] Building kernel binary for target x86_64-unknown-none...");
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd
        .arg("build")
        .arg("--package")
        .arg("flx-kernel")
        .arg("--target")
        .arg("x86_64-unknown-none")
        .current_dir(&root_dir);

    if is_release {
        cargo_cmd.arg("--release");
    }

    let status = cargo_cmd.status().context("Failed to execute cargo build")?;
    if !status.success() {
        bail!("Kernel compilation failed with status: {}", status);
    }

    // 3. Locate compiled kernel ELF
    let profile = if is_release { "release" } else { "debug" };
    let kernel_binary = root_dir
        .join("target")
        .join("x86_64-unknown-none")
        .join(profile)
        .join("flx-kernel");

    if !kernel_binary.exists() {
        bail!("Kernel binary not found at: {}", kernel_binary.display());
    }
    println!("[2/4] Kernel binary compiled: {}", kernel_binary.display());

    // 4. Create UEFI bootable disk image
    let uefi_image = root_dir
        .join("target")
        .join(format!("flx-kernel-uefi-{}.img", profile));

    println!("[3/4] Creating UEFI disk image at: {}", uefi_image.display());
    let uefi_boot = bootloader::UefiBoot::new(&kernel_binary);
    uefi_boot
        .create_disk_image(&uefi_image)
        .context("Failed to create UEFI disk image")?;
    println!("[3/4] UEFI disk image successfully created!");

    if no_qemu {
        println!("[INFO] --no-run specified. Done.");
        return Ok(());
    }

    // 5. Locate OVMF firmware (downloaded once and cached under target/ovmf).
    println!("[4/4] Locating OVMF UEFI firmware...");
    let prebuilt = ovmf_prebuilt::Prebuilt::fetch(
        ovmf_prebuilt::Source::LATEST,
        root_dir.join("target").join("ovmf"),
    )
    .context("Failed to fetch OVMF firmware")?;
    let ovmf_code = prebuilt.get_file(ovmf_prebuilt::Arch::X64, ovmf_prebuilt::FileType::Code);
    let ovmf_vars = prebuilt.get_file(ovmf_prebuilt::Arch::X64, ovmf_prebuilt::FileType::Vars);
    println!("      OVMF code: {}", ovmf_code.display());
    println!("      OVMF vars: {}", ovmf_vars.display());

    // 6. Locate QEMU binary
    let qemu_bin = find_qemu()?;
    println!("      Using QEMU: {}", qemu_bin.display());

    // 7. Launch QEMU
    println!("--------------------------------------------------");
    println!(" Launching QEMU with UEFI (OVMF)...");
    println!("--------------------------------------------------");

    let mut qemu_cmd = Command::new(&qemu_bin);
    qemu_cmd
        .arg("-drive")
        .arg(format!(
            "if=pflash,format=raw,readonly=on,file={}",
            ovmf_code.display()
        ))
        .arg("-drive")
        .arg(format!("if=pflash,format=raw,file={}", ovmf_vars.display()))
        .arg("-drive")
        .arg(format!("format=raw,file={}", uefi_image.display()))
        .arg("-serial")
        .arg("stdio")
        .arg("-m")
        .arg("512M");

    // Pass through any extra arguments
    for arg in std::env::args().skip(1) {
        if arg == "--release" || arg == "--no-run" {
            continue;
        }
        qemu_cmd.arg(arg);
    }

    let qemu_status = qemu_cmd.status().context("Failed to execute QEMU")?;
    println!("QEMU exited with status: {}", qemu_status);

    Ok(())
}

fn find_qemu() -> Result<PathBuf> {
    // 1. Check if qemu-system-x86_64 is in PATH
    if let Ok(path) = which::which("qemu-system-x86_64") {
        return Ok(path);
    }

    // 2. Check standard Windows installation directories
    let candidates = [
        r"C:\Program Files\qemu\qemu-system-x86_64.exe",
        r"C:\Program Files (x86)\qemu\qemu-system-x86_64.exe",
        r"C:\tools\qemu\qemu-system-x86_64.exe",
    ];

    for path_str in &candidates {
        let p = Path::new(path_str);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }

    bail!("qemu-system-x86_64 not found in PATH or standard directories.")
}

mod which {
    use std::path::PathBuf;

    pub fn which(binary_name: &str) -> Result<PathBuf, ()> {
        let path_var = std::env::var_os("PATH").ok_or(())?;
        for dir in std::env::split_paths(&path_var) {
            let direct = dir.join(binary_name);
            if direct.is_file() {
                return Ok(direct);
            }
            let with_exe = dir.join(format!("{}.exe", binary_name));
            if with_exe.is_file() {
                return Ok(with_exe);
            }
        }
        Err(())
    }
}
