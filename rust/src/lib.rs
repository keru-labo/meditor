//! GUI-thread state for routing seek requests and accepting decoder results.

#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekMode {
    Scrub { proxy_available: bool },
    Settle { resume_playback: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoSource {
    Original,
    Proxy,
}

/// A decoder returns this request along with its frame on the GUI thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekRequest {
    serial: u64,
    position_us: u64,
    mode: SeekMode,
}

impl SeekRequest {
    pub fn position_us(self) -> u64 {
        self.position_us
    }

    pub fn source(self) -> VideoSource {
        match self.mode {
            SeekMode::Scrub {
                proxy_available: true,
            } => VideoSource::Proxy,
            _ => VideoSource::Original,
        }
    }

    pub fn resumes_playback(self) -> bool {
        matches!(
            self.mode,
            SeekMode::Settle {
                resume_playback: true,
            }
        )
    }
}

/// Keep one instance for the lifetime of the editor on the GUI thread.
/// Invalidate pending work when stopping or switching media.
#[derive(Debug, Default)]
pub struct SeekController {
    serial: u64,
    pending: Option<SeekRequest>,
}

impl SeekController {
    /// Registers the latest intent before dispatching it to the decoder.
    pub fn request(&mut self, position_us: u64, mode: SeekMode) -> SeekRequest {
        self.serial = self.serial.checked_add(1).expect("seek serial exhausted");
        let request = SeekRequest {
            serial: self.serial,
            position_us,
            mode,
        };
        self.pending = Some(request);
        request
    }

    pub fn invalidate(&mut self) {
        self.pending = None;
    }

    /// Call immediately before presenting on the GUI thread. A frame with an
    /// obsolete request or a duplicate completion must not be presented.
    pub fn accept_frame(&mut self, request: SeekRequest) -> bool {
        if self.pending != Some(request) {
            return false;
        }
        self.pending = None;
        true
    }
}
