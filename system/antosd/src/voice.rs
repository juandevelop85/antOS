//! Local voice capture and transcription.
//!
//! Voice is not a different architecture: it's another way to write the same
//! text string. Once transcribed, it follows exactly the same path as if you
//! had typed it — plan, blast radius, permission level, diff and confirmation.
//!
//! That is deliberate and is what makes voice acceptable. Talking to a machine
//! that mutates your system only makes sense when being wrong is already cheap,
//! and that is why M3 comes after the enclosure and undo, not before.
//!
//! ## Why local
//!
//! Sending audio to a transcription service would undermine half the project's
//! premise: the microphone on your work machine captures much more than the
//! sentence you meant to say. Transcription runs entirely here.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Whisper works with 16 kHz, mono, 16-bit PCM audio. Anything else must be
/// converted first.
const SAMPLE_RATE: &str = "16000";

/// Below this mean level, it is considered that nobody has spoken.
///
/// The number comes from measurement, not assumption. On this machine:
///
/// | source                  | mean level  |
/// |-------------------------|-------------|
/// | digital silence         | −91 dB      |
/// | quiet room              | −47 dB      |
/// | someone speaking        | −18 dB      |
///
/// −40 dB leaves seven decibels of margin above room noise and twenty-two
/// below a voice.
const THRESHOLD_DB: f32 = -40.0;

pub struct Voice {
    whisper: PathBuf,
    model: PathBuf,
    language: String,
}

impl Voice {
    pub fn discover() -> Result<Self> {
        let whisper = match crate::util::env_with_legacy_fallback("ANTOS_WHISPER", "SYSO_WHISPER") {
            Some(path) => PathBuf::from(path),
            None => find_in_path(&["whisper-cli", "whisper-cpp"]).ok_or_else(|| {
                anyhow::anyhow!("cannot find whisper. Install with:\n  brew install whisper-cpp")
            })?,
        };

        let model =
            match crate::util::env_with_legacy_fallback("ANTOS_MODELO_VOZ", "SYSO_MODELO_VOZ") {
                Some(path) => PathBuf::from(path),
                None => default_model(),
            };
        if !model.exists() {
            bail!(
                "cannot find voice model at {}.\n\
                 Download with:\n  ./system/instalar-voz.sh",
                model.display()
            );
        }

        Ok(Voice {
            whisper,
            model,
            language: crate::util::env_with_legacy_fallback("ANTOS_IDIOMA_VOZ", "SYSO_IDIOMA_VOZ")
                .and_then(|v| v.into_string().ok())
                .unwrap_or_else(|| "es".into()),
        })
    }

