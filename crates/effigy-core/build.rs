fn main() {
    let profile = std::env::var("PROFILE").expect("Cargo sets PROFILE for build scripts");
    println!("cargo:rustc-env=EFFIGY_CARGO_PROFILE={profile}");
    println!("cargo:rerun-if-env-changed=PROFILE");
}
