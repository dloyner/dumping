//! DoS (Denial of Service)
//!
//! Big idea: exhaust a resource (bandwidth, CPU, memory, connections, DB) so real
//! users cannot get served. DDoS = the same from many machines.
//!   - Volumetric / protocol (L3/L4): flood, SYN flood. Handled at the edge
//!     (CDN, AWS Shield, firewall), not by app code.
//!   - Application (L7): Slowloris, expensive endpoints, huge bodies, ReDoS.
//!     Handled here: limits + timeouts + rate limiting + load shedding.
//!
//! Goal: raise the attacker's cost and degrade gracefully, not "be unbreakable".

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Router,
    extract::{ConnectInfo, DefaultBodyLimit, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use tower::limit::ConcurrencyLimitLayer;
use tower_http::timeout::TimeoutLayer;

/// Token bucket: allows short bursts, enforces an average rate.
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(capacity: f64, refill_per_sec: f64, now: Instant) -> Self {
        Self { capacity, tokens: capacity, refill_per_sec, last: now }
    }

    /// `now` is a parameter so tests do not need to sleep.
    pub fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Per-IP rate limiter.
/// Caveat: the map itself grows with every new IP, which is a memory DoS vector.
/// Production code evicts idle entries (or uses the `governor` crate).
/// Also: many real users can share one IP (NAT), so combine with other layers.
#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<IpAddr, TokenBucket>>>,
    capacity: f64,
    refill_per_sec: f64,
}

impl RateLimiter {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self { buckets: Arc::default(), capacity, refill_per_sec }
    }

    pub fn check(&self, ip: IpAddr, now: Instant) -> bool {
        let mut map = self.buckets.lock().expect("poisoned");
        map.entry(ip)
            .or_insert_with(|| TokenBucket::new(self.capacity, self.refill_per_sec, now))
            .allow(now)
    }
}

/// Needs `into_make_service_with_connect_info::<SocketAddr>()` (see main.rs).
/// Behind a reverse proxy you must use the forwarded client IP instead.
pub async fn rate_limit(
    State(limiter): State<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if limiter.check(addr.ip(), Instant::now()) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

/// Static limits: body size, total request time, concurrent requests.
/// Production also wants header/idle timeouts (Slowloris) at the reverse proxy
/// and load shedding (return 503 instead of queueing forever).
pub fn with_limits(router: Router) -> Router {
    router
        .layer(DefaultBodyLimit::max(1024 * 1024)) // 1 MiB
        .layer(ConcurrencyLimitLayer::new(64))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(10),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_allows_burst_then_blocks_then_refills() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(2.0, 1.0, t0);
        assert!(b.allow(t0));
        assert!(b.allow(t0));
        assert!(!b.allow(t0)); // burst used up
        assert!(b.allow(t0 + Duration::from_secs(1))); // 1 token refilled
    }

    #[test]
    fn limits_are_per_ip() {
        let now = Instant::now();
        let rl = RateLimiter::new(1.0, 0.0);
        let (a, b): (IpAddr, IpAddr) = ("1.1.1.1".parse().unwrap(), "2.2.2.2".parse().unwrap());
        assert!(rl.check(a, now));
        assert!(!rl.check(a, now));
        assert!(rl.check(b, now)); // other IP is unaffected
    }
}
