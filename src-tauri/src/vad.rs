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

/// Voice Activity Detector using WebRTC VAD.
pub struct VadDetector {
    vad: Vad,
    state: VadState,
    error: Option<String>,
}

impl VadDetector {
    /// Creates a new VAD detector at 16 kHz sample rate.
    ///
    /// Uses `LowBitrate` aggressiveness instead of the default `Quality`
    /// mode: the downmixed mic signal carries ambient noise, and Quality mode
    /// fires on 0.1-0.3 s blips that the model turns into random fragments.
    pub fn new() -> Self {
        VadDetector {
            vad: Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::LowBitrate),
            state: VadState::Silent,
            error: None,
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

        let mut frame_i16 = vec![0i16; FRAME_SIZE];
        for (i, &sample) in pcm_samples.iter().take(FRAME_SIZE).enumerate() {
            let clamped = sample.max(-1.0).min(1.0);
            frame_i16[i] = (clamped * 32767.0) as i16;
        }

        let is_voice = match self.vad.is_voice_segment(&frame_i16) {
            Ok(voice) => voice,
            Err(_) => {
                self.error = Some("VAD processing error".to_string());
                return VadState::Silent;
            }
        };

        match self.state {
            VadState::Silent => {
                if is_voice {
                    self.state = VadState::Speeching;
                }
            }
            VadState::Speeching => {
                if !is_voice {
                    self.state = VadState::Boundary;
                }
            }
            VadState::Boundary => {
                self.state = VadState::Silent;
            }
        }

        self.state
    }

    /// Resets the detector state (e.g., after a transcription is complete).
    pub fn reset(&mut self) {
        if self.error.is_none() {
            self.state = VadState::Silent;
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
}
