//! Certificates for an app's own domains.
//!
//! Each domain in `auto` mode gets an ACME client of its own in Angie, validated
//! over HTTP-01. One client per domain rather than per app because a
//! certificate order fails as a whole: a single domain whose DNS does not point
//! here yet would otherwise keep every other domain of the app on HTTP.
//!
//! This module holds the model: the modes an operator chooses, the states a
//! domain moves through, and the names and timings both sides agree on.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// What the operator asked for a domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsMode {
    /// Angie requests and renews a certificate.
    Auto,
    /// The operator installed the certificate files.
    Manual,
    /// Served over HTTP only.
    Off,
}

impl TlsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Off => "off",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "manual" => Some(Self::Manual),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    /// The state a domain starts in when it is put into this mode.
    pub fn initial_state(self) -> CertState {
        match self {
            Self::Auto => CertState::Pending,
            Self::Manual => CertState::Manual,
            Self::Off => CertState::Off,
        }
    }
}

/// Where a domain is in being served over HTTPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CertState {
    /// `auto`, but nothing has been tried: custom domain certificates are off,
    /// or the domain was just added.
    Pending,
    /// The reachability check is deciding whether a request can succeed.
    Checking,
    /// The domain's client is in Angie's configuration and a certificate has
    /// been requested.
    Issuing,
    /// A valid certificate is being served.
    Active,
    /// The last requests failed; `details` says why, and `next_retry_at` when
    /// Deku tries again.
    Failed,
    /// Served with the operator's certificate.
    Manual,
    /// Served over HTTP by choice.
    Off,
}

impl CertState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Checking => "checking",
            Self::Issuing => "issuing",
            Self::Active => "active",
            Self::Failed => "failed",
            Self::Manual => "manual",
            Self::Off => "off",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "checking" => Some(Self::Checking),
            "issuing" => Some(Self::Issuing),
            "active" => Some(Self::Active),
            "failed" => Some(Self::Failed),
            "manual" => Some(Self::Manual),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

/// A domain with its certificate state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DomainTls {
    pub domain_id: String,
    pub app_id: String,
    pub domain: String,
    pub mode: TlsMode,
    pub state: CertState,
    pub details: Option<String>,
    pub failures: u32,
    pub expires_at: Option<DateTime<Utc>>,
    pub checked_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    /// Angie's `next_run` when the last failure was counted.
    pub observed_run: Option<String>,
}

/// The prefix of every per-domain client, kept apart from the wildcard client.
pub const CLIENT_PREFIX: &str = "deku_d_";

/// The name of the ACME client that holds `domain`'s certificate.
///
/// Derived from the domain alone, so the client and the certificate Angie keeps
/// under its name follow a domain from one app to another. Domain names may
/// contain characters a client name cannot, and an Angie variable is named
/// after the client, so the name is a hash rather than the domain itself.
pub fn client_name(domain: &str) -> String {
    let digest = Sha256::digest(domain.trim().to_ascii_lowercase().as_bytes());
    format!("{CLIENT_PREFIX}{}", &hex::encode(digest)[..16])
}

/// The ACME account key every per-domain client shares.
///
/// Angie creates it on first use. One account for all of them keeps adding many
/// domains at once from running into the certificate authority's limit on new
/// accounts.
pub fn account_key_path(acme: &crate::config::AcmeConfig) -> std::path::PathBuf {
    acme.client_path.join("deku-account.key")
}

/// How long Angie waits before retrying a failed request on its own.
///
/// Its default is two hours. The first request after a reload can fail only
/// because an exiting worker answered the challenge, so a short wait recovers
/// from that quickly; a real failure is handled by [`retry_delay`] instead.
pub const ANGIE_RETRY_AFTER_ERROR: &str = "15m";

/// Failures in a row after which Deku stops Angie from retrying a domain.
///
/// A reload re-requests every missing certificate at once, ignoring Angie's own
/// retry delay, and every deploy reloads. A domain that keeps failing would run
/// into the certificate authority's failed-validation limit within a few
/// deploys, so after this many failures its client is turned off and Deku owns
/// the retry schedule.
pub const FAILURES_BEFORE_PAUSE: u32 = 2;

/// How long to wait before checking a paused domain again, by how many times it
/// has already been retried.
pub fn retry_delay(retries: u32) -> Duration {
    const HOUR: u64 = 3600;
    match retries {
        0 => Duration::from_secs(HOUR),
        1 => Duration::from_secs(4 * HOUR),
        _ => Duration::from_secs(24 * HOUR),
    }
}

