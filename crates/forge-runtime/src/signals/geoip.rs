//! IP geolocation resolution.
//!
//! Ships with an embedded DB-IP Country Lite database for zero-config country
//! resolution. Optionally accepts a MaxMind GeoLite2-City MMDB file for
//! city-level granularity.

use forge_core::ForgeError;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

/// Resolved geolocation for a single IP address.
#[derive(Debug, Clone, Default)]
pub struct GeoInfo {
    /// ISO 3166-1 alpha-2 country code.
    pub country: Option<String>,
    /// Localized city name (English), when a city-level MMDB is configured.
    pub city: Option<String>,
}

enum Backend {
    Embedded(db_ip::DbIpDatabase<db_ip::CountryCode>),
    Mmdb(maxminddb::Reader<Vec<u8>>),
}

/// Thread-safe GeoIP resolver. Uses the embedded DB-IP database by default,
/// or a MaxMind MMDB file when configured for city-level resolution.
#[derive(Clone)]
pub struct GeoIpResolver {
    backend: Arc<Backend>,
}

impl Default for GeoIpResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl GeoIpResolver {
    /// Create a resolver backed by the embedded DB-IP Country Lite database.
    pub fn new() -> Self {
        Self {
            backend: Arc::new(Backend::Embedded(db_ip::include_country_code_database!())),
        }
    }

    /// Create a resolver backed by a MaxMind MMDB file (GeoLite2-City or similar).
    pub fn from_mmdb(path: &Path) -> Result<Self, ForgeError> {
        let reader = maxminddb::Reader::open_readfile(path).map_err(|e| {
            ForgeError::Config(format!(
                "failed to load GeoIP database {}: {e}",
                path.display()
            ))
        })?;
        Ok(Self {
            backend: Arc::new(Backend::Mmdb(reader)),
        })
    }

    /// Resolve an IP string to country and optionally city.
    pub fn lookup(&self, ip_str: &str) -> GeoInfo {
        let ip: IpAddr = match ip_str.parse() {
            Ok(ip) => ip,
            Err(_) => return GeoInfo::default(),
        };

        match self.backend.as_ref() {
            Backend::Embedded(db) => GeoInfo {
                country: db.get(&ip).map(|c| c.as_str().to_string()),
                city: None,
            },
            Backend::Mmdb(reader) => {
                let result: Result<maxminddb::geoip2::City, _> = reader.lookup(ip);
                match result {
                    Ok(record) => GeoInfo {
                        country: record
                            .country
                            .and_then(|c| c.iso_code)
                            .map(|s| s.to_string()),
                        city: record
                            .city
                            .and_then(|c| c.names)
                            .and_then(|n| n.get("en").copied())
                            .map(|s| s.to_string()),
                    },
                    Err(_) => GeoInfo::default(),
                }
            }
        }
    }
}
