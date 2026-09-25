//! GUI-less OpenSSH askpass helper; the containing Desktop owns the prompt UI.
#![cfg_attr(windows, windows_subsystem = "windows")]
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};
use zeroize::Zeroizing;
fn main() {
    if let Err(stage) = run() {
        eprintln!("SSH credential prompt failed: {stage}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), &'static str> {
    let address: std::net::SocketAddr = std::env::var("BUZZ_ASKPASS_ADDRESS")
        .map_err(|_| "missing endpoint")?
        .parse()
        .map_err(|_| "invalid endpoint")?;
    if !address.ip().is_loopback() {
        return Err("non-loopback endpoint");
    }
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(2)).map_err(|_| "connect")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| "read timeout setup")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| "write timeout setup")?;
    let request = buzz_connections::prompt::PromptRequest {
        token: std::env::var("BUZZ_ASKPASS_TOKEN").map_err(|_| "missing capability")?,
        prompt: std::env::args().nth(1).ok_or("missing question")?,
    };
    let input = Zeroizing::new(serde_json::to_vec(&request).map_err(|_| "encode request")?);
    if input.len() > 8192 {
        return Err("question too large");
    }
    stream.write_all(&input).map_err(|_| "write request")?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|_| "finish request")?;
    let mut reply = Zeroizing::new(Vec::new());
    stream
        .take(4097)
        .read_to_end(&mut reply)
        .map_err(|_| "read response")?;
    if reply.len() > 4096 {
        return Err("response too large");
    }
    std::io::stdout()
        .write_all(&reply)
        .map_err(|_| "write answer")?;
    std::io::stdout()
        .write_all(b"\n")
        .map_err(|_| "finish answer")?;
    Ok(())
}
