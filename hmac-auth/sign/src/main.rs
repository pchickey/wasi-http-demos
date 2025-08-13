use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Emit the SHA-256 HMAC signature for a given string, provided as the first
/// and only command line argument.
///
/// Uses the SECRET_KEY environment variable as the key to initialize the
/// HMAC. SECRET_KEY must be a hexidecimal value with an even number of
/// digits. If SECRET_KEY is not present, uses the value `12345678`, which is
/// the combination on my luggage.
fn main() {
    let secret_key: String = std::env::var("SECRET_KEY").unwrap_or_else(|_| "12345678".to_string());
    let secret_key = hex::decode(secret_key).expect("secret key should be hex");

    let args = std::env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        panic!("exactly 1 arg allowed");
    }

    let mut mac = Hmac::<Sha256>::new_from_slice(&secret_key).unwrap();
    mac.update(args[1].as_bytes());
    let signature = mac.finalize().into_bytes();

    println!("{}", hex::encode(signature));
}
