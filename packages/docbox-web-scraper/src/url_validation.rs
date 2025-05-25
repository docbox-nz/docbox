//! # URL Validation
//!
//! Validation for allowed URLs to enforce security requirements

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use url::{Host, Url};

/// Domain resolution trait, allows using a mock domain
/// resolver for tests
pub(crate) trait DomainResolver {
    async fn resolve_domain(
        host: &str,
        port: u16,
    ) -> std::io::Result<impl Iterator<Item = SocketAddr>>;
}

pub struct TokioDomainResolver;

impl DomainResolver for TokioDomainResolver {
    async fn resolve_domain(
        host: &str,
        port: u16,
    ) -> std::io::Result<impl Iterator<Item = SocketAddr>> {
        tokio::net::lookup_host(format!("{host}:{port}")).await
    }
}

const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

/// Validates that the provided `url` is valid for the web-scraper
/// to visit.
///
/// - The URL scheme is in [ALLOW_SCHEMES]
/// - The URL host portion is a domain NOT a IP address
/// - The resolved IP of the domain is a globally reachable address
///
/// This assures that the scraper does not attempt to perform requests against
/// internal addresses as that could be exploited to perform server side request
/// forgery
pub async fn is_allowed_url<D: DomainResolver>(url: &Url) -> bool {
    let host = match url.host() {
        Some(value) => value,
        None => return false,
    };

    // Enforce allowed schema
    if !ALLOWED_SCHEMES.contains(&url.scheme()) {
        return false;
    }

    let port = match url.port_or_known_default() {
        Some(value) => value,
        // Unable to determine the correct port
        None => return false,
    };

    let domain = match host {
        Host::Domain(domain) => domain,
        // Direct IP hosts are disallowed
        Host::Ipv4(_) | Host::Ipv6(_) => return false,
    };

    // Resolve host IP address
    let host_addresses = match D::resolve_domain(domain, port).await {
        Ok(value) => value,
        // Consider resolution failure as a not allowed address
        Err(_) => return false,
    };

    let mut any_valid = false;

    for addr in host_addresses {
        let ip = addr.ip();

        match ip {
            std::net::IpAddr::V4(addr) => {
                if !is_ipv4_global(addr) {
                    return false;
                }

                any_valid = true;
            }
            std::net::IpAddr::V6(addr) => {
                if !is_ipv6_global(addr) {
                    return false;
                }

                any_valid = true;
            }
        }
    }

    any_valid
}

/// Sourced from the unstable rust standard library [Ipv4Addr::is_global]
///
/// Used to check if the provided IPv4 address is globally reachable
fn is_ipv4_global(addr: Ipv4Addr) -> bool {
    !(addr.octets()[0] == 0 // "This network"
            || addr.is_private()
            // Returns [`true`] if this address is part of the Shared Address Space defined in
            // [IETF RFC 6598] (`100.64.0.0/10`).
            || (addr.octets()[0] == 100 && (addr.octets()[1] & 0b1100_0000 == 0b0100_0000))
            || addr.is_loopback()
            || addr.is_link_local()
            // addresses reserved for future protocols (`192.0.0.0/24`)
            // .9 and .10 are documented as globally reachable so they're excluded
            || (
                addr.octets()[0] == 192 && addr.octets()[1] == 0 && addr.octets()[2] == 0
                && addr.octets()[3] != 9 && addr.octets()[3] != 10
            )
            || addr.is_documentation()
            // Returns [`true`] if this address part of the `198.18.0.0/15` range, which is reserved for
            // network devices benchmarking.
            || (addr.octets()[0] == 198 && (addr.octets()[1] & 0xfe) == 18)
            // Returns [`true`] if this address is reserved by IANA for future use.
            || (addr.octets()[0] & 240 == 240 && !addr.is_broadcast())
            || addr.is_broadcast())
}

/// Sourced from the unstable rust standard library [Ipv6Addr::is_global]
///
/// Used to check if the provided IPv6 address is globally reachable
fn is_ipv6_global(addr: Ipv6Addr) -> bool {
    !(addr.is_unspecified()
            || addr.is_loopback()
            // IPv4-mapped Address (`::ffff:0:0/96`)
            || matches!(addr.segments(), [0, 0, 0, 0, 0, 0xffff, _, _])
            // IPv4-IPv6 Translat. (`64:ff9b:1::/48`)
            || matches!(addr.segments(), [0x64, 0xff9b, 1, _, _, _, _, _])
            // Discard-Only Address Block (`100::/64`)
            || matches!(addr.segments(), [0x100, 0, 0, 0, _, _, _, _])
            // IETF Protocol Assignments (`2001::/23`)
            || (matches!(addr.segments(), [0x2001, b, _, _, _, _, _, _] if b < 0x200)
                && !(
                    // Port Control Protocol Anycast (`2001:1::1`)
                    u128::from_be_bytes(addr.octets()) == 0x2001_0001_0000_0000_0000_0000_0000_0001
                    // Traversal Using Relays around NAT Anycast (`2001:1::2`)
                    || u128::from_be_bytes(addr.octets()) == 0x2001_0001_0000_0000_0000_0000_0000_0002
                    // AMT (`2001:3::/32`)
                    || matches!(addr.segments(), [0x2001, 3, _, _, _, _, _, _])
                    // AS112-v6 (`2001:4:112::/48`)
                    || matches!(addr.segments(), [0x2001, 4, 0x112, _, _, _, _, _])
                    // ORCHIDv2 (`2001:20::/28`)
                    // Drone Remote ID Protocol Entity Tags (DETs) Prefix (`2001:30::/28`)`
                    || matches!(addr.segments(), [0x2001, b, _, _, _, _, _, _] if (0x20..=0x3F).contains(&b))
                ))
            // 6to4 (`2002::/16`) – it's not explicitly documented as globally reachable,
            // IANA says N/A.
            || matches!(addr.segments(), [0x2002, _, _, _, _, _, _, _])
            // if this is an address reserved for documentation
            || matches!(addr.segments(), [0x2001, 0xdb8, ..] | [0x3fff, 0..=0x0fff, ..])
            // Segment Routing (SRv6) SIDs (`5f00::/16`)
            || matches!(addr.segments(), [0x5f00, ..])
            || addr.is_unique_local()
            || addr.is_unicast_link_local())
}