    /// Records from the microphone for `seconds`.
    ///
    /// The first time, macOS will ask for microphone permission from the
    /// terminal where this is launched. That permission is granted by a person,
    /// not this program.
    pub fn record(&self, seconds: u32, destination: &Path, device: &str) -> Result<()> {
        let ffmpeg = find_in_path(&["ffmpeg"]).ok_or_else(|| {
            anyhow::anyhow!("cannot find ffmpeg. Install with:\n  brew install ffmpeg")
        })?;

        let mut cmd = Command::new(&ffmpeg);
        crate::sandbox::sin_secretos(&mut cmd);
        let output = cmd
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(["-f", "avfoundation", "-i", device])
            .args(["-t", &seconds.to_string()])
            .args(["-ar", SAMPLE_RATE, "-ac", "1", "-c:a", "pcm_s16le"])
            .arg("-y")
            .arg(destination)
            .output()
            .context("could not launch ffmpeg")?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!(
                "recording failed.\n{}\n\
                 If this is a permissions issue, macOS must authorize the \
                 microphone for your terminal in Settings > Privacy.",
                detail.trim()
            );
        }
        Ok(())
    }

    /// Converts any audio to the format expected by Whisper.
    pub fn normalize(&self, source: &Path, destination: &Path) -> Result<()> {
        let ffmpeg = find_in_path(&["ffmpeg"])
            .ok_or_else(|| anyhow::anyhow!("cannot find ffmpeg to convert the audio"))?;

        let mut cmd = Command::new(&ffmpeg);
        crate::sandbox::sin_secretos(&mut cmd);
        let output = cmd
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
            .arg(source)
            .args(["-ar", SAMPLE_RATE, "-ac", "1", "-c:a", "pcm_s16le"])
            .arg("-y")
            .arg(destination)
            .output()
            .context("could not launch ffmpeg")?;

        if !output.status.success() {
            bail!(
                "could not convert the audio: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    /// Mean audio level in decibels.
    fn level_db(&self, wav: &Path) -> Result<f32> {
        let ffmpeg =
            find_in_path(&["ffmpeg"]).ok_or_else(|| anyhow::anyhow!("cannot find ffmpeg"))?;

        let mut cmd = Command::new(&ffmpeg);
        crate::sandbox::sin_secretos(&mut cmd);
        let output = cmd
            .args(["-hide_banner", "-nostdin", "-i"])
            .arg(wav)
            .args(["-af", "volumedetect", "-f", "null", "-"])
            .output()
            .context("could not measure audio level")?;

        let text = String::from_utf8_lossy(&output.stderr);
        for line in text.lines() {
            if let Some((_, rest)) = line.split_once("mean_volume:") {
                if let Some(number) = rest.split_whitespace().next() {
                    return Ok(number.parse::<f32>().unwrap_or(f32::NEG_INFINITY));
                }
            }
        }
        // If we cannot measure, let it through: better to transcribe too much
        // than to refuse to listen because of a measurement failure.
        Ok(0.0)
    }

    pub fn transcribe(&self, wav: &Path, vocabulary: &str) -> Result<String> {
        // The most important gate in this entire module.
        //
        // Whisper, given background noise, does not say "I heard nothing": it
        // invents a perfectly formed sentence. Recording an empty room produced
        // "People can start a work project." — a plausible intent with which a
        // planner can build a real plan.
        //
        // Filtering silence markers is not enough because the hallucination is
        // not marked. We must refuse to transcribe what does not have enough
        // energy to be a voice.
        let threshold =
            crate::util::env_with_legacy_fallback("ANTOS_UMBRAL_VOZ", "SYSO_UMBRAL_VOZ")
                .and_then(|v| v.into_string().ok())
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(THRESHOLD_DB);

        let level = self.level_db(wav)?;
        if level < threshold {
            bail!(
                "no voice detected (mean level {level:.1} dB, threshold {threshold:.1} dB).\n\
                 If you were speaking, try another microphone:\n  antos listen --devices"
            );
        }

        let mut cmd = Command::new(&self.whisper);
        crate::sandbox::sin_secretos(&mut cmd);
        let output = cmd
            .args(["-m"])
            .arg(&self.model)
            .args(["-f"])
            .arg(wav)
            .args(["-l", &self.language])
            .args(["--no-timestamps", "--no-prints"])
            .args(["--prompt", vocabulary])
            .output()
            .context("could not launch whisper")?;

        if !output.status.success() {
            bail!(
                "transcription failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let clean = clean_transcription(&text);

        if clean.is_empty() {
            bail!("could not understand anything. Was the microphone active?");
        }
        Ok(clean)
    }

    /// Audio devices visible to macOS, with their index.
    pub fn devices() -> Result<String> {
        let ffmpeg =
            find_in_path(&["ffmpeg"]).ok_or_else(|| anyhow::anyhow!("cannot find ffmpeg"))?;

        let output = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-f",
                "avfoundation",
                "-list_devices",
                "true",
                "-i",
                "",
            ])
            .output()
            .context("could not launch ffmpeg")?;

        // ffmpeg lists devices via stderr and then "fails" on purpose because
        // there is nothing to open. This is not an error.
        let text = String::from_utf8_lossy(&output.stderr);
        let mut lines = Vec::new();
        let mut in_audio = false;
        for line in text.lines() {
            if line.contains("audio devices") {
                in_audio = true;
                continue;
            }
            if line.contains("video devices") {
                in_audio = false;
            }
            if in_audio {
                if let Some((_, rest)) = line.split_once("] ") {
                    if rest.starts_with('[') {
                        lines.push(rest.to_string());
                    }
                }
            }
        }
        Ok(lines.join("\n"))
    }

    pub fn model_path(&self) -> &Path {
        &self.model
    }
}

/// Whisper marks silences and noises with brackets or parentheses
/// — `[BLANK_AUDIO]`, `(background music)` — and that is not an intent.
fn clean_transcription(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !(line.starts_with('[') || line.starts_with('(')))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn find_in_path(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn default_model() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    // One-time migration (T31.11): an installation that downloaded its model
    // before the rename has it under `.cache/syso/modelos/`; move the whole
    // directory to `.cache/antos/modelos/` exactly once instead of checking
    // both paths on every call.
    let _ = crate::util::migrate_legacy_path(
        &base.join(".cache/syso/modelos"),
        &base.join(".cache/antos/modelos"),
    );
    base.join(".cache/antos/modelos/ggml-base.bin")
}
