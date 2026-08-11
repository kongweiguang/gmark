// @author kongweiguang

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=resources/windows/gmark.rc");
    println!("cargo:rerun-if-changed=resources/windows/gmark-update-agent.rc");
    println!("cargo:rerun-if-changed=assets/icon/gmark.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bin=gmark=/STACK:8388608");
        embed_resource::compile_for(
            "resources/windows/gmark.rc",
            ["gmark"],
            embed_resource::NONE,
        )
        .manifest_optional()
        .expect("failed to compile gmark Windows resources");
        println!("cargo:rustc-link-arg-bin=gmark-update-agent=/STACK:8388608");
        embed_resource::compile_for(
            "resources/windows/gmark-update-agent.rc",
            ["gmark-update-agent"],
            embed_resource::NONE,
        )
        .manifest_optional()
        .expect("failed to compile gmark update agent Windows resources");
    }
}
