// Re-export the library for use by other crates
pub use client::*;

fn main() {
    eprintln!("This is a library crate. Use it as a dependency in your project.");
    eprintln!("Example:");
    eprintln!("  [dependencies]");
    eprintln!("  client = {{ path = \"../client\" }}");
}
