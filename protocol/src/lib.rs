pub const VERSION: u32 = 2;
pub const TOKEN_LENGTH: usize = 32;
pub const SOCKET_VARIABLE: &str = "PROSCENIO_PACKAGES_SOCKET";
pub const TOKEN_VARIABLE: &str = "PROSCENIO_PACKAGES_TOKEN";
const MAX_FRAME_LENGTH: usize = 1 << 20;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated,
    FrameTooLarge(usize),
    FieldTooLarge(usize),
    TooManyArguments(usize),
    UnknownKind(u8),
    Text,
    TrailingBytes(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Upgrade,
    Quit,
    Allow(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Synchronize,
    Prepare,
    Download,
    Install,
    Hooks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Ready {
        version: u32,
        socket: String,
        token: Vec<u8>,
    },
    Stage(Stage),
    Transaction {
        package_count: u32,
        download_bytes: u64,
    },
    Download {
        received_bytes: u64,
        total_bytes: u64,
    },
    Package {
        index: u32,
        count: u32,
        name: String,
        percent: u32,
    },
    Hook {
        index: u32,
        count: u32,
        name: String,
    },
    Line(String),
    Done {
        is_ok: bool,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forward {
    pub token: Vec<u8>,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit(pub i32);

pub trait Message: Sized {
    fn write(&self, writer: &mut Writer);
    fn read(reader: &mut Reader) -> Result<Self, Error>;

    fn encode(&self) -> Vec<u8> {
        let mut body = Writer::default();
        self.write(&mut body);
        let mut frame = (body.bytes.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&body.bytes);
        frame
    }
}

#[derive(Default)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn write_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_bytes(&mut self, value: &[u8]) {
        self.write_u32(value.len() as u32);
        self.bytes.extend_from_slice(value);
    }

    fn write_text(&mut self, value: &str) {
        self.write_bytes(value.as_bytes());
    }
}

pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], Error> {
        if self.bytes.len() < count {
            return Err(Error::Truncated);
        }
        let (head, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(head)
    }

    fn read_u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn read_u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn read_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let length = self.read_u32()? as usize;
        if length > MAX_FRAME_LENGTH {
            return Err(Error::FieldTooLarge(length));
        }
        Ok(self.take(length)?.to_vec())
    }

    fn read_text(&mut self) -> Result<String, Error> {
        String::from_utf8(self.read_bytes()?).map_err(|_| Error::Text)
    }
}

