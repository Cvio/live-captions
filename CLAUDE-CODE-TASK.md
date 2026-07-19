# Claude Code instruction package — Phase A + minimal Phase B

Goal for this session: a working v0 of live-captions per PLAN.md — continuous microphone capture, VAD segmentation, Parakeet transcription, Ollama es→en translation, captions displayed in a window. Backend pipeline (Phase A) plus the smallest caption UI that proves it (Phase B minimal).

Read CLAUDE.md and PLAN.md in full before any edit. The terminology and scope rules there are binding. In particular: this codebase was derived from Handy via the voice-translator project — never describe it with the words "fork" or "upstream" in code, comments, commits, or docs. Do not reintroduce push-to-talk, paste, or keyboard-shortcut features. Do not rename the crate or touch tts.rs.

## Process rules (non-negotiable)

1. Work step by step in the order below. After EACH step: `cargo check --manifest-path src-tauri/Cargo.toml` must pass, then `git add . && git commit` with a message of the form "Phase A2: <what>". One commit per step, no squashing. The developer reviews per-commit diffs afterward; commit hygiene is the review mechanism.
2. Before writing code in any step, read the existing files you will touch or call. Do not guess at APIs of AudioRecordingManager, TranscriptionManager, the audio_toolkit VAD wrappers, or llm_client — open them and use what exists. Prefer reusing existing functions over rewriting them; refactor minimally where a private function must become pub(crate).
3. Targeted edits only. Do not reformat untouched code, do not fix unrelated warnings, do not delete dormant code.
4. If a step turns out to be impossible as specified (missing API, architectural conflict), stop and write a short STATUS.md explaining the blocker instead of improvising around it.

## Existing foundation (already committed)

- `src-tauri/src/pipeline.rs` — message types (AudioFrame, SpeechSegment, Transcript, Caption) and PipelineChannels. Extend this module; keep the type-per-pipe design.
- Audio capture: cpal-based machinery in audio_toolkit and managers/audio. 16 kHz mono f32 is the working format.
- VAD: Silero wrappers in audio_toolkit/vad (silero.rs, smoothed.rs), model in resources.
- Transcription: managers/transcription TranscriptionManager::transcribe(Vec<f32>) or similar — read it.
- Translation: llm_client has OpenAI-compatible chat completion helpers. Ollama runs at http://localhost:11434/v1, model qwen3:8b.

## Steps

### A2 — capture stage
A `start` function spawns a capture thread that opens the selected input device (reuse existing device-selection code) and continuously sends AudioFrame messages (~30 ms of samples each, monotonic timestamp_ms) into frame_tx. Clean shutdown: an AtomicBool stop flag; thread exits and releases the device when cleared. No VAD here; capture only.

### A3 — VAD segmentation stage
A thread owning frame_rx + segment_tx. Feed frames to the existing Silero VAD (use the smoothed wrapper if suitable). Accumulate speech samples while VAD reports speech; on transition to silence lasting >= 500 ms, emit one SpeechSegment (with a small pre-roll buffer of ~200 ms of audio from before speech started, if the existing VAD wrapper exposes what's needed for that cheaply — skip pre-roll otherwise). Drop segments shorter than 300 ms of speech as noise. Tunables as consts at top of file.

### A4 — transcription stage
A thread owning segment_rx + transcript_tx. For each SpeechSegment call the existing TranscriptionManager (it is in Tauri managed state as Arc; pass the Arc into the thread). Ensure the model is loaded before the pipeline starts (reuse existing load/initiate functions). Empty transcriptions are dropped silently.

### A5 — translation stage
A thread owning transcript_rx + caption_tx. For each Transcript, call the existing llm_client chat completion against provider settings for a local OpenAI-compatible endpoint (base URL http://localhost:11434/v1, model qwen3:8b, no API key). System prompt: "You are a translator. Translate the user's Spanish text to natural English. Output only the translation, nothing else." Strip whitespace and any invisible chars (reuse strip_invisible_chars from actions.rs; make it pub(crate)). On request failure, emit the Caption with translated = "[translation failed]" so the pipeline never stalls. This stage may need a small tokio/async bridge since llm_client is async — use tauri::async_runtime::block_on inside the thread, mirroring patterns already in the codebase if present.

### A6 — pipeline lifecycle + emit stage
A PipelineHandle struct (thread JoinHandles + stop flag) stored in Tauri managed state behind a Mutex<Option<...>>. Two new Tauri commands registered in lib.rs collect_commands: `start_captions` and `stop_captions`. start builds PipelineChannels, spawns stages A2–A5 plus an emit thread that owns caption_rx and emits each Caption to the frontend as Tauri event "caption" (Caption already derives Serialize + specta::Type). stop sets the flag, drops senders so recv loops end, joins threads with a timeout, releases the device. Calling start twice is a no-op with a log warning; stop when not running is a no-op.

### B1 — minimal caption UI
In the existing main window frontend (src/), add a Captions view reachable without removing existing views: a Start/Stop button pair calling the new commands via the regenerated bindings, and a scrolling list of captions from listening to the "caption" event — newest at bottom, each row showing translated text prominently and original Spanish smaller/dimmer. Auto-scroll to newest. Keep styling consistent with the existing UI; no new dependencies. Do not prune any push-to-talk UI in this session (that is Phase C).

### B2 — smoke-test hooks
Add a `--captions-log` behavior: when the pipeline runs with debug logging enabled, log one line per caption (timestamps, original, translated) so terminal testing works without the UI. Also log per-stage timing (segment duration, transcribe ms, translate ms) at debug level — this is the latency budget instrumentation PLAN.md calls for.

## Acceptance (developer will run)

1. `cargo check` green, app launches via existing dev workflow.
2. With Ollama serving qwen3:8b and a Parakeet model downloaded: click Start, speak Spanish, pause — English caption appears within ~3 s. Several utterances accumulate correctly in order.
3. Stop releases the microphone (OS mic indicator goes off). Start again works.
4. Ollama stopped mid-run: captions show "[translation failed]", pipeline keeps running, no crash.
