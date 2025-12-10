use std::env;
use std::fmt;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::path;
use std::path::{ Path, PathBuf };
use std::process::Command;
use walkdir::WalkDir;
use zip::write::{ FileOptions, ExtendedFileOptions };
use zip::ZipWriter;
use std::collections::BTreeMap;

use clap::{Parser, Subcommand};
use regex::Regex;

use crate::directories::get_android_tree;

mod directories;

struct Errors {
    errors: Vec<String>,
}

impl Errors {
    fn new() -> Self {
        Self { errors: Vec::new() }
    }

    fn push(&mut self, err: impl Into<String>) {
        self.errors.push(err.into());
    }

    fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for err in &self.errors {
            writeln!(f, "{}", err)?;
        }
        Ok(())
    }
}

#[derive(Subcommand)]
enum Commands {
    New {
        name: String,
        path: Option<PathBuf>,
    },
    Build {
        name: String,
        path: Option<PathBuf>,

        #[arg(long)]
        no_clear: bool,
        #[arg(long)]
        no_push: bool,
        #[arg(long)]
        no_reboot: bool,
        #[arg(long)]
        ignore_adb: bool,
    },
    ListDirectories { },
    GetAndroidTree { 
        directory_name: Option<String>,
    },
}

#[derive(Parser)]
#[command(name = "Magisk Module Manager")]
#[command(about = "CLI for Creating and Managing Magisk Modules", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn has_adb_device() -> bool {
    let output = std::process::Command::new("adb")
        .arg("devices")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .lines()
        .skip(1)
        .any(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            parts.len() == 2 && parts[1] == "device"
        })
}

fn has_fastboot_device() -> bool {
    let output = std::process::Command::new("fastboot")
        .arg("devices")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .lines()
        .any(|line| !line.trim().is_empty())
}

fn check_if_devices_are_connected() -> bool {
    has_adb_device() || has_fastboot_device()
}

fn create_new_project(name: &str, path: Option<PathBuf>) {
    let base_path = match path {
        Some(p) => {
            if !p.exists() {
                eprintln!("Invalid path: {}", p.display());
                std::process::exit(1);
            }
            p.join(name)
        }
        None => PathBuf::from(name),
    };

    println!("Creating new project '{}' at '{}'", name, base_path.display());
    fs::create_dir_all(&base_path).unwrap();

    // -----------------------------
    // module.prop
    // -----------------------------
    let mut module_prop = fs::File::create(base_path.join("module.prop")).unwrap();
    writeln!(
        module_prop,
        "id={}\nname=Put module name here\nversion=1.0\nversionCode=1\nauthor=Your name\ndescription=Put module description here\nminMagisk=26000",
        name
    ).unwrap();
    println!("Creating module.prop");

    // Helper for script creation
    let write_script = |file: &mut fs::File| {
        writeln!(file, "#!/system/bin/sh").unwrap();
    };

    // customize.sh
    let mut customize = fs::File::create(base_path.join("customize.sh")).unwrap();
    write_script(&mut customize);
    println!("Creating customize.sh");

    // post-fs-data.sh
    let mut post = fs::File::create(base_path.join("post-fs-data.sh")).unwrap();
    write_script(&mut post);
    println!("Creating post-fs-data.sh");

    // service.sh
    let mut service = fs::File::create(base_path.join("service.sh")).unwrap();
    write_script(&mut service);
    println!("Creating service.sh");

    // uninstall.sh
    let mut uninstall = fs::File::create(base_path.join("uninstall.sh")).unwrap();
    write_script(&mut uninstall);
    println!("Creating uninstall.sh");

    // sepolicy.rule
    fs::File::create(base_path.join("sepolicy.rule")).unwrap();
    println!("Creating sepolicy.rule");

    // system.prop
    fs::File::create(base_path.join("system.prop")).unwrap();
    println!("Creating system.prop");

    // Directories inside module
    println!("Creating directories");
    for dir in ["post-fs-data.d", "service.d", "system", "vendor", "product", "system_ext"] {
        fs::create_dir_all(base_path.join(dir)).unwrap();
    }

    println!("Done!");
}

fn zip_folder(src_dir: &Path, dst_dir: &Path, zip_name: &str) {
    let file = File::create(&format!("{}/{}", dst_dir.to_string_lossy(), zip_name)).expect("Failed to create ZIP");
    let mut zip = ZipWriter::new(file);

    let options: FileOptions<'_, ExtendedFileOptions> = FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o755);

    
    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let rel_path = path.strip_prefix(src_dir).unwrap();
        let rel = rel_path.to_string_lossy().replace("\\", "/");

        if path.is_dir() {
            let dir_name = format!("{}/", rel);
            zip.add_directory(dir_name, options.clone()).unwrap();
            continue;
        }

        zip.start_file(rel, options.clone()).unwrap();
        let data = std::fs::read(path).unwrap();
        zip.write_all(&data).unwrap();
    }

    zip.finish().expect("Failed to finish ZIP");
}

fn push_project(name: &str, project_path: &Path, no_clear: bool, no_push: bool, no_reboot: bool, ignore_adb: bool) {
    if !check_if_devices_are_connected() {
        eprintln!("No device found.");
        if !ignore_adb {
            return;
        }
    }

    let zip_name = format!("{}.zip", name);

    let module_path = project_path.join(name);
    zip_folder(&module_path, &project_path, &format!("{name}.zip"));

    // Push zip to sdcard
    if no_push {
        return;
    }

    Command::new("adb")
        .args(["push", &zip_name, &format!("/sdcard/{}", zip_name)])
        .status()
        .unwrap();

    // Create dir in modules_update
    Command::new("adb")
        .args([
            "shell",
            "su",
            "-c",
            &format!("mkdir -p /data/adb/modules_update/{}", name),
        ])
        .status()
        .unwrap();

    // Unzip to the module directory
    Command::new("adb")
        .args([
            "shell",
            "su",
            "-c",
            &format!(
                "unzip -o /sdcard/{} -d /data/adb/modules_update/{}",
                zip_name, name
            ),
        ])
        .status()
        .unwrap();

    // Create update flag
    Command::new("adb")
        .args([
            "shell",
            "su",
            "-c",
            &format!("touch /data/adb/modules_update/{}/update", name),
        ])
        .status()
        .unwrap();

    // Reboot
    if no_reboot {
        return;
    }
    Command::new("adb")
        .arg("reboot")
        .status()
        .unwrap();

    if !no_clear {
        let _ = fs::remove_file(&format!("{}\\{}.zip", project_path.to_string_lossy(), name));
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name, path } => {
            create_new_project(&name, path);
        }
        Commands::Build { name, path, no_clear, no_push, no_reboot, ignore_adb } => {
            let base_path = match path {
                Some(p) => {
                    if !p.exists() {
                        eprintln!("Invalid path: {}", p.display());
                        std::process::exit(1);
                    }
                    p
                }
                None => PathBuf::from(name.clone()),
            };

            push_project(&name, &base_path, no_clear, no_push, no_reboot, ignore_adb);
        }
        Commands::ListDirectories { } => {
            if !check_if_devices_are_connected() {
                eprintln!("There are no devices connected!");
            }
            println!("Listing directories...");

            let tree = directories::list_directories();
            tree.print(0);
        }
        Commands::GetAndroidTree { directory_name } => {
            if !check_if_devices_are_connected() {
                eprintln!("There are no devices connected!");
            }
            println!("Printing android tree...");

            let tree = directories::get_android_tree();
            if let Some(node) = tree.get(&directory_name.unwrap_or("".to_string())) {
                node.print(2);
            } 
            else {
                tree.print(0);
            }
        }
    }
}
