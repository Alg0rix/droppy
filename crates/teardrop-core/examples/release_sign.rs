//! Release-signing tool.
//!
//!   keygen <secret-file>   generate a signing seed, write it to <secret-file>
//!                          (refuses to overwrite), print the public key
//!   sign <file>            sign <file> with the seed in $DROPPY_SIGNING_KEY
//!                          and write the signature hex to <file>.sig

use ed25519_dalek::SigningKey;
use rand::RngCore;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, arg) = (args.get(1).map(String::as_str), args.get(2).map(String::as_str));
    match (cmd, arg) {
        (Some("keygen"), Some(file)) => keygen(file),
        (Some("sign"), Some(file)) => sign(file),
        _ => {
            eprintln!("usage: release_sign keygen <secret-file> | sign <file>");
            exit(2);
        }
    }
}

fn keygen(file: &str) {
    let path = std::path::Path::new(file);
    if path.exists() {
        eprintln!("{file} already exists; refusing to overwrite a secret");
        exit(1);
    }
    let mut seed = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut seed);
    let key = SigningKey::from_bytes(&seed);
    let seed_hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
    let pub_hex: String = key.verifying_key().to_bytes().iter().map(|b| format!("{b:02x}")).collect();
    if let Err(e) = std::fs::write(path, seed_hex) {
        eprintln!("could not write {file}: {e}");
        exit(1);
    }
    println!("{pub_hex}");
    println!("put this in update::UPDATE_PUBKEY; store the file contents as the DROPPY_SIGNING_KEY GitHub secret, then delete the file");
}

fn sign(file: &str) {
    let Ok(seed) = std::env::var("DROPPY_SIGNING_KEY") else {
        eprintln!("DROPPY_SIGNING_KEY is not set");
        exit(1);
    };
    let data = match std::fs::read(file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("could not read {file}: {e}");
            exit(1);
        }
    };
    let Some(sig) = teardrop_core::release::sign(&seed, &data) else {
        eprintln!("DROPPY_SIGNING_KEY is not a 64-hex-char ed25519 seed");
        exit(1);
    };
    if let Err(e) = std::fs::write(format!("{file}.sig"), format!("{sig}\n")) {
        eprintln!("could not write {file}.sig: {e}");
        exit(1);
    }
}
