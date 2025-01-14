use anyhow::Context;
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncRead, AsyncSeek, AsyncWrite, BufReader},
};

use downcast::{downcast, Any};
use fluent_uri::Uri;

use std::{fmt, io::SeekFrom, path::Path};

use crate::Span;

pub trait WriteSeek: Any + AsyncWrite + AsyncSeek + Unpin + Sync + Send + 'static {}
pub trait Write: Any + AsyncWrite + Unpin + Sync + Send {}

downcast!(dyn WriteSeek);
downcast!(dyn Write);

impl<T> WriteSeek for T where T: AsyncWrite + AsyncSeek + Unpin + Sync + Send + 'static {}
impl<T> Write for T where T: AsyncWrite + Unpin + Sync + Send + 'static {}

pub enum Writer {
    Seekable(Box<dyn WriteSeek>),
    Stream(Box<dyn Write>),
}

pub trait ReadSeek: Any + AsyncRead + AsyncSeek + Unpin + Sync + Send + 'static {}
pub trait Read: Any + AsyncRead + Unpin + Sync + Send {}

downcast!(dyn ReadSeek);
downcast!(dyn Read);

impl<T> ReadSeek for T where T: AsyncRead + AsyncSeek + Unpin + Sync + Send + 'static {}
impl<T> Read for T where T: AsyncRead + Unpin + Sync + Send + 'static {}

pub enum Reader {
    Seekable(BufReader<Box<dyn ReadSeek>>),
    Stream(BufReader<Box<dyn Read>>),
}

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("Stream is not readable")]
    NotReadable,

    #[error("Stream is not writeable")]
    NotWriteable,

    #[error("Stream is not seekable")]
    NotSeekable,

    #[error("Unsupported URI scheme {0:?}")]
    UnsupportedScheme(String),

    #[error("Failed to parse URI: {0}")]
    Uri(#[from] fluent_uri::ParseError),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Misc(#[from] anyhow::Error),
}

pub struct Io {
    uri: Uri<String>,
    writer: Option<Writer>,
    reader: Option<Reader>,
}

fn uri_from_path(path: &Path) -> Result<Uri<String>, IoError> {
    let uri = path
        .to_str()
        .map(|s| s.to_string())
        .unwrap_or(String::new());
    let uri = Uri::parse_from(uri).map_err(|e| e.1)?;

    Ok(uri)
}

impl Io {
    pub fn null() -> Self {
        Io {
            uri: Uri::parse_from(String::new()).unwrap(),
            writer: None,
            reader: None,
        }
    }

    pub async fn open(uri: String) -> Result<Self, IoError> {
        let uri = Uri::parse_from(uri).map_err(|e| e.1)?;

        match uri.scheme().map(|s| s.as_str()) {
            Some("file") | None => {}
            Some(scheme) => {
                return Err(IoError::UnsupportedScheme(scheme.to_string()));
            }
        }

        todo!()
    }

    pub async fn create_file<P: AsRef<Path>>(path: P) -> Result<Self, IoError> {
        let uri = uri_from_path(path.as_ref())?;
        let file = File::create(path).await?;

        Ok(Io {
            uri,
            writer: Some(Writer::Seekable(Box::new(file))),
            reader: None,
        })
    }

    pub async fn open_file<P: AsRef<Path> + fmt::Debug>(path: P) -> Result<Self, IoError> {
        let uri = uri_from_path(path.as_ref())?;
        let file = File::open(&path)
            .await
            .with_context(|| format!("Failed to open file {path:?}"))?;

        Ok(Io {
            uri,
            writer: None,
            reader: Some(Reader::Seekable(BufReader::new(Box::new(file)))),
        })
    }

    pub fn from_stream(writer: Box<dyn Write>) -> Self {
        Io {
            uri: Uri::parse_from(String::new()).unwrap(),
            writer: Some(Writer::Stream(writer)),
            reader: None,
        }
    }

    pub fn from_reader(reader: Box<dyn Read>) -> Self {
        Io {
            uri: Uri::parse_from(String::new()).unwrap(),
            writer: None,
            reader: Some(Reader::Stream(BufReader::new(reader))),
        }
    }

