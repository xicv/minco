use minco_mcp::BoundedMessageReader;
use std::io::Cursor;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn permits_multiple_messages_that_each_fit_the_line_limit() {
    let source = Cursor::new(b"1234\n5678\n".to_vec());
    let mut reader = BoundedMessageReader::new(source, 5);
    let mut output = Vec::new();

    reader
        .read_to_end(&mut output)
        .await
        .expect("bounded lines");

    assert_eq!(output, b"1234\n5678\n");
}

#[tokio::test]
async fn rejects_a_protocol_message_before_its_line_exceeds_the_limit() {
    let source = Cursor::new(b"123456\n".to_vec());
    let mut reader = BoundedMessageReader::new(source, 5);
    let mut output = Vec::new();

    let error = reader
        .read_to_end(&mut output)
        .await
        .expect_err("oversized protocol line must fail closed");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("MCP message exceeds"));
}