impl Message for Request {
    fn write(&self, writer: &mut Writer) {
        match self {
            Request::Upgrade => writer.write_u8(0),
            Request::Quit => writer.write_u8(1),
            Request::Allow(pid) => {
                writer.write_u8(2);
                writer.write_u32(*pid);
            }
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, Error> {
        match reader.read_u8()? {
            0 => Ok(Request::Upgrade),
            1 => Ok(Request::Quit),
            2 => Ok(Request::Allow(reader.read_u32()?)),
            kind => Err(Error::UnknownKind(kind)),
        }
    }
}

const STAGES: [Stage; 5] = [
    Stage::Synchronize,
    Stage::Prepare,
    Stage::Download,
    Stage::Install,
    Stage::Hooks,
];

impl Message for Event {
    fn write(&self, writer: &mut Writer) {
        match self {
            Event::Ready {
                version,
                socket,
                token,
            } => {
                writer.write_u8(0);
                writer.write_u32(*version);
                writer.write_text(socket);
                writer.write_bytes(token);
            }
            Event::Stage(stage) => {
                writer.write_u8(1);
                writer.write_u8(
                    STAGES
                        .iter()
                        .position(|candidate| candidate == stage)
                        .unwrap() as u8,
                );
            }
            Event::Transaction {
                package_count,
                download_bytes,
            } => {
                writer.write_u8(2);
                writer.write_u32(*package_count);
                writer.write_u64(*download_bytes);
            }
            Event::Download {
                received_bytes,
                total_bytes,
            } => {
                writer.write_u8(3);
                writer.write_u64(*received_bytes);
                writer.write_u64(*total_bytes);
            }
            Event::Package {
                index,
                count,
                name,
                percent,
            } => {
                writer.write_u8(4);
                writer.write_u32(*index);
                writer.write_u32(*count);
                writer.write_text(name);
                writer.write_u32(*percent);
            }
            Event::Hook { index, count, name } => {
                writer.write_u8(5);
                writer.write_u32(*index);
                writer.write_u32(*count);
                writer.write_text(name);
            }
            Event::Line(line) => {
                writer.write_u8(6);
                writer.write_text(line);
            }
            Event::Done { is_ok, message } => {
                writer.write_u8(7);
                writer.write_u8(u8::from(*is_ok));
                writer.write_text(message);
            }
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, Error> {
        Ok(match reader.read_u8()? {
            0 => Event::Ready {
                version: reader.read_u32()?,
                socket: reader.read_text()?,
                token: reader.read_bytes()?,
            },
            1 => {
                let index = reader.read_u8()?;
                Event::Stage(
                    *STAGES
                        .get(index as usize)
                        .ok_or(Error::UnknownKind(index))?,
                )
            }
            2 => Event::Transaction {
                package_count: reader.read_u32()?,
                download_bytes: reader.read_u64()?,
            },
            3 => Event::Download {
                received_bytes: reader.read_u64()?,
                total_bytes: reader.read_u64()?,
            },
            4 => Event::Package {
                index: reader.read_u32()?,
                count: reader.read_u32()?,
                name: reader.read_text()?,
                percent: reader.read_u32()?,
            },
            5 => Event::Hook {
                index: reader.read_u32()?,
                count: reader.read_u32()?,
                name: reader.read_text()?,
            },
            6 => Event::Line(reader.read_text()?),
            7 => Event::Done {
                is_ok: reader.read_u8()? != 0,
                message: reader.read_text()?,
            },
            kind => return Err(Error::UnknownKind(kind)),
        })
    }
}

impl Message for Forward {
    fn write(&self, writer: &mut Writer) {
        writer.write_bytes(&self.token);
        writer.write_u32(self.args.len() as u32);
        for argument in &self.args {
            writer.write_text(argument);
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, Error> {
        let token = reader.read_bytes()?;
        let count = reader.read_u32()? as usize;
        if count > MAX_FRAME_LENGTH / 4 {
            return Err(Error::TooManyArguments(count));
        }
        let args = (0..count)
            .map(|_| reader.read_text())
            .collect::<Result<_, _>>()?;
        Ok(Forward { token, args })
    }
}

impl Message for Exit {
    fn write(&self, writer: &mut Writer) {
        writer.write_u32(self.0 as u32);
    }

    fn read(reader: &mut Reader) -> Result<Self, Error> {
        Ok(Exit(reader.read_u32()? as i32))
    }
}

#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    bytes_to_skip: usize,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        let skip_length = self.bytes_to_skip.min(bytes.len());
        self.bytes_to_skip -= skip_length;
        self.buffer.extend_from_slice(&bytes[skip_length..]);
    }

    pub fn next_message<T: Message>(&mut self) -> Option<Result<T, Error>> {
        let header = self.buffer.get(..4)?;
        let length = u32::from_le_bytes(header.try_into().unwrap()) as usize;
        if length > MAX_FRAME_LENGTH {
            let end = self.buffer.len().min(4 + length);
            self.buffer.drain(..end);
            self.bytes_to_skip = 4 + length - end;
            return Some(Err(Error::FrameTooLarge(length)));
        }
        if self.buffer.len() < 4 + length {
            return None;
        }
        let body: Vec<u8> = self.buffer.drain(..4 + length).skip(4).collect();
        Some(decode_body(&body))
    }

    pub fn finish(&mut self) -> Result<(), Error> {
        if self.buffer.is_empty() && self.bytes_to_skip == 0 {
            return Ok(());
        }
        self.buffer.clear();
        self.bytes_to_skip = 0;
        Err(Error::Truncated)
    }
}

fn decode_body<T: Message>(body: &[u8]) -> Result<T, Error> {
    let mut reader = Reader { bytes: body };
    let message = T::read(&mut reader)?;
    match reader.bytes.len() {
        0 => Ok(message),
        length => Err(Error::TrailingBytes(length)),
    }
}

pub fn read_one<T: Message>(source: &mut impl std::io::Read) -> std::io::Result<Option<T>> {
    let mut header = [0u8; 4];
    match source.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let to_io_error = |error: Error| std::io::Error::other(format!("{error:?}"));
    let length = u32::from_le_bytes(header) as usize;
    if length > MAX_FRAME_LENGTH {
        return Err(to_io_error(Error::FrameTooLarge(length)));
    }
    let mut body = vec![0u8; length];
    source.read_exact(&mut body)?;
    decode_body(&body).map(Some).map_err(to_io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Message + PartialEq + std::fmt::Debug>(message: T) {
        let mut decoder = Decoder::default();
        let frame = message.encode();
        let (first, second) = frame.split_at(frame.len() / 2);
        decoder.push(first);
        assert!(decoder.next_message::<T>().is_none());
        decoder.push(second);
        assert_eq!(decoder.next_message::<T>(), Some(Ok(message)));
        assert!(decoder.next_message::<T>().is_none());
    }

    #[test]
    fn every_message_survives_encoding_split_across_reads() {
        round_trip(Request::Upgrade);
        round_trip(Request::Quit);
        round_trip(Request::Allow(4_194_303));
        round_trip(Event::Ready {
            version: VERSION,
            socket: "/run/user/1000/proscenio-packages.sock".to_owned(),
            token: vec![7; TOKEN_LENGTH],
        });
        for stage in STAGES {
            round_trip(Event::Stage(stage));
        }
        round_trip(Event::Transaction {
            package_count: 159,
            download_bytes: 403_810_000,
        });
        round_trip(Event::Download {
            received_bytes: 1,
            total_bytes: u64::MAX,
        });
        round_trip(Event::Package {
            index: 3,
            count: 159,
            name: "gtk4".to_owned(),
            percent: 100,
        });
        round_trip(Event::Hook {
            index: 1,
            count: 14,
            name: "Arming ConditionNeedsUpdate...".to_owned(),
        });
        round_trip(Event::Line("ünïcode".to_owned()));
        round_trip(Event::Done {
            is_ok: false,
            message: "failed to commit transaction".to_owned(),
        });
        round_trip(Forward {
            token: vec![1; TOKEN_LENGTH],
            args: vec!["--upgrade".to_owned(), "--".to_owned(), "a.pkg".to_owned()],
        });
        round_trip(Exit(-1));
    }

    #[test]
    fn malformed_frames_are_errors_not_panics() {
        let mut decoder = Decoder::default();
        decoder.push(&[1, 0, 0, 0, 99]);
        assert_eq!(
            decoder.next_message::<Event>(),
            Some(Err(Error::UnknownKind(99)))
        );
        decoder.push(&[2, 0, 0, 0, 2, 1]);
        assert_eq!(decoder.next_message::<Event>(), Some(Err(Error::Truncated)));
        decoder.push(&[2, 0, 0, 0, 1, 9]);
        assert_eq!(
            decoder.next_message::<Request>(),
            Some(Err(Error::TrailingBytes(1)))
        );
        decoder.push(&[5, 0, 0, 0, 6]);
        decoder.push(&(MAX_FRAME_LENGTH as u32 + 1).to_le_bytes());
        assert_eq!(
            decoder.next_message::<Event>(),
            Some(Err(Error::FieldTooLarge(MAX_FRAME_LENGTH + 1)))
        );
        decoder.push(&[8, 0, 0, 0, 0, 0, 0, 0]);
        decoder.push(&(u32::MAX).to_le_bytes());
        assert_eq!(
            decoder.next_message::<Forward>(),
            Some(Err(Error::TooManyArguments(u32::MAX as usize)))
        );
        decoder.push(&(u32::MAX).to_le_bytes());
        assert_eq!(
            decoder.next_message::<Event>(),
            Some(Err(Error::FrameTooLarge(u32::MAX as usize)))
        );
        assert_eq!(decoder.finish(), Err(Error::Truncated));
        assert_eq!(decoder.finish(), Ok(()));
    }

    #[test]
    fn an_oversized_frame_is_skipped_and_the_next_frame_still_decodes() {
        let length = MAX_FRAME_LENGTH + 1;
        let mut stream = (length as u32).to_le_bytes().to_vec();
        stream.extend(vec![0; length]);
        stream.extend(Request::Quit.encode());
        for chunk_length in [2, 4, 65536, stream.len()] {
            let mut decoder = Decoder::default();
            let mut messages = Vec::new();
            for chunk in stream.chunks(chunk_length) {
                decoder.push(chunk);
                messages.extend(std::iter::from_fn(|| decoder.next_message::<Request>()));
            }
            assert_eq!(
                messages,
                [Err(Error::FrameTooLarge(length)), Ok(Request::Quit)],
                "chunks of {chunk_length}"
            );
            assert_eq!(decoder.finish(), Ok(()), "chunks of {chunk_length}");
        }
    }

    #[test]
    fn a_frame_cut_off_at_the_end_of_the_stream_is_truncated() {
        let frame = Request::Allow(42).encode();
        let mut decoder = Decoder::default();
        decoder.push(&frame);
        assert_eq!(decoder.next_message(), Some(Ok(Request::Allow(42))));
        assert_eq!(decoder.finish(), Ok(()));
        decoder.push(&frame[..6]);
        assert_eq!(decoder.next_message::<Request>(), None);
        assert_eq!(decoder.finish(), Err(Error::Truncated));
        assert_eq!(decoder.finish(), Ok(()));
    }

    #[test]
    fn a_blocking_reader_reads_frames_one_by_one_and_stops_at_the_end() {
        let mut stream = Request::Upgrade.encode();
        stream.extend(Request::Quit.encode());
        let mut source = std::io::Cursor::new(stream);
        assert_eq!(read_one(&mut source).unwrap(), Some(Request::Upgrade));
        assert_eq!(read_one(&mut source).unwrap(), Some(Request::Quit));
        assert_eq!(read_one::<Request>(&mut source).unwrap(), None);
        let mut source = std::io::Cursor::new([2, 0, 0, 0, 1, 9]);
        assert_eq!(
            read_one::<Request>(&mut source).unwrap_err().to_string(),
            "TrailingBytes(1)"
        );
        let mut source = std::io::Cursor::new((MAX_FRAME_LENGTH as u32 + 1).to_le_bytes());
        assert_eq!(
            read_one::<Request>(&mut source).unwrap_err().to_string(),
            format!("FrameTooLarge({})", MAX_FRAME_LENGTH + 1)
        );
    }

    #[test]
    fn a_token_survives_hex_and_bad_hex_is_refused() {
        let token = vec![0, 1, 0xab, 0xff];
        assert_eq!(unhex(&hex(&token)), Some(token));
        assert_eq!(unhex("abc"), None);
        assert_eq!(unhex("zz"), None);
        assert_eq!(unhex("+a+b"), None);
        assert_eq!(unhex("é"), None);
    }
}