/// Whether `domain` can get a certificate over HTTP-01 at all.
///
/// A wildcard needs DNS-01, and an IP address is not a name a domain-validated
/// certificate is issued for, so both stay manual.
pub fn supports_http_challenge(domain: &str) -> bool {
    let domain = domain.trim();
    !domain.starts_with("*.") && domain.parse::<std::net::IpAddr>().is_err() && domain.contains('.')
}

/// What Deku saw about a domain on one pass of the watcher.
#[derive(Debug, Default)]
pub struct Observation<'a> {
    /// Angie's report for the domain's client, when Angie answered and the
    /// client is loaded.
    pub angie: Option<&'a crate::acme::status::ClientStatus>,
    /// Whether the client's certificate file holds a certificate.
    pub issued: bool,
    pub expires_at: Option<DateTime<Utc>>,
    /// The reason Angie's last failed request gave.
    pub failure: Option<String>,
    /// The result of a reachability check run on this pass.
    pub reachability: Option<Result<(), String>>,
}

/// Where a domain moves to, when it moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub state: CertState,
    pub details: Option<String>,
    pub failures: u32,
    pub expires_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub observed_run: Option<String>,
}

impl Transition {
    fn from(current: &DomainTls) -> Self {
        Self {
            state: current.state,
            details: current.details.clone(),
            failures: current.failures,
            expires_at: current.expires_at,
            next_retry_at: current.next_retry_at,
            observed_run: current.observed_run.clone(),
        }
    }
}

/// How soon a domain that does not reach this server is checked again.
///
/// The check costs the certificate authority nothing, so it repeats quickly at
/// first, while DNS is likely still propagating, and settles at hourly.
pub fn recheck_delay(failed_checks: u32) -> Duration {
    let minutes = 5u64.saturating_mul(1 << failed_checks.min(4));
    Duration::from_secs(minutes.min(60) * 60)
}

/// Whether the watcher should run the reachability check for `domain` now.
pub fn wants_check(domain: &DomainTls, now: DateTime<Utc>) -> bool {
    match domain.state {
        CertState::Pending => true,
        CertState::Checking | CertState::Failed => domain.next_retry_at.is_none_or(|at| at <= now),
        _ => false,
    }
}

/// The next state of an `auto` domain, or `None` when nothing changed.
///
/// - `pending` and `checking` wait for the reachability check; a pass moves to
///   `issuing`, where the domain's client is written and Angie requests.
/// - `issuing` moves to `active` once Angie holds a valid certificate. Each
///   failed attempt is counted once; after [`FAILURES_BEFORE_PAUSE`] the domain
///   is `failed`, its client is turned off, and Deku schedules the retry.
/// - `failed` goes back through the check when its retry is due.
/// - `active` falls back to `issuing` if the certificate disappears or expires.
pub fn advance(
    current: &DomainTls,
    seen: &Observation<'_>,
    now: DateTime<Utc>,
) -> Option<Transition> {
    if current.mode != TlsMode::Auto {
        return None;
    }
    let mut next = Transition::from(current);

    match current.state {
        CertState::Pending | CertState::Checking | CertState::Failed => match &seen.reachability {
            Some(Ok(())) => {
                next.state = CertState::Issuing;
                next.details = None;
                next.next_retry_at = None;
                if current.state != CertState::Failed {
                    next.failures = 0;
                }
            }
            Some(Err(reason)) => {
                // A failed check never reached the certificate authority, so it
                // counts against the check's schedule, not the domain's failures.
                let checks = if current.state == CertState::Checking {
                    current.failures + 1
                } else {
                    1
                };
                if current.state != CertState::Failed {
                    next.state = CertState::Checking;
                    next.failures = checks;
                }
                next.details = Some(reason.clone());
                next.next_retry_at = Some(now + to_chrono(recheck_delay(checks.saturating_sub(1))));
            }
            None => return None,
        },
        CertState::Issuing => {
            let angie = seen.angie?;
            if angie.has_valid_certificate() && seen.issued {
                next.state = CertState::Active;
                next.details = None;
                next.failures = 0;
                next.expires_at = seen.expires_at;
                next.next_retry_at = None;
                next.observed_run = None;
            } else if angie.failed() && angie.next_run != current.observed_run {
                next.failures = current.failures + 1;
                next.observed_run.clone_from(&angie.next_run);
                next.details = Some(seen.failure.clone().unwrap_or_else(|| {
                    "The certificate authority refused the request.".to_string()
                }));
                if next.failures >= FAILURES_BEFORE_PAUSE {
                    next.state = CertState::Failed;
                    let retries = next.failures - FAILURES_BEFORE_PAUSE;
                    next.next_retry_at = Some(now + to_chrono(retry_delay(retries)));
                }
            } else {
                return None;
            }
        }
        CertState::Active => {
            let lost = !seen.issued || seen.expires_at.is_some_and(|expiry| expiry <= now);
            if lost {
                next.state = CertState::Issuing;
                next.details = Some(
                    "The certificate is missing or expired; requesting a new one.".to_string(),
                );
            } else if seen.expires_at.is_some() && seen.expires_at != current.expires_at {
                // Renewed: Angie replaced the file with a later expiry.
                next.expires_at = seen.expires_at;
            } else {
                return None;
            }
        }
        CertState::Manual | CertState::Off => return None,
    }

    (next != Transition::from(current)).then_some(next)
}

