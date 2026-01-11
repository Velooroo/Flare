use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// Protocol: [4 byte length][data]

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
