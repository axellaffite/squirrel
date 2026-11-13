//! Client address resolution, shared by rate limiting and security logs.

use std::convert::Infallible;
use std::fmt;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use ipnet::IpNet;

use crate::state::AppState;

const X_FORWARDED_FOR: &str = "x-forwarded-for";
/// IPv6 clients usually own a whole /64, so they are limited per /64.
const IPV6_KEY_PREFIX: u8 = 64;

/// Reverse proxies whose `X-Forwarded-For` entries are believed.
#[derive(Debug, Clone, Default)]
pub struct TrustedProxies(Arc<[IpNet]>);

#[derive(Debug, thiserror::Error)]
#[error("invalid trusted proxy {0:?}: expected an IP address or CIDR range")]
pub struct TrustedProxiesError(String);

impl TrustedProxies {
    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = ip.to_canonical();
        self.0.iter().any(|net| net.contains(&ip))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromStr for TrustedProxies {
    type Err = TrustedProxiesError;

    /// Comma-separated IPs (`10.0.0.1`) or CIDR ranges (`10.0.0.0/8`); empty means none.
    fn from_str(list: &str) -> Result<Self, Self::Err> {
        list.split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(|entry| {
                entry
                    .parse::<IpNet>()
                    .or_else(|_| entry.parse::<IpAddr>().map(IpNet::from))
                    .map(|net| net.trunc())
                    .map_err(|_| TrustedProxiesError(entry.to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|nets| Self(nets.into()))
    }
}

/// The peer address, unless the peer is a trusted proxy: then the rightmost
/// `X-Forwarded-For` entry that is not itself a trusted proxy. Entries left of
/// it are client-controlled and ignored. Falls back to the peer.
pub fn resolve(peer: IpAddr, headers: &HeaderMap, trusted: &TrustedProxies) -> IpAddr {
    let peer = peer.to_canonical();
    if !trusted.contains(peer) {
        return peer;
    }
    let entries: Vec<&str> = headers
        .get_all(X_FORWARDED_FOR)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .collect();
    for entry in entries.into_iter().rev() {
        let Some(ip) = parse_forwarded_entry(entry) else {
            return peer;
        };
        if !trusted.contains(ip) {
            return ip;
        }
    }
    peer
}

fn parse_forwarded_entry(entry: &str) -> Option<IpAddr> {
    entry
        .parse::<IpAddr>()
        .or_else(|_| entry.parse::<SocketAddr>().map(|addr| addr.ip()))
        .ok()
        .map(|ip| ip.to_canonical())
}

/// Rate-limit bucket for an address: IPv4 as is, IPv6 masked to its /64.
pub fn rate_limit_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => {
            let mask = u128::MAX << (128 - u32::from(IPV6_KEY_PREFIX));
            IpAddr::V6(Ipv6Addr::from(u128::from(v6) & mask))
        }
        v4 => v4,
    }
}

/// Resolved client address for logs. `None` when the server was not started
/// with connection info (e.g. some tests).
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub Option<IpAddr>);

impl fmt::Display for ClientIp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(ip) => ip.fmt(f),
            None => f.write_str("unknown"),
        }
    }
}

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip());
        Ok(Self(peer.map(|peer| {
            resolve(peer, &parts.headers, &state.config.trusted_proxies)
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn xff(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(X_FORWARDED_FOR, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn parses_ips_and_cidrs() {
        let trusted: TrustedProxies = " 10.0.0.0/8, 192.168.1.5 ,fd00::/8".parse().unwrap();
        assert!(trusted.contains(ip("10.1.2.3")));
        assert!(trusted.contains(ip("192.168.1.5")));
        assert!(!trusted.contains(ip("192.168.1.6")));
        assert!(trusted.contains(ip("fd12::1")));
        assert!(trusted.contains(ip("::ffff:10.0.0.1")));
        assert!("".parse::<TrustedProxies>().unwrap().is_empty());
        assert!("10.0.0.0/33".parse::<TrustedProxies>().is_err());
        assert!("proxy.local".parse::<TrustedProxies>().is_err());
    }

    #[test]
    fn untrusted_peer_ignores_forwarded_for() {
        let trusted = TrustedProxies::default();
        assert_eq!(
            resolve(ip("203.0.113.9"), &xff("1.1.1.1"), &trusted),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn trusted_peer_uses_rightmost_untrusted_entry() {
        let trusted: TrustedProxies = "10.0.0.0/8".parse().unwrap();
        let headers = xff("6.6.6.6, 1.1.1.1, 10.0.0.2");
        assert_eq!(resolve(ip("10.0.0.1"), &headers, &trusted), ip("1.1.1.1"));
    }

    #[test]
    fn trusted_peer_falls_back_to_peer() {
        let trusted: TrustedProxies = "10.0.0.0/8".parse().unwrap();
        let peer = ip("10.0.0.1");
        assert_eq!(resolve(peer, &HeaderMap::new(), &trusted), peer);
        assert_eq!(resolve(peer, &xff("10.0.0.2"), &trusted), peer);
        assert_eq!(resolve(peer, &xff("1.1.1.1, garbage"), &trusted), peer);
    }

    #[test]
    fn accepts_entries_with_ports() {
        let trusted: TrustedProxies = "10.0.0.1".parse().unwrap();
        let headers = xff("[2001:db8::1]:4711");
        assert_eq!(
            resolve(ip("10.0.0.1"), &headers, &trusted),
            ip("2001:db8::1")
        );
    }

    #[test]
    fn ipv6_keys_are_masked_to_a_64() {
        assert_eq!(
            rate_limit_key(ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd")),
            ip("2001:db8:1:2::")
        );
        assert_eq!(rate_limit_key(ip("1.2.3.4")), ip("1.2.3.4"));
        assert_eq!(rate_limit_key(ip("::ffff:1.2.3.4")), ip("1.2.3.4"));
    }
}