#[cfg(test)]
mod test {
    use crate::url_validation::is_allowed_url;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use url::Url;

    use super::DomainResolver;

    struct MockDomainResolver;

    impl DomainResolver for MockDomainResolver {
        async fn resolve_domain(
            host: &str,
            port: u16,
        ) -> std::io::Result<impl Iterator<Item = std::net::SocketAddr>> {
            match host {
                // Localhost should always resolve to 127.0.0.1
                "localhost" => Ok([SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::new(127, 0, 0, 1),
                    port,
                ))]
                .into_iter()),

                // Example domains should resolve to the fixed public address that
                // example.com uses, this is owned by IANA
                "example.com" | "example.org" | "example.net" => Ok([SocketAddr::V4(
                    SocketAddrV4::new(Ipv4Addr::new(93, 184, 216, 34), port),
                )]
                .into_iter()),

                // Fake "bad" domains that point to local addresses
                "local.example.com" | "local.example.org" | "local.example.net" => {
                    Ok([SocketAddr::V4(SocketAddrV4::new(
                        Ipv4Addr::new(127, 0, 0, 1),
                        port,
                    ))]
                    .into_iter())
                }

                // Resolve anything else as a public address (Use cloudflare DNS)
                _ => Ok([SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::new(1, 1, 1, 1),
                    port,
                ))]
                .into_iter()),
            }
        }
    }

    #[tokio::test]
    async fn test_attempt_local_ip() {
        // All local hosts should be rejected
        for host in [
            "http://127.0.0.1",
            "https://127.0.0.1",
            "http://10.0.0.1",
            "https://10.0.0.1",
            "http://10.0.0.0",
            "https://10.0.0.0",
            "http://192.168.0.1",
            "https://192.168.0.1",
        ] {
            assert!(!is_allowed_url::<MockDomainResolver>(&Url::parse(host).unwrap()).await);
        }
    }

    #[tokio::test]
    async fn test_attempt_local_host() {
        // All local hosts should be rejected
        for host in ["http://localhost", "https://localhost"] {
            assert!(!is_allowed_url::<MockDomainResolver>(&Url::parse(host).unwrap()).await);
        }
    }

    /// Checks that public hosts are allowed
    #[tokio::test]
    async fn test_attempt_public_host() {
        for host in [
            "https://example.com",
            "http://example.com",
            "https://example.org",
            "http://example.org",
            "https://example.net",
            "http://example.net",
        ] {
            assert!(is_allowed_url::<MockDomainResolver>(&Url::parse(host).unwrap()).await);
        }
    }

    /// Checks that domains that point to local addresses are disallowed
    #[tokio::test]
    async fn test_attempt_bad_public_host() {
        // All public hosts should be allowed
        for host in [
            "https://local.example.com",
            "http://local.example.com",
            "https://local.example.org",
            "http://local.example.org",
            "https://local.example.net",
            "http://local.example.net",
        ] {
            assert!(!is_allowed_url::<MockDomainResolver>(&Url::parse(host).unwrap()).await);
        }
    }

    /// Checks that valid schemes are allowed
    #[tokio::test]
    async fn test_attempt_allowed_host_schemas() {
        for host in ["http://example.com", "https://example.com"] {
            assert!(is_allowed_url::<MockDomainResolver>(&Url::parse(host).unwrap()).await);
        }
    }

    /// Checks that invalid schemes are denied
    #[tokio::test]
    async fn test_attempt_bad_host_schemas() {
        for host in [
            "ftp://example.com",
            "sftp://example.com",
            "file://example.com",
            "ws://example.com",
            "wss://example.com",
            "data://example.com",
            "mailto://example.com",
            "telnet://example.com",
            "blob://example.com",
            "scp://example.com",
        ] {
            assert!(!is_allowed_url::<MockDomainResolver>(&Url::parse(host).unwrap()).await);
        }
    }
}
