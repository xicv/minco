from pathlib import Path

path = Path('extensions/minco-aws-adapters/src/ses.rs')
text = path.read_text(encoding='utf-8')

replacements = [
    (
        '''use aws_sdk_sesv2::{\n    config::Config as SesConfig,\n    operation::send_email::SendEmailError,''',
        '''use aws_sdk_sesv2::{\n    operation::send_email::SendEmailError,''',
    ),
    (
        'let output = request.send().await.map_err(classify_send_error)?;',
        'let output = request.send().await.map_err(|error| classify_send_error(&error))?;',
    ),
    (
        '''fn classify_send_error(\n    error: aws_sdk_sesv2::error::SdkError<SendEmailError>,\n) -> MailError {''',
        '''fn classify_send_error(\n    error: &aws_sdk_sesv2::error::SdkError<SendEmailError>,\n) -> MailError {''',
    ),
]

for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'expected one match, found {count}: {old!r}')
    text = text.replace(old, new)

path.write_text(text, encoding='utf-8')
