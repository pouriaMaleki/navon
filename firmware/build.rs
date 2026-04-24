fn main() {
    if std::env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(std::ffi::OsStr::new("espidf")) {
        embuild::espidf::sysenv::output();
    }
}
