# CLAUDE.md — live-captions

Instructions for Claude Code and any AI assistant working in this repo. Read fully before editing.

## What this project is

A local, offline live-captioning app. It listens to continuous audio, segments speech automatically with VAD, transcribes with Parakeet V3, translates Spanish→English with a local LLM (Ollama), and displays rolling translated captions in an always-on-top window.

Primary use case: the developer's video calls with his Spanish-speaking father. One machine, one direction (Spanish audio in → English captions on screen).

## Terminology rules (strict)

- This codebase was copied from the voice-translator project, which was **derived from Handy** (MIT license). Past tense, lineage only.
- NEVER use the words "fork" or "upstream" to describe the relationship to Handy or voice-translator. There is no upstream. No syncing, no merging from other repos, ever.
- The push-to-talk, paste, and keyboard-shortcut features of the ancestor codebase are **removed by design**. Do not reintroduce them, do not "fix" their absence.

## Current state (honest inventory)

Works today:
- Rust/Tauri app shell compiles clean (`cargo check`)
- Model manager: downloads and loads Parakeet V3
- Transcription manager: transcribes audio buffers
- Audio toolkit: cpal device capture, resampling, Silero VAD (silero_vad_v4.onnx in resources)
- LLM client: OpenAI-compatible chat completion against Ollama (localhost:11434/v1)
- Overlay window machinery (frameless, always-on-top) — currently shaped as the old recording pill

Built but dormant (do not delete):
- `tts.rs` — speak-aloud path, service-over-HTTP contract (POST {"text": ...} → WAV bytes). Future feature.
- Audio manager record/stop methods — awaiting the streaming loop as their new caller.

Not built yet:
- Continuous capture loop (the streaming pipeline)
- Caption display window and its frontend
- System-audio (WASAPI loopback) capture

Known debt:
- Frontend (src/) still contains push-to-talk UI: settings for shortcuts, paste options, references to deleted commands (e.g. initialize_enigo in bindings.ts). Frontend pruning is a planned phase. Rust-side settings fields for paste/post-process remain until that phase.
- Crate is still named `handy` in Cargo.toml; window title says VoiceTranslator. Rename is a deliberate, separate task — do not rename opportunistically.

## Architecture direction

Pipeline of stages connected by channels, each stage on its own thread:

capture → VAD segmentation → transcribe (Parakeet) → translate (Ollama) → caption window

- Chunks move by ownership transfer through channels (std::sync::mpsc or crossbeam). Shared state only via Arc where genuinely shared.
- Translation is whole-utterance, not word-by-word. Incremental/streaming translation is explicitly deferred to v2.
- Audio source is swappable: v0 uses microphone input; system-audio loopback is a later source behind the same interface.

## Working rules

- One bounded change per instruction package. `cargo check` must pass at the end of every package.
- The developer reviews every diff before commit. Do not chain unrequested changes.
- Prefer targeted edits over rewrites. Do not reformat untouched code.
- Distinguish "works today" from "built but dormant" in any docs you touch.
- No emojis in documentation.
