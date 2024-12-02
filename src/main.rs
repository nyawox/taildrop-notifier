#![feature(io_error_more)]
#![feature(exit_status_error)]
use clap::{Arg, Command as clapCommand};
use nix::unistd::chown;
use nix::unistd::{Gid, Uid, setresgid, setresuid};
use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecursiveMode, Watcher, recommended_watcher};
use notify_rust::Notification;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::Builder;
use tokio::sync::mpsc;

const SOUND_EFFECT: &[u8] = include_bytes!("../assets/sounds/taildrop_notify.wav");

fn run_as_user<F>(username: &str, action: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
    let uid = Command::new("id").args(["-u", username]).output()?.stdout;
    let uid = String::from_utf8(uid)?.trim().parse::<u32>()?;
    let gid = Command::new("id").args(["-g", username]).output()?.stdout;
    let gid = String::from_utf8(gid)?.trim().parse::<u32>()?;
    let original_uid = Uid::current();
    let original_gid = Gid::current();
    setresgid(original_gid, Gid::from_raw(gid), original_gid)?;
    setresuid(original_uid, Uid::from_raw(uid), original_uid)?;
    let result = action();
    setresgid(original_gid, original_gid, original_gid)?;
    setresuid(original_uid, original_uid, original_uid)?;
    result
}

async fn play_sound(username: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_as_user(username, || {
        let uid_output = Command::new("id")
            .args(["-u", username])
            .output()
            .expect("Failed to fetch user ID");
        let uid = String::from_utf8_lossy(&uid_output.stdout)
            .trim()
            .to_string();
        let xdg_runtime_dir = format!("/run/user/{}", uid);

        // Create a temporary file for the audio file
        let mut temp_file = Builder::new()
            .suffix(".wav")
            .tempfile()
            .expect("Failed to create temporary file");

        temp_file
            .write_all(SOUND_EFFECT)
            .expect("Failed to write WAV data to temporary file");
        let temp_file_path = temp_file.path().to_string_lossy().into_owned();

        println!("Playing notification sound for user: {}", username);

        // Play the WAV file using pw-play
        Command::new("pw-play")
            .arg(&temp_file_path)
            .arg("--volume")
            .arg("10")
            .env("XDG_RUNTIME_DIR", &xdg_runtime_dir)
            .status()?;

        Ok(())
    })?;

    Ok(())
}

async fn handle_file_event(
    path: &PathBuf,
    username: &str,
    download_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("null")
        .to_string();

    println!("Processing file: {}", &filename);

    let mut user_action = None;
    // Send notification
    run_as_user(username, || {
        println!("Attempting to send notification for file: {}", &filename);
        let response = Notification::new()
            .summary("Taildrop")
            .body(&format!("Receive file: {}", &filename))
            .action("Accept", "Accept")
            .action("Decline", "Decline")
            .icon("network-wireless")
            .urgency(notify_rust::Urgency::Critical)
            .show()?;
        println!("Notification sent, awaiting user response...");
        response.wait_for_action(|action| match action {
            "Accept" => user_action = Some("Accept"),
            "Decline" => user_action = Some("Decline"),
            _ => eprintln!("Invalid action: {}", action),
        });
        Ok(())
    })?;

    match user_action {
        Some("Accept") => {
            println!("User accepted file: {}", &filename);
            if let Err(e) = move_file(path, download_path.join(&filename), username) {
                eprintln!("Failed to receive {}: {}", &filename, e);
            } else {
                println!("File received: {}", &filename);
            }
        }
        // treat dismissing notification the same as declining
        // shouldn't be a problem if we set urgency to critical
        _ => {
            println!("User declined file: {}", &filename);
            if let Err(e) = fs::remove_file(path) {
                eprintln!("Error deleting file {}: {}", &filename, e);
            }
        }
    }
    Ok(())
}

