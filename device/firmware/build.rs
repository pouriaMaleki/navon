fn main() {
    if std::env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(std::ffi::OsStr::new("espidf")) {
        embuild::espidf::sysenv::output();
    }
    // Local component sources live outside the cargo crate so cargo's default
    // change-tracking misses them. Touching them must rerun esp-idf-sys's
    // build.rs (which invokes CMake/Ninja); without these directives an edit
    // to e.g. components/hosted_ble/hosted_ble.c silently links against the
    // stale .obj from a previous build.
    println!("cargo:rerun-if-changed=components");
    println!("cargo:rerun-if-changed=sdkconfig.defaults");
    println!("cargo:rerun-if-changed=partitions.csv");
}
