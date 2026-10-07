//! Per-request phase timing for proxy diagnostics.
//!
//! The counters separate deterministic local preparation from upstream waits.
//! They store durations, attempt counts, and the mapped model only; request
//! bodies, credentials, and URLs never enter the timing record.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub struct ProxyPhaseTimings {
    context_ms: AtomicU64,
    request_prepare_ms: AtomicU64,
    upstream_headers_ms: AtomicU64,
    stream_first_chunk_ms: AtomicU64,
    semantic_prime_ms: AtomicU64,
    attempts: AtomicU64,
    outbound_model: Mutex<Option<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyPhaseSnapshot {
    pub context_ms: u64,
    pub request_prepare_ms: u64,
    pub upstream_headers_ms: u64,
    pub stream_first_chunk_ms: u64,
    pub semantic_prime_ms: u64,
    pub attempts: u64,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    RequestPrepare,
    UpstreamHeaders,
    StreamFirstChunk,
    SemanticPrime,
}

pub struct PhaseTimer<'a> {
    timings: &'a ProxyPhaseTimings,
    phase: Phase,
    started: Instant,
}

impl Drop for PhaseTimer<'_> {
    fn drop(&mut self) {
        self.timings.add(self.phase, self.started.elapsed());
    }
}

impl ProxyPhaseTimings {
    pub fn set_context(&self, elapsed: Duration) {
        self.context_ms
            .store(duration_ms(elapsed), Ordering::Relaxed);
    }

    pub fn record_attempt(&self) {
        self.attempts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_outbound_model(&self, model: &str) {
        if let Ok(mut outbound_model) = self.outbound_model.lock() {
            *outbound_model = Some(model.to_string());
        }
    }

    pub fn outbound_model(&self) -> Option<String> {
        self.outbound_model.lock().ok()?.clone()
    }

    pub fn request_prepare_timer(&self) -> PhaseTimer<'_> {
        self.timer(Phase::RequestPrepare)
    }

    pub fn upstream_headers_timer(&self) -> PhaseTimer<'_> {
        self.timer(Phase::UpstreamHeaders)
    }

    pub fn stream_first_chunk_timer(&self) -> PhaseTimer<'_> {
        self.timer(Phase::StreamFirstChunk)
    }

    pub fn semantic_prime_timer(&self) -> PhaseTimer<'_> {
        self.timer(Phase::SemanticPrime)
    }

    pub fn snapshot(&self) -> ProxyPhaseSnapshot {
        ProxyPhaseSnapshot {
            context_ms: self.context_ms.load(Ordering::Relaxed),
            request_prepare_ms: self.request_prepare_ms.load(Ordering::Relaxed),
            upstream_headers_ms: self.upstream_headers_ms.load(Ordering::Relaxed),
            stream_first_chunk_ms: self.stream_first_chunk_ms.load(Ordering::Relaxed),
            semantic_prime_ms: self.semantic_prime_ms.load(Ordering::Relaxed),
            attempts: self.attempts.load(Ordering::Relaxed),
        }
    }

    pub fn log(
        &self,
        app_type: &str,
        session_id: &str,
        total_ms: u64,
        first_output_ms: Option<u64>,
        outcome: &str,
    ) {
        let phase = self.snapshot();
        let outbound_model = self
            .outbound_model()
            .unwrap_or_else(|| "unknown".to_string());
        let after_first_ms = first_output_ms.map(|first| total_ms.saturating_sub(first));
        log::info!(
            "[PERF] app={app_type} session={session_id} outcome={outcome} attempts={} outbound_model={outbound_model} context_ms={} request_prepare_ms={} upstream_headers_ms={} stream_first_chunk_ms={} semantic_prime_ms={} first_output_ms={} after_first_ms={} total_ms={total_ms}",
            phase.attempts,
            phase.context_ms,
            phase.request_prepare_ms,
            phase.upstream_headers_ms,
            phase.stream_first_chunk_ms,
            phase.semantic_prime_ms,
            optional_ms(first_output_ms),
            optional_ms(after_first_ms),
        );
    }

    fn timer(&self, phase: Phase) -> PhaseTimer<'_> {
        PhaseTimer {
            timings: self,
            phase,
            started: Instant::now(),
        }
    }

    fn add(&self, phase: Phase, elapsed: Duration) {
        let target = match phase {
            Phase::RequestPrepare => &self.request_prepare_ms,
            Phase::UpstreamHeaders => &self.upstream_headers_ms,
            Phase::StreamFirstChunk => &self.stream_first_chunk_ms,
            Phase::SemanticPrime => &self.semantic_prime_ms,
        };
        target.fetch_add(duration_ms(elapsed), Ordering::Relaxed);
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn optional_ms(value: Option<u64>) -> String {
    value.map_or_else(|| "none".to_string(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_accumulates_attempt_phases() {
        let timings = ProxyPhaseTimings::default();
        timings.set_context(Duration::from_millis(3));
        timings.record_attempt();
        timings.record_attempt();
        timings.add(Phase::RequestPrepare, Duration::from_millis(5));
        timings.add(Phase::RequestPrepare, Duration::from_millis(7));
        timings.add(Phase::UpstreamHeaders, Duration::from_millis(11));
        timings.add(Phase::StreamFirstChunk, Duration::from_millis(13));
        timings.add(Phase::SemanticPrime, Duration::from_millis(17));
        timings.set_outbound_model("gpt-5.6-terra");

        assert_eq!(timings.outbound_model().as_deref(), Some("gpt-5.6-terra"));
        assert_eq!(
            timings.snapshot(),
            ProxyPhaseSnapshot {
                context_ms: 3,
                request_prepare_ms: 12,
                upstream_headers_ms: 11,
                stream_first_chunk_ms: 13,
                semantic_prime_ms: 17,
                attempts: 2,
            }
        );
    }
}
