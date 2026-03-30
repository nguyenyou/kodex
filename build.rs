fn main() {
    println!("cargo:rerun-if-changed=proto/semanticdb.proto");
    prost_build::Config::new()
        .compile_protos(&["proto/semanticdb.proto"], &["proto/"])
        .expect("Failed to compile semanticdb.proto");
}
