use crate::{
    MailAddress, MailError, MailErrorKind, MailMessage, MailReceipt, MailTransport, render_mime,
};
use async_trait::async_trait;
use chrono::Utc;
use std::{fmt, net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    time::timeout,
};

const MAX_SMTP_RESPONSE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct MailpitTransportConfig {
    pub endpoint: SocketAddr,
    pub from: MailAddress,
    pub connect_timeout: Duration,
    pub command_timeout: Duration,
}

impl MailpitTransportConfig {
    pub fn new(endpoint: SocketAddr, from: MailAddress) -> Result<Self, MailError> {
        if !endpoint.ip().is_loopback() {
            return Err(MailError::new(
                MailErrorKind::Configuration,
                "mailpit",
                "Mailpit SMTP endpoint must be loopback-only",
            ));
        }
        from.validate()?;
        Ok(Self {
            endpoint,
            from,
            connect_timeout: Duration::from_secs(2),
            command_timeout: Duration::from_secs(3),
        })
    }
}

impl Default for MailpitTransportConfig {
    fn default() -> Self {
        Self::new(
            SocketAddr::from(([127, 0, 0, 1], 1025)),
            MailAddress::new("minco@localhost").expect("static local address"),
        )
        .expect("static loopback Mailpit configuration")
    }
}

#[derive(Clone)]
pub struct MailpitTransport {
    config: MailpitTransportConfig,
}

impl MailpitTransport {
    pub fn new(config: MailpitTransportConfig) -> Result<Self, MailError> {
        if !config.endpoint.ip().is_loopback()
            || config.connect_timeout.is_zero()
            || config.command_timeout.is_zero()
        {
            return Err(MailError::new(
                MailErrorKind::Configuration,
                "mailpit",
                "Mailpit transport configuration is invalid",
            ));
        }
        config.from.validate()?;
        Ok(Self { config })
    }
}

impl Default for MailpitTransport {
    fn default() -> Self {
        Self::new(MailpitTransportConfig::default()).expect("static Mailpit configuration")
    }
}

impl fmt::Debug for MailpitTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailpitTransport")
            .field("endpoint", &self.config.endpoint)
            .field("from", &"[REDACTED]")
            .field("connect_timeout", &self.config.connect_timeout)
            .field("command_timeout", &self.config.command_timeout)
            .finish()
    }
}

#[async_trait]
impl MailTransport for MailpitTransport {
    fn name(&self) -> &'static str {
        "mailpit"
    }

    async fn send(&self, message: &MailMessage, attempt: u32) -> Result<MailReceipt, MailError> {
        message.validate()?;
        let mime = render_mime(message, &self.config.from)?;
        let stream = timeout(
            self.config.connect_timeout,
            TcpStream::connect(self.config.endpoint),
        )
        .await
        .map_err(|_| {
            MailError::new(
                MailErrorKind::Unavailable,
                self.name(),
                "Mailpit SMTP connection timed out",
            )
        })?
        .map_err(|_| {
            MailError::new(
                MailErrorKind::Unavailable,
                self.name(),
                "Mailpit SMTP endpoint is unavailable",
            )
        })?;
        let mut connection = SmtpConnection::new(stream, self.config.command_timeout);

        connection.expect_response(220, false).await?;
        connection
            .command("EHLO minco.local\r\n", 250, false)
            .await?;
        connection
            .command(
                &format!("MAIL FROM:<{}>\r\n", self.config.from.address),
                250,
                false,
            )
            .await?;
        for recipient in message.recipients() {
            connection
                .command(&format!("RCPT TO:<{}>\r\n", recipient.address), 250, false)
                .await?;
        }
        connection.command("DATA\r\n", 354, false).await?;
        connection.write_message(&mime).await?;
        connection.expect_response(250, true).await?;
        let _ = connection.command("QUIT\r\n", 221, true).await;

        Ok(MailReceipt {
            message_id: message.id,
            transport: self.name().into(),
            provider_message_id: format!("mailpit:{}", message.id),
            accepted_at: Utc::now(),
            attempt,
        })
    }
}

struct SmtpConnection {
    stream: BufReader<TcpStream>,
    command_timeout: Duration,
}

impl SmtpConnection {
    fn new(stream: TcpStream, command_timeout: Duration) -> Self {
        Self {
            stream: BufReader::new(stream),
            command_timeout,
        }
    }

    async fn command(
        &mut self,
        command: &str,
        expected: u16,
        after_data: bool,
    ) -> Result<String, MailError> {
        self.write_all(command.as_bytes(), after_data).await?;
        self.expect_response(expected, after_data).await
    }

    async fn write_message(&mut self, mime: &[u8]) -> Result<(), MailError> {
        let mut body = dot_stuff(mime);
        if !body.ends_with(b"\r\n") {
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b".\r\n");
        self.write_all(&body, true).await
    }

    async fn write_all(&mut self, bytes: &[u8], after_data: bool) -> Result<(), MailError> {
        timeout(self.command_timeout, async {
            self.stream.get_mut().write_all(bytes).await?;
            self.stream.get_mut().flush().await
        })
        .await
        .map_err(|_| smtp_io_error(after_data, "SMTP write timed out"))?
        .map_err(|_| smtp_io_error(after_data, "SMTP connection closed during write"))
    }

