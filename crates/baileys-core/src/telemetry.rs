//! Observability and health telemetry subsystem for Rose-Baileys.
//!
//! Tracks:
//! - Inbound and outbound message throughput
//! - WebSocket ping/pong roundtrip latency
//! - Signal protocol session and prekey cache hit rates
//! - Reconnection attempts and reliability health

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// High-performance thread-safe atomic metrics counters.
#[derive(Debug, Default)]
pub struct TelemetryMetrics {
    pub messages_in: AtomicU64,
    pub messages_out: AtomicU64,
    pub ping_pong_latency_ms: AtomicU64,
    pub signal_session_cache_hits: AtomicU64,
    pub signal_session_cache_misses: AtomicU64,
    pub reconnect_count: AtomicU64,
}

impl TelemetryMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn record_msg_in(&self) {
        self.messages_in.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_msg_out(&self) {
        self.messages_out.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_ping_latency(&self, dur: Duration) {
        self.ping_pong_latency_ms
            .store(dur.as_millis() as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_signal_hit(&self) {
        self.signal_session_cache_hits
            .fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_signal_miss(&self) {
        self.signal_session_cache_misses
            .fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_reconnect(&self) {
        self.reconnect_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Captures a frozen snapshot of the current engine health metrics.
    pub fn snapshot(&self) -> TelemetrySnapshot {
        let hits = self.signal_session_cache_hits.load(Ordering::Relaxed);
        let misses = self.signal_session_cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate_pct = if total > 0 {
            (hits as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        TelemetrySnapshot {
            messages_in: self.messages_in.load(Ordering::Relaxed),
            messages_out: self.messages_out.load(Ordering::Relaxed),
            last_ping_latency_ms: self.ping_pong_latency_ms.load(Ordering::Relaxed),
            signal_cache_hit_rate_pct: (hit_rate_pct * 100.0).round() / 100.0,
            reconnect_count: self.reconnect_count.load(Ordering::Relaxed),
        }
    }
}

/// Frozen snapshot of engine telemetry metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub messages_in: u64,
    pub messages_out: u64,
    pub last_ping_latency_ms: u64,
    pub signal_cache_hit_rate_pct: f64,
    pub reconnect_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_snapshot_and_metrics() {
        let metrics = TelemetryMetrics::new();
        metrics.record_msg_in();
        metrics.record_msg_in();
        metrics.record_msg_out();
        metrics.record_ping_latency(Duration::from_millis(42));
        metrics.record_signal_hit();
        metrics.record_signal_hit();
        metrics.record_signal_hit();
        metrics.record_signal_miss();
        metrics.record_reconnect();

        let snap = metrics.snapshot();
        assert_eq!(snap.messages_in, 2);
        assert_eq!(snap.messages_out, 1);
        assert_eq!(snap.last_ping_latency_ms, 42);
        assert_eq!(snap.signal_cache_hit_rate_pct, 75.0);
        assert_eq!(snap.reconnect_count, 1);
    }
}
