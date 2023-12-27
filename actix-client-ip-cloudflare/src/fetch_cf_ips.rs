use std::net::{IpAddr, Ipv4Addr};

use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use serde::Deserialize;

/// URL for Cloudflare's canonical list of IP ranges.
pub const CF_URL_IPS: &str = "https://api.cloudflare.com/client/v4/ips";

#[derive(Debug)]
#[non_exhaustive]
pub enum CfIpsFetchErr {
    Fetch,
}

impl_more::impl_display_enum!(CfIpsFetchErr, Fetch => "failed to fetch");

impl std::error::Error for CfIpsFetchErr {}

#[derive(Debug, Deserialize)]
pub struct CfIpsResult {
    ipv4_cidrs: Vec<Ipv4Network>,
    ipv6_cidrs: Vec<Ipv6Network>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CfIpsResponse {
    Success { result: CfIpsResult },
    Failure { success: bool },
}

/// Trusted IP ranges.
///
/// This set of IPs is used for determining the trustworthiness of a Cloudflare header. If the
/// connection's peer address (i.e., the first network hop) is contained in this set, then
/// [`TrustedClientIp`](crate::TrustedClientIp) will extract the stated client IP address or,
/// otherwise, respond with an error. An instance of this type should be placed in app data for the
/// extractor to retrieve.
///
/// If your origin server's direct peer _is_ Cloudflare, see [`fetch_trusted_cf_ips()`] for
/// a convenient method of obtaining the official list of IP ranges from Cloudflare's API.
///
/// If you origin server has additional network hops, such as a load balancer, add it's IP (or IP
/// range) to your trusted IP set using [`with_ip_range()`](Self::with_ip_range()).
///
/// The `Default` implementation constructs an empty IP set.
#[derive(Debug, Clone, Default)]
pub struct TrustedIps {
    pub(crate) cidr_ranges: Vec<IpNetwork>,
}

impl TrustedIps {
    /// Construct new empty set of trusted IPs.
    pub fn new() -> Self {
        Self {
            cidr_ranges: Vec::new(),
        }
    }

    /// Add trusted IP range to set.
    pub fn add_ip_range(mut self, cidr: IpNetwork) -> Self {
        self.cidr_ranges.push(cidr);
        self
    }

    /// Add trusted IP range to set.
    #[doc(hidden)]
    #[deprecated(since = "0.1.1", note = "Renamed to `.add_ip_range()`.")]
    pub fn with_ip_range(self, cidr: IpNetwork) -> Self {
        self.add_ip_range(cidr)
    }

    /// Adds the `127.0.0.0/8` IP range to this set.
    pub fn add_loopback_ips(self) -> Self {
        self.add_ip_range(IpNetwork::V4(
            Ipv4Network::new(Ipv4Addr::from([127, 0, 0, 0]), 8).unwrap(),
        ))
    }

    /// Adds the `10.0.0.0/8` and `192.168.0.0/16` IP ranges to this set.
    pub fn add_private_ips(self) -> Self {
        self.add_ip_range(IpNetwork::V4(
            Ipv4Network::new(Ipv4Addr::from([10, 0, 0, 0]), 8).unwrap(),
        ))
        .add_ip_range(IpNetwork::V4(
            Ipv4Network::new(Ipv4Addr::from([192, 168, 0, 0]), 16).unwrap(),
        ))
    }

    /// Returns true if `ip` is trustworthy according to this set.
    pub fn contains(&self, ip: IpAddr) -> bool {
        self.cidr_ranges.iter().any(|cidr| cidr.contains(ip))
    }

    /// Constructs new set of trusted IPs from a deserialized Cloudflare response.
    pub fn try_from_response(res: CfIpsResponse) -> Result<Self, CfIpsFetchErr> {
        let ips = match res {
            CfIpsResponse::Success { result } => result,
            CfIpsResponse::Failure { .. } => {
                tracing::error!("parsing response returned success: false");
                return Err(CfIpsFetchErr::Fetch);
            }
        };

        let mut cidr_ranges = Vec::new();

        for cidr in ips.ipv4_cidrs {
            cidr_ranges.push(IpNetwork::V4(cidr));
        }

        for cidr in ips.ipv6_cidrs {
            cidr_ranges.push(IpNetwork::V6(cidr));
        }

        Ok(Self { cidr_ranges })
    }
}

/// Fetched trusted Cloudflare IP addresses from their API.
#[cfg(feature = "fetch-ips")]
pub async fn fetch_trusted_cf_ips() -> Result<TrustedIps, CfIpsFetchErr> {
    let client = awc::Client::new();

    tracing::debug!("fetching cloudflare IPs");
    let mut res = client.get(CF_URL_IPS).send().await.map_err(|err| {
        tracing::error!("{err}");
        CfIpsFetchErr::Fetch
    })?;

    tracing::debug!("parsing response");
    let res = res.json::<CfIpsResponse>().await.map_err(|err| {
        tracing::error!("{err}");
        CfIpsFetchErr::Fetch
    })?;

    TrustedIps::try_from_response(res)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn cf_ips_from_response() {
        let res = CfIpsResponse::Failure { success: false };
        assert!(TrustedIps::try_from_response(res).is_err());

        let res = CfIpsResponse::Failure { success: false };
        assert!(TrustedIps::try_from_response(res).is_err());

        let res = CfIpsResponse::Success {
            result: CfIpsResult {
                ipv4_cidrs: vec![Ipv4Network::new(Ipv4Addr::from([0, 0, 0, 0]), 0).unwrap()],
                ipv6_cidrs: vec![Ipv6Network::new(Ipv6Addr::from(0_u128), 0).unwrap()],
            },
        };
        assert!(TrustedIps::try_from_response(res).is_ok());
    }

    #[test]
    fn trusted_ips_membership() {
        let ips = TrustedIps::new();
        assert!(!ips.contains("127.0.0.1".parse().unwrap()));
        assert!(!ips.contains("10.0.1.1".parse().unwrap()));

        let ips = TrustedIps::new().add_loopback_ips();
        assert!(ips.contains("127.0.0.1".parse().unwrap()));
        assert!(!ips.contains("10.0.1.1".parse().unwrap()));

        let ips = TrustedIps::new().add_loopback_ips().add_private_ips();
        assert!(ips.contains("127.0.0.1".parse().unwrap()));
        assert!(ips.contains("10.0.1.1".parse().unwrap()));
    }

    #[test]
    fn trusted_ips_clone() {
        let ips = TrustedIps::new().add_loopback_ips();
        assert!(ips.contains("127.0.0.1".parse().unwrap()));
        assert!(!ips.contains("10.0.1.1".parse().unwrap()));

        let ips = ips.clone();
        assert!(ips.contains("127.0.0.1".parse().unwrap()));
        assert!(!ips.contains("10.0.1.1".parse().unwrap()));
    }
}
