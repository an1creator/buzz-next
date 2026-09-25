#![cfg(unix)]
use buzz_remote_host::codex_connection::bridge;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn existing_socket_preserves_rpc_and_does_not_forward_auth_mutations() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("existing.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut peer = tokio_tungstenite::accept_async(stream).await.unwrap();
        let request = peer.next().await.unwrap().unwrap().into_text().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request).unwrap()["method"],
            "model/list"
        );
        peer.send(Message::Text(
            "{\n\"id\":2,\"result\":{\"data\":[]}}".into(),
        ))
        .await
        .unwrap();
        assert!(matches!(
            peer.next().await,
            Some(Ok(Message::Close(_))) | None
        ));
    });
    let (mut client_in, input) = tokio::io::duplex(1024);
    let (output, client_out) = tokio::io::duplex(1024);
    let transport = tokio::spawn(async move { bridge(&socket, input, output).await });
    let mut lines = BufReader::new(client_out).lines();
    client_in
        .write_all(b"{\"id\":1,\"method\":\"account/logout\"}\n")
        .await
        .unwrap();
    let denial: serde_json::Value =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(denial["id"], 1);
    assert_eq!(denial["error"]["code"], -32601);
    client_in
        .write_all(b"{\"id\":2,\"method\":\"model/list\"}\n")
        .await
        .unwrap();
    let result: serde_json::Value =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(result["result"]["data"], serde_json::json!([]));
    client_in.shutdown().await.unwrap();
    assert!(transport.await.unwrap().is_ok());
    server.await.unwrap();
}

#[tokio::test]
async fn missing_socket_fails_without_creating_an_app_server() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("missing.sock");
    let error = bridge(&socket, tokio::io::empty(), tokio::io::sink())
        .await
        .unwrap_err();
    assert_eq!(error, "Shared Codex socket is unavailable");
    assert!(!socket.exists());
}