    async fn expect_response(
        &mut self,
        expected: u16,
        after_data: bool,
    ) -> Result<String, MailError> {
        let (status, response) = timeout(self.command_timeout, self.read_response())
            .await
            .map_err(|_| smtp_io_error(after_data, "SMTP response timed out"))?
            .map_err(|_| smtp_io_error(after_data, "SMTP connection closed during response"))?;
        if status == expected {
            return Ok(response);
        }
        let kind = match status / 100 {
            4 if after_data => MailErrorKind::Unavailable,
            4 => MailErrorKind::Unavailable,
            5 => MailErrorKind::Rejected,
            _ if after_data => MailErrorKind::Ambiguous,
            _ => MailErrorKind::Protocol,
        };
        Err(MailError::new(
            kind,
            "mailpit",
            format!("SMTP server returned status {status}"),
        ))
    }

    async fn read_response(&mut self) -> std::io::Result<(u16, String)> {
        let mut complete = String::new();
        let mut status = None;
        loop {
            let mut line = String::new();
            if self.stream.read_line(&mut line).await? == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "SMTP response ended unexpectedly",
                ));
            }
            if complete.len() + line.len() > MAX_SMTP_RESPONSE_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "SMTP response exceeds the bounded size",
                ));
            }
            let bytes = line.as_bytes();
            if bytes.len() < 4 || !bytes[..3].iter().all(u8::is_ascii_digit) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "SMTP response is malformed",
                ));
            }
            let code = line[..3].parse::<u16>().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "SMTP status is malformed")
            })?;
            if status
                .replace(code)
                .is_some_and(|previous| previous != code)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "SMTP multiline status changed",
                ));
            }
            let continuation = bytes[3] == b'-';
            if !continuation && bytes[3] != b' ' {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "SMTP response separator is malformed",
                ));
            }
            complete.push_str(line.trim_end_matches(['\r', '\n']));
            complete.push('\n');
            if !continuation {
                return Ok((code, complete));
            }
        }
    }
}

fn smtp_io_error(after_data: bool, message: &str) -> MailError {
    MailError::new(
        if after_data {
            MailErrorKind::Ambiguous
        } else {
            MailErrorKind::Unavailable
        },
        "mailpit",
        message,
    )
}

fn dot_stuff(message: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(message.len() + 16);
    let mut at_line_start = true;
    for byte in message {
        if at_line_start && *byte == b'.' {
            output.push(b'.');
        }
        output.push(*byte);
        at_line_start = *byte == b'\n';
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MailMessage;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    #[tokio::test]
    async fn loopback_smtp_captures_rich_mime_without_bcc_header() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let endpoint = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            stream
                .get_mut()
                .write_all(b"220 mailpit ESMTP\r\n")
                .await
                .unwrap();
            let mut captured = Vec::new();
            loop {
                let mut line = String::new();
                if stream.read_line(&mut line).await.unwrap() == 0 {
                    break;
                }
                if line.starts_with("EHLO") {
                    stream
                        .get_mut()
                        .write_all(b"250-mailpit\r\n250 8BITMIME\r\n")
                        .await
                        .unwrap();
                } else if line.starts_with("MAIL FROM") || line.starts_with("RCPT TO") {
                    stream.get_mut().write_all(b"250 ok\r\n").await.unwrap();
                } else if line == "DATA\r\n" {
                    stream
                        .get_mut()
                        .write_all(b"354 send data\r\n")
                        .await
                        .unwrap();
                    loop {
                        let mut data_line = Vec::new();
                        stream.read_until(b'\n', &mut data_line).await.unwrap();
                        if data_line == b".\r\n" {
                            break;
                        }
                        captured.extend_from_slice(&data_line);
                    }
                    stream
                        .get_mut()
                        .write_all(b"250 accepted\r\n")
                        .await
                        .unwrap();
                } else if line.starts_with("QUIT") {
                    stream.get_mut().write_all(b"221 bye\r\n").await.unwrap();
                    break;
                }
            }
            captured
        });

        let transport = MailpitTransport::new(
            MailpitTransportConfig::new(
                endpoint,
                MailAddress::new("no-reply@example.com").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let message = MailMessage::builder("account.welcome", "Welcome")
            .to(MailAddress::new("person@example.com").unwrap())
            .bcc(MailAddress::new("audit@example.com").unwrap())
            .text("Hello")
            .html("<p>Hello</p>")
            .build()
            .unwrap();
        let receipt = transport.send(&message, 1).await.unwrap();
        assert_eq!(receipt.transport, "mailpit");
        let captured = String::from_utf8(server.await.unwrap()).unwrap();
        assert!(!captured.contains("Bcc:"));
        assert!(!captured.contains("audit@example.com"));
        assert!(captured.contains("multipart/alternative"));
    }

    #[test]
    fn remote_plaintext_smtp_is_rejected() {
        let endpoint = SocketAddr::from(([192, 0, 2, 1], 1025));
        assert!(
            MailpitTransportConfig::new(
                endpoint,
                MailAddress::new("no-reply@example.com").unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn dot_stuffing_covers_every_line_start() {
        assert_eq!(dot_stuff(b".a\r\n.b\r\n"), b"..a\r\n..b\r\n");
    }
}
