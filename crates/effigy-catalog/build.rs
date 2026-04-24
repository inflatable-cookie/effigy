fn main() {
    println!("cargo:rerun-if-changed=catalog");
    println!("cargo:rerun-if-changed=starters");
}
