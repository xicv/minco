# Feedback Plugin Research Sources

Research snapshot: 2026-07-24. The plugin uses provider-neutral interfaces; the
sources below explain the browser and transcription constraints behind the
implementation.

## Browser capture

- Screen Capture specification: https://www.w3.org/TR/screen-capture/
- Media Capture and Streams: https://www.w3.org/TR/mediacapture-streams/
- MediaStream Recording: https://www.w3.org/TR/mediastream-recording/
- HTML canvas image serialization: https://html.spec.whatwg.org/multipage/canvas.html
- Custom Elements: https://html.spec.whatwg.org/multipage/custom-elements.html

Design consequences:

- screen and microphone capture always start from a user action;
- browser permission/selection UI is never bypassed;
- the widget captures one selected surface rather than DOM/session replay;
- streams are stopped promptly after capture;
- images are downscaled and size checked before upload;
- voice capture records a bounded media blob and does not claim browser speech
  recognition support.

## Transcription

- OpenAI speech-to-text guide: https://platform.openai.com/docs/guides/speech-to-text
- OpenAI audio transcription API: https://platform.openai.com/docs/api-reference/audio/createTranscription
- OpenAI Whisper repository: https://github.com/openai/whisper
- whisper.cpp repository: https://github.com/ggml-org/whisper.cpp

Design consequences:

- `TranscriptionService` is provider-neutral and optional;
- the official plugin offers an OpenAI-compatible HTTP adapter behind a Cargo
  feature and a local command adapter that never invokes a shell;
- audio remains useful as an attachment when transcription is disabled or
  fails;
- provider, model, language, retention, residency, and cost are deployment
  decisions rather than hidden defaults.

## Feedback workflow and security

- Sentry JavaScript SDK feedback implementation:
  https://github.com/getsentry/sentry-javascript
- RFC 9457 Problem Details: https://www.rfc-editor.org/rfc/rfc9457
- OWASP File Upload Cheat Sheet:
  https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html

The implementation borrows the discoverable embeddable-widget idea, but keeps
storage, workflow, identity, notifications, audit, events, and AI export in
explicit Minco ports. It does not embed a vendor control plane.
