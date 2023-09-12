use std::net::IpAddr;

use cidr_utils::{cidr::IpCidr, utils::IpCidrCombiner};
use serde::Deserialize;

/// URL for Cloudflare's canonical list of IP ranges.
pub const CF_URL_IPS: &str = "https://api.cloudflare.com/client/v4/ips";

#[derive(Debug)]
pub enum Err {
    Fetch,
}

impl_more::impl_display_enum!(Err, Fetch => "failed to fetch");

impl std::error::Error for Err {}

#[derive(Debug, Deserialize)]
pub struct CfIpsResult {
    ipv4_cidrs: Vec<cidr_utils::cidr::Ipv4Cidr>,
    ipv6_cidrs: Vec<cidr_utils::cidr::Ipv6Cidr>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CfIpsResponse {
    Success { result: CfIpsResult },
    Failure { success: bool },
}

/// Trusted IP ranges.
#[derive(Debug)]
pub struct TrustedIps {
    pub(crate) cidr_ranges: IpCidrCombiner,
}

impl TrustedIps {
    pub fn try_from_response(res: CfIpsResponse) -> Result<Self, Err> {
        let ips = match res {
            CfIpsResponse::Success { result } => result,
            CfIpsResponse::Failure { .. } => {
                tracing::error!("parsing response returned success: false");
                return Err(Err::Fetch);
            }
        };

        let mut cidr_ranges = IpCidrCombiner::new();

        for cidr in ips.ipv4_cidrs {
            cidr_ranges.push(IpCidr::V4(cidr));
        }

        for cidr in ips.ipv6_cidrs {
            cidr_ranges.push(IpCidr::V6(cidr));
        }

        Ok(Self { cidr_ranges })
    }

    /// Add trusted IP range to list.
    pub fn with_ip_range(mut self, cidr: IpCidr) -> Self {
        self.cidr_ranges.push(cidr);
        self
    }

    /// Returns true if `ip` is controlled by Cloudflare.
    pub fn contains(&self, ip: IpAddr) -> bool {
        self.cidr_ranges.contains(ip)
    }
}

impl Clone for TrustedIps {
    fn clone(&self) -> Self {
        let ipv4_cidrs = self.cidr_ranges.get_ipv4_cidrs();
        let ipv6_cidrs = self.cidr_ranges.get_ipv6_cidrs();

        Self {
            cidr_ranges: ipv4_cidrs
                .iter()
                .copied()
                .map(IpCidr::V4)
                .chain(ipv6_cidrs.iter().copied().map(IpCidr::V6))
                .fold(
                    IpCidrCombiner::with_capacity(ipv4_cidrs.len(), ipv6_cidrs.len()),
                    |mut combiner, cidr| {
                        combiner.push(cidr);
                        combiner
                    },
                ),
        }
    }
}

/// Fetched trusted Cloudflare IP addresses from their API.
#[cfg(feature = "fetch-ips")]
pub async fn fetch_trusted_cf_ips() -> Result<TrustedIps, Err> {
    let client = awc::Client::new();

    tracing::debug!("fetching cloudflare ips");
    let mut res = client.get(CF_URL_IPS).send().await.map_err(|err| {
        tracing::error!("{err}");
        Err::Fetch
    })?;

    tracing::debug!("parsing response");
    let res = res.json::<CfIpsResponse>().await.map_err(|err| {
        tracing::error!("{err}");
        Err::Fetch
    })?;

    TrustedIps::try_from_response(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cf_ips_from_response() {
        let res = CfIpsResponse::Failure { success: false };
        assert!(TrustedIps::try_from_response(res).is_err());

        let res = CfIpsResponse::Failure { success: false };
        assert!(TrustedIps::try_from_response(res).is_err());
    }
}
