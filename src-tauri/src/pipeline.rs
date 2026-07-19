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
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::HostTrait;
use log::{debug, error, info};
use tauri::{Emitter, Manager};

use crate::audio_toolkit::audio::{open_capture_stream, AudioChunk, FrameResampler};
use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;
use crate::audio_toolkit::vad::{SileroVad, SmoothedVad, VadFrame, VoiceActivityDetector};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::llm_client;
use crate::settings::{get_settings, PostProcessProvider};

/// Duration of one AudioFrame. Matches the Silero VAD frame size.
const FRAME_MS: u64 = 30;
/// How often the capture loop wakes up to check the stop flag while idle.
const CAPTURE_POLL: Duration = Duration::from_millis(100);

/// VAD segmentation tunables (A3).
/// Silence run that ends an utterance.
const SILENCE_END_MS: u64 = 500;
/// Utterances with less accumulated speech than this are dropped as noise.
const MIN_SPEECH_MS: u64 = 300;
/// Pre-roll kept from before speech onset (frames of 30 ms; 7 ~= 210 ms).
const PRE_ROLL_FRAMES: usize = 7;
/// Short VAD dropouts bridged inside an utterance (frames; 5 ~= 150 ms).
const HANGOVER_FRAMES: usize = 5;
/// Consecutive voiced frames required to trigger speech onset.
const ONSET_FRAMES: usize = 2;
/// Silero speech probability threshold (matches the recording path).
const VAD_THRESHOLD: f32 = 0.3;

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

/// VAD segmentation stage (A3): feed frames through the smoothed Silero VAD,
/// accumulate voiced audio, and emit one SpeechSegment per utterance once
/// silence has lasted >= SILENCE_END_MS. The SmoothedVad supplies onset
/// smoothing, a ~210 ms pre-roll at speech onset, and bridges short
/// dropouts; the >= 500 ms end-of-utterance silence is counted here.
pub fn spawn_vad(
    vad_model_path: std::path::PathBuf,
    frame_rx: Receiver<AudioFrame>,
    segment_tx: Sender<SpeechSegment>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let silero = match SileroVad::new(&vad_model_path, VAD_THRESHOLD) {
            Ok(v) => v,
            Err(e) => {
                error!("VAD stage: failed to create SileroVad: {e}");
                return;
            }
        };
        let mut vad = SmoothedVad::new(
            Box::new(silero),
            PRE_ROLL_FRAMES,
            HANGOVER_FRAMES,
            ONSET_FRAMES,
        );

        // Current utterance being accumulated, if any.
        let mut segment: Vec<f32> = Vec::new();
        let mut in_segment = false;
        let mut start_ms: u64 = 0;
        let mut speech_ms: u64 = 0;
        let mut silence_run_ms: u64 = 0;

        info!("VAD stage started");

        // Loop ends when the capture stage drops frame_tx.
        while let Ok(frame) = frame_rx.recv() {
            match vad.push_frame(&frame.samples) {
                Ok(VadFrame::Speech(buf)) => {
                    if !in_segment {
                        in_segment = true;
                        segment.clear();
                        speech_ms = 0;
                        // buf holds pre-roll + current frame; the utterance
                        // started that far before this frame's timestamp.
                        let buf_ms = buf.len() as u64 * 1000 / WHISPER_SAMPLE_RATE as u64;
                        start_ms = frame
                            .timestamp_ms
                            .saturating_sub(buf_ms.saturating_sub(FRAME_MS));
                    }
                    segment.extend_from_slice(buf);
                    speech_ms += FRAME_MS;
                    silence_run_ms = 0;
                }
                Ok(VadFrame::Noise) => {
                    if !in_segment {
                        continue;
                    }
                    // Keep the natural pause inside the utterance buffer.
                    segment.extend_from_slice(&frame.samples);
                    silence_run_ms += FRAME_MS;
                    if silence_run_ms >= SILENCE_END_MS {
                        let end_ms = frame.timestamp_ms + FRAME_MS;
                        if speech_ms >= MIN_SPEECH_MS {
                            debug!(
                                "VAD stage: utterance {}..{} ms ({} ms speech)",
                                start_ms, end_ms, speech_ms
                            );
                            if segment_tx
                                .send(SpeechSegment {
                                    samples: std::mem::take(&mut segment),
                                    start_ms,
                                    end_ms,
                                })
                                .is_err()
                            {
                                break; // downstream hung up
                            }
                        } else {
                            debug!(
                                "VAD stage: dropped {} ms of speech as noise",
                                speech_ms
                            );
                            segment.clear();
                        }
                        in_segment = false;
                        silence_run_ms = 0;
                    }
                }
                Err(e) => {
                    error!("VAD stage: push_frame failed: {e}");
                }
            }
        }

        info!("VAD stage stopped");
    })
}

