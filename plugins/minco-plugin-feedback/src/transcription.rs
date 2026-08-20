//! Compatibility names for transcription primitives now shared by interactions.

#[cfg(feature = "command-transcription")]
pub use minco_interaction::CommandTranscriber;
#[cfg(feature = "openai-transcription")]
pub use minco_interaction::OpenAiTranscriber;
pub use minco_interaction::{
    AudioInput, DisabledTranscriber, Transcriber as FeedbackTranscriber, Transcript,
    TranscriptionError, TranscriptionRequest, TranscriptionResult, TranscriptionService,
};
