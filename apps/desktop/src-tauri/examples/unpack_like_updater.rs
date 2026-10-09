//! Release check for `scripts/build-update-artifact.sh`: unpacks an updater
//! package the way tauri-plugin-updater 2.12's macOS install does, so the
//! signature and Gatekeeper checks run on the tree an update would install.
//!
//! The plugin reads the gzip tarball with the `flate2` and `tar` crates, drops
//! the first path component of every entry (`Yawn.app/`), and unpacks into a
//! fresh temporary directory, which it then renames into place as the app
//! bundle. That directory starts at mode 0700, as a temp dir does; the
//! archive's own top-level entry then sets it from the packaged bundle (755
//! on a 2026-10-09 test). This does the same, so the result is what lands in
//! /Applications. macOS `tar` is a different implementation and could unpack
//! a symlink, hard link or long path differently.
//!
//! Usage: unpack_like_updater <package.app.tar.gz> <new-directory>

use std::fs::{self, File};
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use flate2::read::GzDecoder;

fn unpack(package: &Path, destination: &Path) -> std::io::Result<usize> {
    fs::create_dir(destination)?;
    fs::set_permissions(destination, fs::Permissions::from_mode(0o700))?;
    let mut archive = tar::Archive::new(GzDecoder::new(BufReader::new(File::open(package)?)));
    let mut count = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let collected: PathBuf = entry.path()?.iter().skip(1).collect();
        let target = destination.join(&collected);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
        count += 1;
    }
    Ok(count)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [package, destination] = args.as_slice() else {
        eprintln!("usage: unpack_like_updater <package.app.tar.gz> <new-directory>");
        return ExitCode::from(2);
    };
    match unpack(Path::new(package), Path::new(destination)) {
        Ok(count) => {
            println!("unpacked like the updater: {count} entries into {destination}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("unpack like the updater: FAIL — {error}");
            ExitCode::FAILURE
        }
    }
}
