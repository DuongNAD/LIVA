fn main() {
    println!("Testing rodio::OutputStream::try_default()...");
    match rodio::OutputStream::try_default() {
        Ok(_) => println!("Success! Audio output stream initialized."),
        Err(e) => println!(
            "Error: Failed to initialize default audio output stream: {}",
            e
        ),
    }
}
