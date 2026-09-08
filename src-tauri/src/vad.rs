use webrtc_vad::{SampleRate, Vad, VadMode};

// Safety: needed to satisfy cpal's `Send + 'static` stream-callback bound,
// which cannot otherwise be met: `webrtc_vad::Vad` (0.4.0) wraps a raw
// `*mut Fvad` and does not implement `Send`. The argument is ownership-based
// (F-25), not convention-based: `VadDetector` is constructed once, moved into
// exactly one cpal callback, and the `Fvad` handle has no interior
// indirections, callbacks, or thread-local state. cpal invokes that callback
// serially on a single audio thread, and the stream is stopped before the
// callback (and the detector with it) is dropped, so the handle is never
// aliased or used concurrently.
unsafe impl Send for VadDetector {}

/// VAD detection state for utterance boundary detection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VadState {
    /// Currently detecting speech.
    Speeching,
    /// Currently in silence (no speech detected).
    Silent,
    /// Transition from speech to silence (utterance end).
    Boundary,
}

/// Minimum RMS (samples in -1..=1) for a frame to be trusted as potentially
/// containing speech. The WebRTC model keeps answering "voice" for quiet
/// frames once it has seen signal, so near-silence is gated on energy before
/// the VAD decision is believed.
const MIN_ENERGY_RMS: f32 = 0.004;

/// Consecutive voice frames required before speech starts (30 ms at 10 ms
/// frames). A single frame flips on noise blips; a run is speech onset.
const VOICE_CONFIRM_FRAMES: u32 = 3;

/// Consecutive non-voice frames required before an utterance ends (150 ms).
/// Keeps natural intra-word pauses and the tail of the last word inside the
/// utterance instead of fragmenting it.
const SILENCE_END_FRAMES: u32 = 15;

/// Consecutive-frame debounce state for the VAD state machine.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
struct VadCounters {
    voice_run: u32,
    silence_run: u32,
}

/// Advances the VAD state machine with debouncing. Pure: `is_voice` is the
/// caller's (already energy-gated) decision for the current frame.
fn next_vad_state(state: VadState, counters: &mut VadCounters, is_voice: bool) -> VadState {
    if is_voice {
        counters.voice_run = counters.voice_run.saturating_add(1);
        counters.silence_run = 0;
    } else {
        counters.voice_run = 0;
        counters.silence_run = counters.silence_run.saturating_add(1);
    }

    match state {
        VadState::Silent => {
            if counters.voice_run >= VOICE_CONFIRM_FRAMES {
                VadState::Speeching
            } else {
                VadState::Silent
            }
        }
        VadState::Speeching => {
            if counters.silence_run >= SILENCE_END_FRAMES {
                VadState::Boundary
            } else {
                VadState::Speeching
            }
        }
        // Boundary is one frame long; the next frame returns to Silent with
        // fresh counters so a stale partial run cannot confirm a new onset.
        VadState::Boundary => {
            counters.voice_run = 0;
            counters.silence_run = 0;
            VadState::Silent
        }
    }
}

/// Root mean square of a frame of -1..=1 samples.
fn frame_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Voice Activity Detector using WebRTC VAD with an energy gate and
/// consecutive-frame debouncing, so ambient noise and quiet frames that the
/// raw model still classifies as voice cannot register as speech.
pub struct VadDetector {
    vad: Vad,
    state: VadState,
    error: Option<String>,
    counters: VadCounters,
}

