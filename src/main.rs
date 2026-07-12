use colored::*;
use serde::Deserialize;
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct DiskInfo {
    pub media_name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Partition {
    pub volume_name: Option<String>,
    pub device_identifier: Option<String>,

    #[serde(rename = "APFSVolumes", default)]
    pub apfs_volumes: Vec<Partition>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicalStore {
    pub device_identifier: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct DiskEntry {
    pub device_identifier: String,

    #[serde(default)]
    pub size: u64,

    pub volume_name: Option<String>,

    #[serde(default)]
    pub partitions: Vec<Partition>,

    #[serde(rename = "APFSVolumes", default)]
    pub apfs_volumes: Vec<Partition>,

    #[serde(rename = "APFSPhysicalStores", default)]
    pub apfs_physical_stores: Vec<PhysicalStore>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct DiskList {
    pub whole_disks: Vec<String>,
    pub all_disks_and_partitions: Vec<DiskEntry>,
}

pub struct EjectableDisk {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub volumes: Vec<String>,
}

fn human_size(b: f64) -> String {
    if b >= 1_099_511_627_776.0 {
        format!("{:.1} TB", b / 1_099_511_627_776.0)
    } else if b >= 1_073_741_824.0 {
        format!("{:.1} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else {
        format!("{:.1} KB", b / 1024.0)
    }
}

fn fetch_external_disks() -> Vec<EjectableDisk> {
    let out = Command::new("diskutil")
        .args(["list", "-plist", "external"])
        .output()
        .expect("Failed to execute diskutil");

    let list: DiskList = plist::from_bytes(&out.stdout).unwrap_or_default();

    // Separate APFS container disks from physical disks
    let (container_disks, physical_disks): (Vec<DiskEntry>, Vec<DiskEntry>) = list
        .all_disks_and_partitions
        .into_iter()
        .filter(|disk| list.whole_disks.contains(&disk.device_identifier))
        .partition(|disk| !disk.apfs_physical_stores.is_empty());

    physical_disks
        .into_iter()
        .map(|disk| {
            let mut volumes = Vec::new();

            // 1. Check volume name directly on the disk
            if let Some(v) = &disk.volume_name {
                if v != "EFI" {
                    volumes.push(v.clone());
                }
            }

            // 2. Check partitions
            for p in &disk.partitions {
                if let Some(v) = &p.volume_name {
                    if v != "EFI" {
                        volumes.push(v.clone());
                    }
                }

                // Check nested APFS volumes in partition
                for apfs in &p.apfs_volumes {
                    if let Some(v) = &apfs.volume_name {
                        if v != "EFI" {
                            volumes.push(v.clone());
                        }
                    }
                }

                // Check for APFS container disks backed by this partition
                if let Some(part_id) = &p.device_identifier {
                    for container in &container_disks {
                        let is_store = container
                            .apfs_physical_stores
                            .iter()
                            .any(|store| store.device_identifier.as_ref() == Some(part_id));
                        if is_store {
                            for apfs in &container.apfs_volumes {
                                if let Some(v) = &apfs.volume_name {
                                    if v != "EFI" {
                                        volumes.push(v.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 3. Check for APFS volumes directly on the disk entry (in case it behaves like a container)
            for apfs in &disk.apfs_volumes {
                if let Some(v) = &apfs.volume_name {
                    if v != "EFI" {
                        volumes.push(v.clone());
                    }
                }
            }

            volumes.sort();
            volumes.dedup();

            // get hardware name
            let info_out = Command::new("diskutil")
                .args(["info", "-plist", &disk.device_identifier])
                .output()
                .expect("Failed to fetch disk info");

            let info: DiskInfo = plist::from_bytes(&info_out.stdout).unwrap_or_default();

            EjectableDisk {
                id: disk.device_identifier,
                name: info.media_name.unwrap_or_else(|| "Unknown".to_string()),
                size: disk.size,
                volumes,
            }
        })
        .collect()
}

fn main() {
    println!("\n{}", "💿 Disk Eject Helper".cyan().bold());

    let disks = fetch_external_disks();
    if disks.is_empty() {
        println!("{}", "⚠ No ejectable disks found.".yellow());
        return;
    }

    let mut fzf_input = String::new();
    for d in &disks {
        let vols = if d.volumes.is_empty() {
            "(no mounted volumes)".to_string()
        } else {
            d.volumes.join(", ")
        };
        fzf_input.push_str(&format!(
            "{} ({}) - {} | Volumes: {}\n",
            d.name,
            human_size(d.size as f64),
            d.id,
            vols
        ));
    }

    let mut child = Command::new("fzf")
        .args([
            "--prompt",
            "Select disk to eject > ",
            "--height",
            "40%",
            "--layout",
            "reverse",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to launch fzf. Is it installed?");

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(fzf_input.as_bytes())
            .expect("Failed to write to fzf stdin");
    }

    let output = child.wait_with_output().expect("Failed to read fzf output");

    if !output.status.success() {
        println!("{}", "Cancelled.".dimmed());
        return;
    }

    let selection = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let target = disks
        .iter()
        .find(|d| selection.contains(&format!("- {} |", d.id)))
        .expect("Failed to parse fzf selection");

    println!(
        "\n{} {} {}…",
        "⏏ Ejecting".yellow(),
        target.name.bold(),
        format!("({})", target.id).yellow()
    );

    let success = Command::new("diskutil")
        .args(["eject", &target.id])
        .status()
        .map_or(false, |s| s.success());

    if success {
        println!(
            "{}",
            format!("✓ Successfully ejected {}.", target.name).green()
        );
    } else {
        println!(
            "{}",
            format!(
                "✗ Failed to eject {}. Try: diskutil unmountDisk /dev/{}",
                target.name, target.id
            )
            .red()
        );
    }
}
