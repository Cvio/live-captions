//! Pipeline message types and channel plumbing.
//!
//! The pipeline is a chain of stages connected by channels. Each message type
//! below is what flows through ONE pipe. Ownership of each message moves
//! stage-to-stage through the channels; no two stages ever hold the same
//! message at once.
//!
//! capture --(AudioFrame)--> vad --(SpeechSegment)--> transcribe
//!     --(Transcript)--> translate --(Caption)--> caption window

use std::sync::mpsc::{channel, Receiver, Sender};

/// A short frame of raw audio from the capture stream (already mono f32,
/// 16 kHz). Small and frequent: the capture thread sends these continuously
/// while running.
pub struct AudioFrame {
    pub samples: Vec<f32>,
    /// Milliseconds since pipeline start, at the first sample of this frame.
    pub timestamp_ms: u64,
}

/// One complete utterance, cut by the VAD at a silence boundary.
pub struct SpeechSegment {
    pub samples: Vec<f32>,
    /// When the utterance started, ms since pipeline start.
    pub start_ms: u64,
    /// When the VAD decided it ended.
    pub end_ms: u64,
}

/// The transcription of one utterance, before translation.
pub struct Transcript {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// A finished caption, ready for display.
#[derive(Clone, serde::Serialize, specta::Type)]
pub struct Caption {
    /// Transcribed source-language text (Spanish).
    pub original: String,
    /// Translated text (English).
    pub translated: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// The sending ends held by each stage, and the receiving ends the next
/// stage consumes. Built once at pipeline start; each thread takes ownership
/// of exactly the ends it needs.
pub struct PipelineChannels {
    pub frame_tx: Sender<AudioFrame>,
    pub frame_rx: Receiver<AudioFrame>,
    pub segment_tx: Sender<SpeechSegment>,
    pub segment_rx: Receiver<SpeechSegment>,
    pub transcript_tx: Sender<Transcript>,
    pub transcript_rx: Receiver<Transcript>,
    pub caption_tx: Sender<Caption>,
    pub caption_rx: Receiver<Caption>,
}

impl PipelineChannels {
    pub fn new() -> Self {
        let (frame_tx, frame_rx) = channel::<AudioFrame>();
        let (segment_tx, segment_rx) = channel::<SpeechSegment>();
        let (transcript_tx, transcript_rx) = channel::<Transcript>();
        let (caption_tx, caption_rx) = channel::<Caption>();
        Self {
            frame_tx,
            frame_rx,
            segment_tx,
            segment_rx,
            transcript_tx,
            transcript_rx,
            caption_tx,
            caption_rx,
        }
    }
}