pub async fn taildrop_monitor(
    username: &str,
    download_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, mut rx) = mpsc::channel(32); // Async channel
    let mut watcher = recommended_watcher(move |res| {
        let _ = tx.blocking_send(res);
    })?;
    watcher.watch(
        Path::new("/var/lib/tailscale/files"),
        RecursiveMode::Recursive,
    )?;

    println!("Monitoring started. Waiting for files...");

    while let Some(result) = rx.recv().await {
        match result {
            Ok(event) => {
                println!("Event detected: {:?}", event);

                // prompt the user when the partial file has been renamed to actual file
                // this alone should be enough to handle new files
                if matches!(
                    event.kind,
                    EventKind::Modify(ModifyKind::Name(RenameMode::To))
                ) {
                    for path in event
                        .paths
                        .clone()
                        .into_iter()
                        .filter(|p| !is_partial_file(p))
                    {
                        println!("Handling new file: {:?}", path);

                        let usernama = username.to_string().clone();
                        let usernamy = username.to_string().clone();
                        let download_path = download_path.clone();
                        // Send notification
                        tokio::spawn(async move {
                            if let Err(e) =
                                handle_file_event(&path.clone(), &usernama, &download_path).await
                            {
                                eprintln!("Error handling file event: {}", e);
                            }
                        });
                        // Play notification sound
                        tokio::spawn(async move {
                            if let Err(e) = play_sound(&usernamy).await {
                                eprintln!("Error playing notification sound: {}", e);
                            }
                        });
                    }
                }
            }
            Err(e) => eprintln!("Watch error: {:?}", e),
        }
    }
    Ok(())
}

fn is_partial_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("partial")
}

fn move_file<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q, username: &str) -> io::Result<()> {
    match fs::rename(&from, &to) {
        Ok(_) => {
            change_ownership(&to, username)?;
            Ok(())
        }
        // handle cases where the source and destination are in different devices (partitions)
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            fs::copy(&from, &to)?;
            fs::remove_file(&from)?;
            change_ownership(&to, username)?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn change_ownership<P: AsRef<Path>>(path: P, username: &str) -> io::Result<()> {
    let uid = Command::new("id").args(["-u", username]).output()?.stdout;
    let uid = String::from_utf8(uid).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let uid = uid
        .trim()
        .parse::<u32>()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let gid = Command::new("id").args(["-g", username]).output()?.stdout;
    let gid = String::from_utf8(gid).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let gid = gid
        .trim()
        .parse::<u32>()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let path_ref = path.as_ref();

    chown(path_ref, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid)))
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}

fn map_homedir(path: &String, username: &str) -> PathBuf {
    if let Some(tail) = path.strip_prefix("~/") {
        let home = PathBuf::from("/home/").join(username.to_owned() + "/");
        home.join(tail)
    } else {
        PathBuf::from(path)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if the program is being run as root
    if !Uid::effective().is_root() {
        eprintln!(
            "Error: This program requires root permission to handle received files in /var/lib/tailscale/files."
        );
        eprintln!("Try running again with elevated permissions: `sudo !!`");
        std::process::exit(1);
    }

    // Define the command-line arguments
    let matches = clapCommand::new("taildrop-notifier")
        .version("0.1.0")
        .author("myname")
        .about("Detect arriving taildrop files and prompt with user-friendly notifications")
        .arg(
            Arg::new("user")
                .long("user")
                .short('u')
                .value_name("USER")
                .help("Set the user. Required to change the file ownership, send notification, and interact with pipewire")
                .required(true),
        )
        .arg(
            Arg::new("path")
                .long("path")
                .short('p')
                .value_name("PATH")
                .help("Set the download path for received files")
                .default_value("~/Downloads"),
        )
        .get_matches();

    // Get values from the parsed arguments
    let username = matches.get_one::<String>("user").unwrap(); // we can safely unwrap this since we specified the argument as required
    let download_path = matches
        .get_one::<String>("path")
        .map(|path| map_homedir(path, username))
        .expect("Download path defaults to ~/Downloads");

    // Ensure the download path exists
    tokio::fs::create_dir_all(&download_path).await?;

    println!("User: {}", username);
    println!("Path: {:?}", download_path);

    // Pass arguments to the monitor function
    taildrop_monitor(username, download_path).await
}
