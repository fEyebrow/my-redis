use tokio::net::TcpStream;
use tokio::io::BufWriter;
use bytes::{ BytesMut, Buf };
use mini_redis::{Frame::{ Error::{ Incomplete } }, Result};
use std::io::Cursor;

struct Connection {
  stream: BufWriter(TcpStream),
  buffer: BytesMut,
}

impl Connection {
  pub fn new(stream: TcpStream) -> Connection {
    Connection {
      stream: BufWriter::new(stream),
      buffer: BytesMut::with_capacity(4096),
    }
  }

  pub async fn read_frame(&mut self) -> Result<Option<Frame>> {
    loop {
      if let Some(frame) = self.parse_frame()? {
        return Ok(Some(frame));
      }

      if 0 == self.stream.read_buf(&mut self.buffer).await? {
        if self.buffer.is_empty() {
          return Ok(None)
        } else {
          return Err("connection reset by peer".into())
        }
      }
    }
  }

  fn parse_frame(&mut self) -> Result<Option<Frame>> {
    let mut buf = Cursor::new(&self.buffer[..]);
    match Frame::check(&mut buf) {
      OK(_) => {
        let len = buf.position() as usize;

        buf.set_position(0);

        let frame = Frame::parse(&mut buf)?;
        self.buff.advance(len);
        Ok(Some(frame))
      },
      Err(Incomplete) => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  pub async fn write_frame(&mut self, frame: &Frame) -> Result<()> {
    match frame {
      Frame::Simple(val) => {
          self.stream.write_u8(b'+').await?;
          self.stream.write_all(val.as_bytes()).await?;
          self.stream.write_all(b"\r\n").await?;
      }
      Frame::Error(val) => {
          self.stream.write_u8(b'-').await?;
          self.stream.write_all(val.as_bytes()).await?;
          self.stream.write_all(b"\r\n").await?;
      }
      Frame::Integer(val) => {
          self.stream.write_u8(b':').await?;
          self.write_decimal(*val).await?;
      }
      Frame::Null => {
          self.stream.write_all(b"$-1\r\n").await?;
      }
      Frame::Bulk(val) => {
          let len = val.len();

          self.stream.write_u8(b'$').await?;
          self.write_decimal(len as u64).await?;
          self.stream.write_all(val).await?;
          self.stream.write_all(b"\r\n").await?;
      }
      Frame::Array(_val) => unimplemented!(),
    }

    self.stream.flush().await;

    Ok(())
  }

  async fn write_decimal(&mut self, val: u64) -> io::Result<()> {
    use std::io::Write;

    // Convert the value to a string
    let mut buf = [0u8; 12];
    let mut buf = Cursor::new(&mut buf[..]);
    write!(&mut buf, "{}", val)?;

    let pos = buf.position() as usize;
    self.stream.write_all(&buf.get_ref()[..pos]).await?;
    self.stream.write_all(b"\r\n").await?;

    Ok(())
  }
}