//! GUI-less OpenSSH askpass helper; the containing Desktop owns the prompt UI.
#![cfg_attr(windows, windows_subsystem = "windows")]
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};
use zeroize::Zeroizing;
fn main() {
    if run().is_err() {
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let address: std::net::SocketAddr = std::env::var("BUZZ_ASKPASS_ADDRESS")?.parse()?;
    if !address.ip().is_loopback() {
        return Err("Invalid prompt address".into());
    }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = buzz_connections::prompt::PromptRequest {
        token: std::env::var("BUZZ_ASKPASS_TOKEN")?,
        prompt: std::env::args().nth(1).ok_or("Missing prompt")?,
    };
    let input = Zeroizing::new(serde_json::to_vec(&request)?);
    if input.len() > 8192 {
        return Err("Prompt too large".into());
    }
    stream.write_all(&input)?;
    stream.shutdown(Shutdown::Write)?;
    let mut reply = Zeroizing::new(Vec::new());
    stream.take(4097).read_to_end(&mut reply)?;
    if reply.len() > 4096 {
        return Err("Response too large".into());
    }
    std::io::stdout().write_all(&reply)?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}