impl VadDetector {
    /// Creates a new VAD detector at 16 kHz sample rate.
    ///
    /// Uses `LowBitrate` aggressiveness instead of the default `Quality`
    /// mode: the downmixed mic signal carries ambient noise, and Quality mode
    /// fires on 0.1-0.3 s blips that the model turns into random fragments.
    /// The crate (webrtc-vad 0.4) exposes no more aggressive mode, so the
    /// noise rejection lives in the energy gate and frame debouncing.
    pub fn new() -> Self {
        VadDetector {
            vad: Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::LowBitrate),
            state: VadState::Silent,
            error: None,
            counters: VadCounters::default(),
        }
    }

    /// Returns true if the VAD has encountered a fatal error.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Returns the VAD error message, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Processes a frame of audio samples and returns the current VAD state.
    /// Frame size should be 160 samples (10ms at 16 kHz).
    pub fn process_frame(&mut self, pcm_samples: &[f32]) -> VadState {
        const FRAME_SIZE: usize = 160;

        if pcm_samples.len() < FRAME_SIZE {
            return self.state;
        }
        let frame = &pcm_samples[..FRAME_SIZE];

        // Gate the VAD decision on energy; without this, quiet frames are
        // reported as voice by the raw model once it has seen signal.
        let energy_ok = frame_rms(frame) >= MIN_ENERGY_RMS;

        let mut frame_i16 = vec![0i16; FRAME_SIZE];
        for (i, &sample) in frame.iter().enumerate() {
            let clamped = sample.max(-1.0).min(1.0);
            frame_i16[i] = (clamped * 32767.0) as i16;
        }

        let vad_voice = match self.vad.is_voice_segment(&frame_i16) {
            Ok(voice) => voice,
            Err(_) => {
                self.error = Some("VAD processing error".to_string());
                return VadState::Silent;
            }
        };

        self.state = next_vad_state(self.state, &mut self.counters, vad_voice && energy_ok);
        self.state
    }

    /// Resets the detector state (e.g., after a transcription is complete).
    pub fn reset(&mut self) {
        if self.error.is_none() {
            self.state = VadState::Silent;
            self.counters = VadCounters::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_detector_creation() {
        let detector = VadDetector::new();
        assert_eq!(detector.state, VadState::Silent);
        assert!(!detector.is_error());
        assert!(detector.error().is_none());
    }

    #[test]
    fn test_vad_reset_no_error() {
        let mut detector = VadDetector::new();
        detector.reset();
        assert_eq!(detector.state, VadState::Silent);
    }

    #[test]
    fn test_vad_process_frame_silent_input() {
        let mut detector = VadDetector::new();
        let silence = vec![0.0f32; 160];
        let state = detector.process_frame(&silence);
        assert!(matches!(state, VadState::Silent | VadState::Speeching));
    }

    #[test]
    fn test_vad_error_state() {
        let mut detector = VadDetector::new();
        let tone: Vec<f32> = (0..160)
            .map(|i| ((i as f32 * 0.1).sin() * 0.5 + 0.5).max(-1.0).min(1.0))
            .collect();
        let _ = detector.process_frame(&tone);
        assert!(!detector.is_error());
    }

    #[test]
    fn test_frame_rms_zero_and_full_scale() {
        assert_eq!(frame_rms(&[0.0; 160]), 0.0);
        assert!((frame_rms(&[0.5; 160]) - 0.5).abs() < 1e-6);
        assert_eq!(frame_rms(&[]), 0.0);
    }

    #[test]
    fn test_speech_start_requires_voice_run() {
        let mut counters = VadCounters::default();
        assert_eq!(
            next_vad_state(VadState::Silent, &mut counters, true),
            VadState::Silent
        );
        assert_eq!(
            next_vad_state(VadState::Silent, &mut counters, true),
            VadState::Silent
        );
        // The third consecutive voice frame confirms the onset.
        assert_eq!(
            next_vad_state(VadState::Silent, &mut counters, true),
            VadState::Speeching
        );
    }

    #[test]
    fn test_alternating_blips_never_reach_speech() {
        let mut counters = VadCounters::default();
        for _ in 0..10 {
            assert_eq!(
                next_vad_state(VadState::Silent, &mut counters, true),
                VadState::Silent
            );
            assert_eq!(
                next_vad_state(VadState::Silent, &mut counters, false),
                VadState::Silent
            );
        }
    }

    #[test]
    fn test_speech_end_requires_silence_run() {
        let mut counters = VadCounters::default();
        let mut state = VadState::Silent;
        for _ in 0..VOICE_CONFIRM_FRAMES {
            state = next_vad_state(state, &mut counters, true);
        }
        assert_eq!(state, VadState::Speeching);
        for i in 1..SILENCE_END_FRAMES {
            state = next_vad_state(state, &mut counters, false);
            assert_eq!(state, VadState::Speeching, "boundary too early at frame {i}");
        }
        state = next_vad_state(state, &mut counters, false);
        assert_eq!(state, VadState::Boundary);
        state = next_vad_state(state, &mut counters, true);
        assert_eq!(state, VadState::Silent);
    }

    #[test]
    fn test_voice_during_speech_resets_silence_run() {
        let mut counters = VadCounters::default();
        let mut state = VadState::Silent;
        for _ in 0..VOICE_CONFIRM_FRAMES {
            state = next_vad_state(state, &mut counters, true);
        }
        assert_eq!(state, VadState::Speeching);
        for _ in 0..(SILENCE_END_FRAMES - 1) {
            state = next_vad_state(state, &mut counters, false);
        }
        // A voice frame resets the silence run, so the utterance keeps going.
        assert_eq!(
            next_vad_state(state, &mut counters, true),
            VadState::Speeching
        );
        state = VadState::Speeching;
        for _ in 0..(SILENCE_END_FRAMES - 1) {
            assert_eq!(
                next_vad_state(state, &mut counters, false),
                VadState::Speeching
            );
            state = VadState::Speeching;
        }
        assert_eq!(
            next_vad_state(state, &mut counters, false),
            VadState::Boundary
        );
    }

    #[test]
    fn test_energy_gate_treats_quiet_frame_as_non_voice() {
        // A near-silent frame (RMS well below the energy gate) must stay
        // Silent regardless of what the raw VAD model reports.
        let mut detector = VadDetector::new();
        let quiet = vec![0.0001f32; 160];
        for _ in 0..10 {
            assert_eq!(detector.process_frame(&quiet), VadState::Silent);
        }
    }

    #[test]
    fn test_reset_clears_counters() {
        let mut detector = VadDetector::new();
        let loud = vec![0.5f32; 160];
        for _ in 0..VOICE_CONFIRM_FRAMES {
            detector.process_frame(&loud);
        }
        assert_eq!(detector.state, VadState::Speeching);
        detector.reset();
        assert_eq!(detector.state, VadState::Silent);
        // Resetting the counters drops the partial voice run, so a single
        // voice frame does not immediately restart speech.
        assert_eq!(detector.process_frame(&loud), VadState::Silent);
    }
}
