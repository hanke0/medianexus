use std::path::PathBuf;

fn main() {
    // When embedding the SPA, ensure the Vite output directory exists so
    // `rust-embed` can compile even before the first frontend build.
    if std::env::var_os("CARGO_FEATURE_EMBED_FRONTEND").is_some() {
        let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/dist");
        if !dist.exists() {
            let _ = std::fs::create_dir_all(&dist);
            let stub = dist.join("index.html");
            if !stub.exists() {
                let _ = std::fs::write(
                    stub,
                    b"<!doctype html><html><body>Run <code>npm run build:frontend</code> then rebuild.</body></html>\n",
                );
            }
        }
        println!("cargo:rerun-if-changed=../frontend/dist");
    }
}
