// build.rs — Download and compile Redis from source at build time
//
// This runs BEFORE `cargo build`. It:
//   1. Checks if Redis is already compiled in target/redis/
//   2. If not, downloads Redis 7.4.2 source tarball
//   3. Compiles it with `make` (takes ~30-40 seconds first time)
//   4. Tells Cargo where to find the binary via environment variable
//
// Subsequent builds skip the download+compile (already cached).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const REDIS_VERSION: &str = "7.4.2";
const REDIS_URL: &str = "https://github.com/redis/redis/archive/refs/tags/7.4.2.tar.gz";

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let redis_dir = out_dir.join("redis");
    let redis_bin = redis_dir.join("src").join("redis-server");

    // Skip if already compiled
    if redis_bin.exists() {
        println!("cargo:rustc-env=REDIS_SERVER_PATH={}", redis_bin.display());
        println!("cargo:warning=Redis {} already compiled", REDIS_VERSION);
        return;
    }

    println!("cargo:warning=Downloading Redis {}...", REDIS_VERSION);

    // Download
    let tarball = out_dir.join("redis.tar.gz");
    let status = Command::new("curl")
        .args(["-sL", REDIS_URL, "-o"])
        .arg(&tarball)
        .status()
        .expect("failed to run curl — is curl installed?");
    assert!(status.success(), "failed to download Redis");

    // Extract
    let status = Command::new("tar")
        .args(["xzf"])
        .arg(&tarball)
        .arg("-C")
        .arg(&out_dir)
        .status()
        .expect("failed to extract tarball");
    assert!(status.success(), "failed to extract Redis");

    // Rename extracted dir
    let extracted = out_dir.join(format!("redis-{}", REDIS_VERSION));
    if redis_dir.exists() {
        fs::remove_dir_all(&redis_dir).ok();
    }
    fs::rename(&extracted, &redis_dir).expect("failed to rename redis dir");

    // Compile
    println!(
        "cargo:warning=Compiling Redis {} (this takes ~30 seconds first time)...",
        REDIS_VERSION
    );
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get().to_string())
        .unwrap_or_else(|_| "4".to_string());

    let status = Command::new("make")
        .arg(format!("-j{}", num_cpus))
        .arg("redis-server")
        .current_dir(&redis_dir)
        .status()
        .expect("failed to run make — is gcc/make installed?");
    assert!(status.success(), "failed to compile Redis");

    // Ensure the binary is executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&redis_bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&redis_bin, perms).unwrap();
    }

    // Clean up tarball
    fs::remove_file(&tarball).ok();

    println!(
        "cargo:warning=Redis {} compiled successfully!",
        REDIS_VERSION
    );
    println!("cargo:rustc-env=REDIS_SERVER_PATH={}", redis_bin.display());
}
