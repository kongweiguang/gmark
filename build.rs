// @author kongweiguang

fn main() {
    println!("cargo:rerun-if-changed=resources/windows/gmark.rc");
    println!("cargo:rerun-if-changed=assets/icon/gmark.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("resources/windows/gmark.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to compile gmark Windows resources");
    }
}