    pub async fn write_span(&mut self, span: Span) -> Result<(), IoError> {
        use tokio::io::AsyncWriteExt;

        let writer = self.writer.as_mut().ok_or(IoError::NotWriteable)?;

        match writer {
            Writer::Seekable(writer) => {
                // TODO: replace with write_vectored
                for span in span.spans() {
                    writer.write_all(span).await?
                }
            }
            Writer::Stream(writer) => {
                // TODO: replace with write_vectored
                for span in span.spans() {
                    writer.write_all(span).await?
                }
            }
        };

        Ok(())
    }

    pub async fn write(&mut self, bytes: &[u8]) -> Result<(), IoError> {
        use tokio::io::AsyncWriteExt;

        let writer = self.writer.as_mut().ok_or(IoError::NotWriteable)?;

        match writer {
            Writer::Seekable(writer) => writer.write_all(bytes).await?,
            Writer::Stream(writer) => writer.write_all(bytes).await?,
        }

        Ok(())
    }

    pub fn reader(&mut self) -> Result<&mut (dyn AsyncRead + Unpin + Send + Sync), IoError> {
        let reader = self.reader.as_mut().ok_or(IoError::NotWriteable)?;

        match reader {
            Reader::Seekable(reader) => Ok(reader),
            Reader::Stream(reader) => Ok(reader),
        }
    }

    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), IoError> {
        use tokio::io::AsyncReadExt;

        let reader = self.reader.as_mut().ok_or(IoError::NotWriteable)?;

        match reader {
            Reader::Seekable(reader) => reader.read_exact(buf).await?,
            Reader::Stream(reader) => reader.read_exact(buf).await?,
        };

        Ok(())
    }

    pub async fn read_probe(&mut self) -> Result<&[u8], IoError> {
        let reader = self.reader.as_mut().ok_or(IoError::NotWriteable)?;

        let inner_bytes = match reader {
            Reader::Seekable(reader) => reader.fill_buf().await?,
            Reader::Stream(reader) => reader.fill_buf().await?,
        };

        Ok(inner_bytes)
    }

    pub async fn skip(&mut self, amt: u64) -> Result<(), IoError> {
        use tokio::io::{self, AsyncReadExt, AsyncSeekExt};

        let reader = self.reader.as_mut().ok_or(IoError::NotWriteable)?;

        match reader {
            Reader::Seekable(reader) => reader.seek(SeekFrom::Current(amt as i64)).await?,
            Reader::Stream(reader) => io::copy(&mut reader.take(amt), &mut io::sink()).await?,
        };

        Ok(())
    }

    pub async fn seek(&mut self, pos: SeekFrom) -> Result<u64, IoError> {
        use tokio::io::AsyncSeekExt;

        let writer = self.writer.as_mut().ok_or(IoError::NotWriteable)?;

        let pos = match writer {
            Writer::Seekable(writer) => writer.seek(pos).await?,
            _ => return Err(IoError::NotSeekable)?,
        };

        Ok(pos)
    }

    pub fn seekable(&self) -> bool {
        matches!(self.writer, Some(Writer::Seekable(_)))
    }

    pub fn into_writer<T: 'static>(&mut self) -> Result<Box<T>, IoError> {
        let writer = self.writer.take().ok_or(IoError::NotWriteable)?;

        let writer = match writer {
            Writer::Seekable(writer) => writer
                .downcast::<T>()
                .expect("Invalid write type requested"),
            Writer::Stream(writer) => writer
                .downcast::<T>()
                .expect("Invalid write type requested"),
        };

        Ok(writer)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    #[tokio::test]
    async fn test() {
        let uri = Uri::parse_from(String::from("../test/foo")).unwrap();
        panic!("{:?}", uri);
    }

    #[test_case(&[b"abc"], b"abc")]
    #[test_case(&[b"a", b"b", b"c"], b"abc")]
    #[tokio::test]
    async fn io_write(spans: &[&[u8]], expected: &[u8]) {
        let buf: Vec<u8> = Vec::new();

        let mut io = Io::from_stream(Box::new(buf));
        for span in spans {
            io.write(span).await.unwrap();
        }

        let buf: Box<Vec<u8>> = io.into_writer().unwrap();

        assert_eq!(expected, *buf);
    }
}
