use crate::error::{CoreError, Result};
use crate::ipc::protocol::{IpcRequest, IpcResponse};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

pub const MAX_MESSAGE_SIZE: u32 = 64 * 1024 * 1024; // 64 MB guard

pub async fn write_message<W: AsyncWriteExt + Unpin, T: Serialize>(
    writer: &mut W,
    message: &T,
) -> Result<()> {
    let payload = serde_json::to_vec(message)?;
    let length = payload.len() as u32;

    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_message<R: AsyncReadExt + Unpin, T: DeserializeOwned>(
    reader: &mut R,
) -> Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let length = u32::from_be_bytes(len_buf);

    if length > MAX_MESSAGE_SIZE {
        return Err(CoreError::Ipc(format!(
            "Message length {} exceeds max payload size {}",
            length, MAX_MESSAGE_SIZE
        )));
    }

    let mut buf = vec![0u8; length as usize];
    reader.read_exact(&mut buf).await?;

    let message: T = serde_json::from_slice(&buf)?;
    Ok(message)
}

pub struct IpcClient;

impl IpcClient {
    pub async fn send_request<P: AsRef<Path>>(
        socket_path: P,
        request: &IpcRequest,
    ) -> Result<IpcResponse> {
        let stream = timeout(
            Duration::from_millis(1500),
            UnixStream::connect(socket_path.as_ref()),
        )
        .await
        .map_err(|_| CoreError::Ipc("Connection timed out".to_string()))?
        .map_err(|e| CoreError::Ipc(format!("Failed to connect to daemon socket: {}", e)))?;

        let (mut reader, mut writer) = stream.into_split();

        write_message(&mut writer, request).await?;
        let response: IpcResponse = read_message(&mut reader).await?;

        Ok(response)
    }
}
