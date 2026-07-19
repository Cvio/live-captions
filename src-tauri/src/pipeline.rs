//! Pipeline message types and channel plumbing.
//!
//! The pipeline is a chain of stages connected by channels. Each message type
//! below is what flows through ONE pipe. Ownership of each message moves
//! stage-to-stage through the channels; no two stages ever hold the same
//! message at once.
//!
//! capture --(AudioFrame)--> vad --(SpeechSegment)--> transcribe
//!     --(Transcript)--> translate --(Caption)--> caption window

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::HostTrait;
use log::{error, info};
use tauri::Manager;

use crate::audio_toolkit::audio::{open_capture_stream, AudioChunk, FrameResampler};
use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;
use crate::managers::audio::AudioRecordingManager;
use crate::settings::get_settings;

/// Duration of one AudioFrame. Matches the Silero VAD frame size.
const FRAME_MS: u64 = 30;
/// How often the capture loop wakes up to check the stop flag while idle.
const CAPTURE_POLL: Duration = Duration::from_millis(100);

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

/// Capture stage (A2): open the selected input device and continuously send
/// ~30 ms AudioFrames (16 kHz mono f32) into `frame_tx` until `stop` is set.
///
/// Device selection reuses the settings-driven logic on
/// AudioRecordingManager. The cpal stream is opened on the spawned thread
/// (cpal streams are !Send) and released when the thread exits.
pub fn spawn_capture(
    app: &tauri::AppHandle,
    frame_tx: Sender<AudioFrame>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    let settings = get_settings(app);
    let recording_manager = app.state::<Arc<AudioRecordingManager>>();
    let device = recording_manager.get_effective_microphone_device(&settings);

    std::thread::spawn(move || {
        let host = crate::audio_toolkit::get_cpal_host();
        let device = match device.or_else(|| host.default_input_device()) {
            Some(d) => d,
            None => {
                error!("Capture stage: no input device found");
                return;
            }
        };

        let (sample_tx, sample_rx) = channel::<AudioChunk>();
        let (stream, in_sample_rate) =
            match open_capture_stream(&device, sample_tx, stop.clone()) {
                Ok(ok) => ok,
                Err(e) => {
                    error!("Capture stage: failed to open input stream: {e}");
                    return;
                }
            };

        let mut resampler = FrameResampler::new(
            in_sample_rate as usize,
            WHISPER_SAMPLE_RATE as usize,
            Duration::from_millis(FRAME_MS),
        );

        // Monotonic timestamp derived from the count of emitted 16 kHz samples.
        let mut samples_sent: u64 = 0;

        info!("Capture stage started ({} Hz input)", in_sample_rate);

        'outer: loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }

            let raw = match sample_rx.recv_timeout(CAPTURE_POLL) {
                Ok(AudioChunk::Samples(s)) => s,
                Ok(AudioChunk::EndOfStream) => break,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };

            let mut send_failed = false;
            resampler.push(&raw, &mut |frame: &[f32]| {
                let timestamp_ms = samples_sent * 1000 / WHISPER_SAMPLE_RATE as u64;
                samples_sent += frame.len() as u64;
                if frame_tx
                    .send(AudioFrame {
                        samples: frame.to_vec(),
                        timestamp_ms,
                    })
                    .is_err()
                {
                    send_failed = true;
                }
            });
            if send_failed {
                // Downstream hung up; nothing left to feed.
                break 'outer;
            }
        }

        drop(stream); // releases the audio device
        info!("Capture stage stopped");
    })
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
