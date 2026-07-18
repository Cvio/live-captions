use std::io::Cursor;

use log::debug;
use rodio::OutputStreamBuilder;

/// Sends `text` to the TTS server at `endpoint`, receives audio bytes,
/// and plays them on the default output device. Blocks until playback ends.
/// `language` selects the server voice; "auto" lets the server detect it.
pub async fn speak(text: &str, endpoint: &str, language: &str) -> Result<(), String> {
    // 1. Ask the TTS server to synthesize the text.
    let client = reqwest::Client::new();
    let response = client
        .post(endpoint)
        .json(&serde_json::json!({ "text": text, "language": language }))
        .send()
        .await
        .map_err(|e| format!("TTS request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("TTS server returned status {}", response.status()));
    }

    // 2. Read the returned audio (expected: WAV bytes).
    let audio_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read TTS audio: {e}"))?
        .to_vec();

    debug!("TTS returned {} bytes of audio", audio_bytes.len());

    // 3. Play the audio. rodio playback is blocking, so run it off the async
    //    runtime on a dedicated blocking thread.
    tokio::task::spawn_blocking(move || play_bytes(audio_bytes))
        .await
        .map_err(|e| format!("Playback thread panicked: {e}"))?
}

/// Plays raw audio bytes (WAV) on the default output device, mirroring the
/// rodio pattern used in audio_feedback.rs.
fn play_bytes(audio_bytes: Vec<u8>) -> Result<(), String> {
    let stream_handle = OutputStreamBuilder::from_default_device()
        .map_err(|e| format!("No audio output device: {e}"))?
        .open_stream()
        .map_err(|e| format!("Failed to open audio stream: {e}"))?;

    let mixer = stream_handle.mixer();
    let cursor = Cursor::new(audio_bytes);

    let sink = rodio::play(mixer, cursor).map_err(|e| format!("Failed to play audio: {e}"))?;
    sink.sleep_until_end();

    Ok(())
}