/// Transcription stage (A4): run each SpeechSegment through the existing
/// TranscriptionManager. Empty transcriptions are dropped silently; errors
/// are logged and the segment discarded so the pipeline keeps flowing.
/// The caller is responsible for initiating the model load before start.
pub fn spawn_transcribe(
    transcription_manager: Arc<TranscriptionManager>,
    segment_rx: Receiver<SpeechSegment>,
    transcript_tx: Sender<Transcript>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        info!("Transcription stage started");

        while let Ok(seg) = segment_rx.recv() {
            let segment_ms = seg.samples.len() as u64 * 1000 / WHISPER_SAMPLE_RATE as u64;
            let st = std::time::Instant::now();
            match transcription_manager.transcribe(seg.samples) {
                Ok(text) => {
                    debug!(
                        "Transcription stage: {} ms segment transcribed in {} ms",
                        segment_ms,
                        st.elapsed().as_millis()
                    );
                    if text.is_empty() {
                        continue;
                    }
                    if transcript_tx
                        .send(Transcript {
                            text,
                            start_ms: seg.start_ms,
                            end_ms: seg.end_ms,
                        })
                        .is_err()
                    {
                        break; // downstream hung up
                    }
                }
                Err(e) => {
                    error!("Transcription stage: transcribe failed: {e}");
                }
            }
        }

        info!("Transcription stage stopped");
    })
}

/// Translation stage (A5) endpoint: local Ollama, OpenAI-compatible.
const TRANSLATE_BASE_URL: &str = "http://localhost:11434/v1";
const TRANSLATE_MODEL: &str = "qwen3:8b";
const TRANSLATE_SYSTEM_PROMPT: &str = "You are a translator. Translate the user's Spanish text \
to natural English. Output only the translation, nothing else.";
/// Placeholder shown when the translation request fails; the pipeline never stalls.
const TRANSLATION_FAILED: &str = "[translation failed]";

/// Translation stage (A5): translate each Transcript es->en via the existing
/// llm_client against local Ollama. llm_client is async, so requests are
/// bridged with tauri::async_runtime::block_on inside this thread.
pub fn spawn_translate(
    transcript_rx: Receiver<Transcript>,
    caption_tx: Sender<Caption>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let provider = PostProcessProvider {
            id: "custom".to_string(),
            label: "Ollama (captions)".to_string(),
            base_url: TRANSLATE_BASE_URL.to_string(),
            allow_base_url_edit: false,
            models_endpoint: None,
            supports_structured_output: false,
        };

        info!("Translation stage started");

        while let Ok(transcript) = transcript_rx.recv() {
            let st = std::time::Instant::now();
            let result = tauri::async_runtime::block_on(llm_client::send_chat_completion_with_schema(
                &provider,
                String::new(), // no API key for local Ollama
                TRANSLATE_MODEL,
                transcript.text.clone(),
                Some(TRANSLATE_SYSTEM_PROMPT.to_string()),
                None,
                Some("none".to_string()),
                None,
            ));

            let translated = match result {
                Ok(Some(content)) => {
                    let cleaned = crate::actions::strip_invisible_chars(&content)
                        .trim()
                        .to_string();
                    if cleaned.is_empty() {
                        error!("Translation stage: empty translation response");
                        TRANSLATION_FAILED.to_string()
                    } else {
                        cleaned
                    }
                }
                Ok(None) => {
                    error!("Translation stage: response had no content");
                    TRANSLATION_FAILED.to_string()
                }
                Err(e) => {
                    error!("Translation stage: request failed: {e}");
                    TRANSLATION_FAILED.to_string()
                }
            };

            debug!(
                "Translation stage: translated in {} ms",
                st.elapsed().as_millis()
            );

            if caption_tx
                .send(Caption {
                    original: transcript.text,
                    translated,
                    start_ms: transcript.start_ms,
                    end_ms: transcript.end_ms,
                })
                .is_err()
            {
                break; // downstream hung up
            }
        }

        info!("Translation stage stopped");
    })
}