fn to_chrono(duration: Duration) -> chrono::TimeDelta {
    chrono::TimeDelta::from_std(duration).unwrap_or(chrono::TimeDelta::MAX)
}

#[cfg(test)]
mod tests {
    use super::{
        advance, client_name, recheck_delay, retry_delay, supports_http_challenge, wants_check,
        CertState, DomainTls, Observation, TlsMode, CLIENT_PREFIX,
    };
    use crate::acme::status::ClientStatus;
    use chrono::{TimeDelta, Utc};
    use std::time::Duration;

    fn domain(state: CertState, failures: u32) -> DomainTls {
        DomainTls {
            domain_id: "d1".to_string(),
            app_id: "a1".to_string(),
            domain: "shop.example.com".to_string(),
            mode: TlsMode::Auto,
            state,
            details: None,
            failures,
            expires_at: None,
            checked_at: None,
            next_retry_at: None,
            observed_run: None,
        }
    }

    fn angie(state: &str, certificate: &str, next_run: Option<&str>) -> ClientStatus {
        ClientStatus {
            state: state.to_string(),
            certificate: certificate.to_string(),
            details: None,
            next_run: next_run.map(ToString::to_string),
        }
    }

    #[test]
    fn a_reachable_domain_starts_issuing_and_an_unreachable_one_waits() {
        let now = Utc::now();
        let reachable = Observation {
            reachability: Some(Ok(())),
            ..Observation::default()
        };
        let next = advance(&domain(CertState::Pending, 0), &reachable, now).expect("moves");
        assert_eq!(next.state, CertState::Issuing);

        let unreachable = Observation {
            reachability: Some(Err("No DNS record for shop.example.com".to_string())),
            ..Observation::default()
        };
        let next = advance(&domain(CertState::Pending, 0), &unreachable, now).expect("moves");
        assert_eq!(
            next.state,
            CertState::Checking,
            "the authority was never asked"
        );
        assert_eq!(
            next.details.as_deref(),
            Some("No DNS record for shop.example.com")
        );
        assert_eq!(next.next_retry_at, Some(now + TimeDelta::minutes(5)));

        assert_eq!(
            advance(&domain(CertState::Pending, 0), &Observation::default(), now),
            None,
            "nothing happens until the check has run"
        );
    }

    #[test]
    fn one_failed_attempt_is_counted_once_and_two_pause_the_domain() {
        let now = Utc::now();
        let failed = angie("failed", "missing", Some("2026-10-16T00:15:00Z"));
        let seen = Observation {
            angie: Some(&failed),
            failure: Some("connection refused".to_string()),
            ..Observation::default()
        };
        let first = advance(&domain(CertState::Issuing, 0), &seen, now).expect("first failure");
        assert_eq!((first.state, first.failures), (CertState::Issuing, 1));
        assert_eq!(first.details.as_deref(), Some("connection refused"));

        // The same failure seen again on the next pass is not a new one.
        let mut after_first = domain(CertState::Issuing, 1);
        after_first.observed_run = first.observed_run.clone();
        assert_eq!(advance(&after_first, &seen, now), None);

        // Angie tried again and failed again: the domain pauses.
        let again = angie("failed", "missing", Some("2026-10-16T00:30:00Z"));
        let seen_again = Observation {
            angie: Some(&again),
            ..Observation::default()
        };
        let paused = advance(&after_first, &seen_again, now).expect("second failure");
        assert_eq!((paused.state, paused.failures), (CertState::Failed, 2));
        assert_eq!(paused.next_retry_at, Some(now + TimeDelta::hours(1)));
    }

    #[test]
    fn an_issued_certificate_makes_the_domain_active() {
        let now = Utc::now();
        let valid = angie("ready", "valid", Some("2026-12-14T22:00:00Z"));
        let expiry = now + TimeDelta::days(90);
        let seen = Observation {
            angie: Some(&valid),
            issued: true,
            expires_at: Some(expiry),
            ..Observation::default()
        };
        let next = advance(&domain(CertState::Issuing, 1), &seen, now).expect("moves");
        assert_eq!((next.state, next.failures), (CertState::Active, 0));
        assert_eq!(next.expires_at, Some(expiry));

        // Angie reporting valid without a usable file on disk is not enough.
        let no_file = Observation {
            angie: Some(&valid),
            ..Observation::default()
        };
        assert_eq!(advance(&domain(CertState::Issuing, 0), &no_file, now), None);
    }

