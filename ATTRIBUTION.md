# Third-party data attributions

This file lists data assets shipped with the Forge framework that require
attribution under their license.

## DB-IP IP-to-Country Lite

`forge-runtime` embeds the **DB-IP IP-to-Country Lite** database via the
`db_ip` crate's `include-country-code-lite` feature. The data is published
by DB-IP under the [CC BY 4.0 license][cc-by-4] and requires a notice in
any product that ships a binary using the embedded data:

> IP geolocation by DB-IP — <https://db-ip.com>

Operators that override `[signals] geoip_db_path` to use a self-licensed
MaxMind MMDB file are not required to display this notice.

[cc-by-4]: https://creativecommons.org/licenses/by/4.0/
