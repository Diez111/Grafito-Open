use std::env;
use camino::Utf8PathBuf;
use uniffi_bindgen::bindings::{generate, GenerateOptions, TargetLanguage};

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 3 {
        eprintln!("Usage: {} <library-path> <output-dir>", args[0]);
        std::process::exit(1);
    }
    
    let library_path = Utf8PathBuf::from(&args[1]);
    let out_dir = Utf8PathBuf::from(&args[2]);
    
    if !library_path.exists() {
        eprintln!("Error: Library file not found: {}", library_path);
        std::process::exit(1);
    }
    
    let options = GenerateOptions {
        languages: vec![TargetLanguage::Kotlin],
        source: library_path,
        out_dir,
        config_override: None,
        format: true,
        crate_filter: None,
        metadata_no_deps: false,
    };
    
    match generate(options) {
        Ok(_) => println!("✓ Kotlin bindings generated successfully"),
        Err(e) => {
            eprintln!("Error generating bindings: {}", e);
            std::process::exit(1);
        }
    }
}