    #[test]
    fn a_paused_domain_is_checked_again_when_its_retry_is_due() {
        let now = Utc::now();
        let mut paused = domain(CertState::Failed, 2);
        paused.next_retry_at = Some(now + TimeDelta::minutes(10));
        assert!(!wants_check(&paused, now));
        paused.next_retry_at = Some(now - TimeDelta::minutes(1));
        assert!(wants_check(&paused, now));

        let reachable = Observation {
            reachability: Some(Ok(())),
            ..Observation::default()
        };
        let next = advance(&paused, &reachable, now).expect("moves");
        assert_eq!(next.state, CertState::Issuing);
        assert_eq!(
            next.failures, 2,
            "a further failure pauses it again, for longer"
        );
    }

    #[test]
    fn an_active_domain_follows_renewals_and_recovers_a_lost_certificate() {
        let now = Utc::now();
        let mut active = domain(CertState::Active, 0);
        active.expires_at = Some(now + TimeDelta::days(20));

        let renewed = Observation {
            issued: true,
            expires_at: Some(now + TimeDelta::days(90)),
            ..Observation::default()
        };
        let next = advance(&active, &renewed, now).expect("renewal is recorded");
        assert_eq!(next.state, CertState::Active);
        assert_eq!(next.expires_at, Some(now + TimeDelta::days(90)));

        let lost = Observation::default();
        assert_eq!(
            advance(&active, &lost, now).expect("moves").state,
            CertState::Issuing
        );
    }

    #[test]
    fn manual_and_off_domains_are_left_alone() {
        let mut manual = domain(CertState::Manual, 0);
        manual.mode = TlsMode::Manual;
        let seen = Observation {
            reachability: Some(Ok(())),
            ..Observation::default()
        };
        assert_eq!(advance(&manual, &seen, Utc::now()), None);
    }

    #[test]
    fn rechecks_start_quickly_and_settle_at_hourly() {
        assert_eq!(recheck_delay(0), Duration::from_secs(5 * 60));
        assert_eq!(recheck_delay(1), Duration::from_secs(10 * 60));
        assert_eq!(recheck_delay(3), Duration::from_secs(40 * 60));
        assert_eq!(recheck_delay(9), Duration::from_secs(60 * 60));
    }

    #[test]
    fn a_client_name_is_stable_and_safe_for_an_angie_variable() {
        let name = client_name("Shop.Example.com ");
        assert_eq!(
            name,
            client_name("shop.example.com"),
            "case and padding do not matter"
        );
        assert!(name.starts_with(CLIENT_PREFIX));
        assert!(
            name.chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_'),
            "{name} has to be usable inside $acme_cert_<client>"
        );
        assert_ne!(name, client_name("www.shop.example.com"));
    }

    #[test]
    fn modes_and_states_round_trip_through_storage() {
        for mode in [TlsMode::Auto, TlsMode::Manual, TlsMode::Off] {
            assert_eq!(TlsMode::parse(mode.as_str()), Some(mode));
        }
        for state in [
            CertState::Pending,
            CertState::Checking,
            CertState::Issuing,
            CertState::Active,
            CertState::Failed,
            CertState::Manual,
            CertState::Off,
        ] {
            assert_eq!(CertState::parse(state.as_str()), Some(state));
        }
        assert_eq!(TlsMode::Auto.initial_state(), CertState::Pending);
        assert_eq!(TlsMode::Manual.initial_state(), CertState::Manual);
    }

    #[test]
    fn retries_back_off_to_once_a_day() {
        assert_eq!(retry_delay(0), Duration::from_secs(3600));
        assert_eq!(retry_delay(1), Duration::from_secs(4 * 3600));
        assert_eq!(retry_delay(2), Duration::from_secs(24 * 3600));
        assert_eq!(retry_delay(9), Duration::from_secs(24 * 3600));
    }

    #[test]
    fn wildcards_and_addresses_cannot_use_http_validation() {
        assert!(supports_http_challenge("shop.example.com"));
        assert!(!supports_http_challenge("*.example.com"));
        assert!(!supports_http_challenge("203.0.113.7"));
        assert!(!supports_http_challenge("2001:db8::1"));
        assert!(!supports_http_challenge("localhost"));
    }
}
