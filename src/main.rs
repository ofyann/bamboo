pub mod cli;
pub mod error;
pub mod image;
mod auth;
mod registry;
mod sync;
mod logging;

fn main() {
    println!("bamboo initialized");
}