/// How long stop_captions waits for the stage threads to wind down before
/// giving up and letting them finish detached.
const STOP_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// A running pipeline: the shared stop flag plus every stage thread.
pub struct PipelineHandle {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

/// Tauri managed state holding the running pipeline, if any.
#[derive(Default)]
pub struct PipelineState(pub Mutex<Option<PipelineHandle>>);

/// Emit stage (A6): forward each finished Caption to the frontend as the
/// Tauri event "caption". Exits when the translation stage drops caption_tx.
fn spawn_emit(app: tauri::AppHandle, caption_rx: Receiver<Caption>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        info!("Emit stage started");
        while let Ok(caption) = caption_rx.recv() {
            if let Err(e) = app.emit("caption", &caption) {
                error!("Emit stage: failed to emit caption event: {e}");
            }
        }
        info!("Emit stage stopped");
    })
}

/// Build channels, spawn stages A2-A5 plus the emit thread, and return the
/// handle. Assumes the caller has checked that no pipeline is running.
pub fn start(app: &tauri::AppHandle) -> Result<PipelineHandle, String> {
    // Kick off the transcription model load now (no-op if already loaded);
    // the transcribe stage waits on the load before its first segment.
    app.state::<Arc<TranscriptionManager>>().initiate_model_load();

    let vad_model_path = app
        .path()
        .resolve(
            "resources/models/silero_vad_v4.onnx",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Failed to resolve VAD model path: {e}"))?;

    let channels = PipelineChannels::new();
    let stop = Arc::new(AtomicBool::new(false));

    let threads = vec![
        spawn_capture(app, channels.frame_tx, stop.clone()),
        spawn_vad(vad_model_path, channels.frame_rx, channels.segment_tx),
        spawn_transcribe(
            app.state::<Arc<TranscriptionManager>>().inner().clone(),
            channels.segment_rx,
            channels.transcript_tx,
        ),
        spawn_translate(channels.transcript_rx, channels.caption_tx),
        spawn_emit(app.clone(), channels.caption_rx),
    ];

    // The unused receiver/sender ends of `channels` were moved into the
    // stages above; nothing is left holding a duplicate sender, so each
    // recv loop ends when its upstream stage exits.
    Ok(PipelineHandle { stop, threads })
}

/// Signal the pipeline to stop and wait (bounded) for the threads to exit.
/// The capture thread notices the flag, drops its stream (releasing the
/// device) and its sender; the shutdown then cascades stage to stage.
pub fn stop(handle: PipelineHandle) {
    handle.stop.store(true, Ordering::Relaxed);

    let (done_tx, done_rx) = channel::<()>();
    std::thread::spawn(move || {
        for t in handle.threads {
            let _ = t.join();
        }
        let _ = done_tx.send(());
    });

    match done_rx.recv_timeout(STOP_JOIN_TIMEOUT) {
        Ok(()) => info!("Caption pipeline stopped"),
        Err(_) => log::warn!(
            "Caption pipeline threads still finishing after {:?}; detaching",
            STOP_JOIN_TIMEOUT
        ),
    }
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
