# PLAN.md — live-captions roadmap

Bottom line: prove the streaming pipeline on microphone input first, then swap the audio source to system loopback, then polish the caption window. Two-device call app (Pipecat) is a separate future project that reuses this pipeline.

## User story (v0)

As the user, I start the app, click Start, and speak Spanish. Within ~2 seconds of pausing, an English caption of what I said appears in an always-on-top caption window. Captions accumulate as a rolling list. Clicking Stop ends capture. Everything runs locally; nothing leaves the machine.

Acceptance:
- No key held during speech. Segmentation is automatic (VAD).
- Caption latency after end of utterance: under ~3 s on the RTX 4070 laptop.
- App survives 10+ minutes of continuous operation without stalling or leaking.

## User story (v1)

As the user, I select "system audio" as the source and join a Google Meet call. My father speaks Spanish on his phone; English captions of his speech appear on my laptop. He installs nothing and changes nothing.

New work over v0: WASAPI loopback capture as a second audio source behind the same source interface. Everything downstream unchanged.

## Out of scope (parked, deliberately)

- Two-device call app with captions both directions (Pipecat) — separate project
- Speak-aloud TTS of translations — tts.rs stays dormant until after v1
- Incremental/streaming translation with self-revising captions — v2 experiment at earliest
- Mobile — blocked on owning the call app; see two-device project
- English→Spanish direction — trivial to add later, not needed for v0/v1

## Build phases

Phase A — pipeline skeleton (backend):
1. CaptionSegment type and channel plumbing between stages
2. Continuous capture thread feeding a ring buffer from the existing cpal stream
3. VAD segmentation stage: consume the stream, emit utterance-sized sample buffers on silence boundaries (reuse existing Silero VAD wrapper)
4. Transcribe stage: existing TranscriptionManager, called per segment
5. Translate stage: existing llm_client against Ollama qwen3:8b with a fixed es→en system prompt
6. Emit stage: Tauri event per caption to the frontend

Each step is one instruction package; cargo check green after each; developer reviews each diff.

Phase B — caption window (frontend):
1. Repurpose the overlay window machinery into a caption window (wider, bottom-of-screen, click-through optional)
2. Caption list UI: rolling captions, newest at bottom, original + translation
3. Start/Stop control and source picker in the main window

Phase C — frontend pruning:
Remove push-to-talk UI, shortcut/paste settings surfaces, and dead bindings. Deferred until B works so there is always a runnable app.

Phase D — v1 source swap:
WASAPI loopback capture behind the audio-source interface. Meet call field test with Dad.

## Performance notes

- Parakeet and VAD already run locally on this hardware in the ancestor app; the new cost is continuous VAD, which is lightweight.
- qwen3:8b translation of utterance-sized text is the latency budget item; measure before optimizing. If too slow, try smaller model or prompt trim before architectural changes.
