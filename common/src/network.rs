//! Length-prefixed framing used on every TCP/TLS connection.
//!
//! Wire format: `[4-byte big-endian length][payload]`, payload is usually JSON.

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// INFO: Hard cap per message. 10 MB is plenty for control messages; note that
//       actual source archives are downloaded by the daemon, not through this path.
const MAX_MSG_SIZE: usize = 10 * 1024 * 1024;

pub async fn send_msg<S>(stream: &mut S, data: &[u8]) -> Result<()>
where
    S: AsyncWriteExt + Unpin,
{
    let len = data.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(data).await?;
    Ok(())
}

pub async fn recv_msg<S>(stream: &mut S) -> Result<Vec<u8>>
where
    S: AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    // INFO: Reject oversized frames before allocating, so a bad peer cannot ask
    //       us for an arbitrarily large buffer.
    if len > MAX_MSG_SIZE {
        anyhow::bail!("Message too large: {} bytes (max {})", len, MAX_MSG_SIZE);
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

pub async fn send_json<S, T>(stream: &mut S, data: &T) -> Result<()>
where
    S: AsyncWriteExt + Unpin,
    T: serde::Serialize,
{
    let json = serde_json::to_vec(data)?;
    send_msg(stream, &json).await
}

pub async fn recv_json<S, T>(stream: &mut S) -> Result<T>
where
    S: AsyncReadExt + Unpin,
    T: serde::de::DeserializeOwned,
{
    let data = recv_msg(stream).await?;
    Ok(serde_json::from_slice(&data)?)
}
