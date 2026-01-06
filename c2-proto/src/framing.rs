use crate::{Message, ProtocolError, Result};
use bytes::{Buf, BufMut, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, warn};

/// Maximum frame size (10MB for safety)
pub const MAX_FRAME_SIZE: u32 = 10 * 1024 * 1024;

/// Default maximum frame size for most deployments (1MB)
pub const DEFAULT_MAX_FRAME_SIZE: u32 = 1024 * 1024;

/// Reads a length-prefixed JSON frame from an async reader
pub async fn read_frame<R>(
    reader: &mut R,
    max_frame_size: u32,
) -> Result<Message>
where
    R: AsyncRead + Unpin,
{
    // Read 4-byte big-endian length prefix
    let length = reader.read_u32().await?;

    if length > max_frame_size {
        warn!("Received oversized frame: {} bytes (max: {})", length, max_frame_size);
        return Err(ProtocolError::FrameTooLarge(length, max_frame_size));
    }

    debug!("Reading frame of {} bytes", length);

    // Read the payload
    let mut payload = vec![0u8; length as usize];
    reader.read_exact(&mut payload).await?;

    // Deserialize JSON
    let message: Message = serde_json::from_slice(&payload)?;
    debug!("Received message type: {}", message.msg_type);

    Ok(message)
}

/// Writes a length-prefixed JSON frame to an async writer
pub async fn write_frame<W>(writer: &mut W, message: &Message) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Serialize to JSON
    let payload = serde_json::to_vec(message)?;
    let length = payload.len() as u32;

    debug!("Writing frame of {} bytes, type: {}", length, message.msg_type);

    // Write length prefix (4 bytes, big-endian)
    writer.write_u32(length).await?;

    // Write payload
    writer.write_all(&payload).await?;

    // Flush to ensure data is sent
    writer.flush().await?;

    Ok(())
}

/// Codec for use with tokio_util::codec::Framed
pub struct JsonFrameCodec {
    max_frame_size: u32,
}

impl JsonFrameCodec {
    pub fn new(max_frame_size: u32) -> Self {
        Self { max_frame_size }
    }
}

impl Default for JsonFrameCodec {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_FRAME_SIZE)
    }
}

impl tokio_util::codec::Decoder for JsonFrameCodec {
    type Item = Message;
    type Error = ProtocolError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        // Need at least 4 bytes for length prefix
        if src.len() < 4 {
            return Ok(None);
        }

        // Peek at length without consuming
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_be_bytes(length_bytes);

        if length > self.max_frame_size {
            warn!("Received oversized frame: {} bytes (max: {})", length, self.max_frame_size);
            return Err(ProtocolError::FrameTooLarge(length, self.max_frame_size));
        }

        // Check if we have the full frame
        let frame_size = 4 + length as usize;
        if src.len() < frame_size {
            // Not enough data yet, reserve space
            src.reserve(frame_size - src.len());
            return Ok(None);
        }

        // We have a complete frame, consume it
        src.advance(4); // Skip length prefix
        let payload = src.split_to(length as usize);

        // Deserialize JSON
        let message: Message = serde_json::from_slice(&payload)?;
        debug!("Decoded message type: {}", message.msg_type);

        Ok(Some(message))
    }
}

impl tokio_util::codec::Encoder<Message> for JsonFrameCodec {
    type Error = ProtocolError;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<()> {
        // Serialize to JSON
        let payload = serde_json::to_vec(&item)?;
        let length = payload.len() as u32;

        debug!("Encoding frame of {} bytes, type: {}", length, item.msg_type);

        // Reserve space
        dst.reserve(4 + payload.len());

        // Write length prefix
        dst.put_u32(length);

        // Write payload
        dst.put_slice(&payload);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::*;

    #[tokio::test]
    async fn test_frame_roundtrip() {
        let message = Message::hello("test-client".to_string());

        let mut buffer = Vec::new();
        write_frame(&mut buffer, &message).await.unwrap();

        let mut cursor = std::io::Cursor::new(buffer);
        let decoded = read_frame(&mut cursor, DEFAULT_MAX_FRAME_SIZE).await.unwrap();

        assert_eq!(message.msg_type, decoded.msg_type);
    }

    #[tokio::test]
    async fn test_oversized_frame_rejected() {
        let mut buffer = Vec::new();

        // Write a length that exceeds max
        buffer.extend_from_slice(&(DEFAULT_MAX_FRAME_SIZE + 1).to_be_bytes());

        let mut cursor = std::io::Cursor::new(buffer);
        let result = read_frame(&mut cursor, DEFAULT_MAX_FRAME_SIZE).await;

        assert!(matches!(result, Err(ProtocolError::FrameTooLarge(_, _))));
    }

    #[test]
    fn test_codec_decode_incomplete() {
        use tokio_util::codec::Decoder;

        let mut codec = JsonFrameCodec::default();
        let mut buf = BytesMut::new();

        // Only 2 bytes, need 4 for length
        buf.extend_from_slice(&[0, 0]);

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_none());
    }
}
