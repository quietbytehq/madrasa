//! Country, currency and time zone facts a school is set up with. Pakistan is the first
//! market; everything here is a table so another country is one row, not a code change.
//!
//! - Weekend days are ISO weekday numbers (1 = Monday … 7 = Sunday).
//! - Currencies with two minor units only: money is `NUMERIC(14,2)`.
//! - Time zones are IANA names validated by `chrono-tz`.

use chrono_tz::Tz;
use serde::Serialize;
use std::str::FromStr;

/// How a numeric date like `03/04/2015` is read when it is ambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOrder {
    /// Day, month, year (Pakistan, the UK, most of the world).
    DayFirst,
    /// Month, day, year (United States).
    MonthFirst,
}

#[derive(Debug, Clone, Serialize)]
pub struct CountryInfo {
    /// ISO 3166-1 alpha-2, upper case.
    pub code: &'static str,
    pub name: &'static str,
    pub time_zone: &'static str,
    pub currency: &'static str,
    /// Default weekly off days for schools.
    pub weekend: &'static [u32],
    /// What the national identity number is called on screen.
    pub id_label: &'static str,
    pub date_order: DateOrder,
}

const fn c(
    code: &'static str,
    name: &'static str,
    time_zone: &'static str,
    currency: &'static str,
    weekend: &'static [u32],
    id_label: &'static str,
    date_order: DateOrder,
) -> CountryInfo {
    CountryInfo {
        code,
        name,
        time_zone,
        currency,
        weekend,
        id_label,
        date_order,
    }
}

const SAT_SUN: &[u32] = &[6, 7];
const FRI_SAT: &[u32] = &[5, 6];
const SUN: &[u32] = &[7];
const FRI: &[u32] = &[5];
const SAT: &[u32] = &[6];
const NATIONAL_ID: &str = "National ID";

/// Countries offered in setup. Pakistan first, then alphabetical by name.
pub const COUNTRIES: &[CountryInfo] = &[
    c(
        "PK",
        "Pakistan",
        "Asia/Karachi",
        "PKR",
        SUN,
        "CNIC / B-Form",
        DateOrder::DayFirst,
    ),
    c(
        "AF",
        "Afghanistan",
        "Asia/Kabul",
        "AFN",
        FRI,
        "Tazkira",
        DateOrder::DayFirst,
    ),
    c(
        "AU",
        "Australia",
        "Australia/Sydney",
        "AUD",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "BD",
        "Bangladesh",
        "Asia/Dhaka",
        "BDT",
        FRI_SAT,
        "NID",
        DateOrder::DayFirst,
    ),
    c(
        "CA",
        "Canada",
        "America/Toronto",
        "CAD",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "EG",
        "Egypt",
        "Africa/Cairo",
        "EGP",
        FRI_SAT,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "IN",
        "India",
        "Asia/Kolkata",
        "INR",
        SAT_SUN,
        "Aadhaar",
        DateOrder::DayFirst,
    ),
    c(
        "ID",
        "Indonesia",
        "Asia/Jakarta",
        "IDR",
        SAT_SUN,
        "NIK",
        DateOrder::DayFirst,
    ),
    c(
        "IE",
        "Ireland",
        "Europe/Dublin",
        "EUR",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "KE",
        "Kenya",
        "Africa/Nairobi",
        "KES",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "MY",
        "Malaysia",
        "Asia/Kuala_Lumpur",
        "MYR",
        SAT_SUN,
        "MyKad",
        DateOrder::DayFirst,
    ),
    c(
        "MV",
        "Maldives",
        "Indian/Maldives",
        "MVR",
        FRI_SAT,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "NP",
        "Nepal",
        "Asia/Kathmandu",
        "NPR",
        SAT,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "NZ",
        "New Zealand",
        "Pacific/Auckland",
        "NZD",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "NG",
        "Nigeria",
        "Africa/Lagos",
        "NGN",
        SAT_SUN,
        "NIN",
        DateOrder::DayFirst,
    ),
    c(
        "PH",
        "Philippines",
        "Asia/Manila",
        "PHP",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::MonthFirst,
    ),
    c(
        "QA",
        "Qatar",
        "Asia/Qatar",
        "QAR",
        FRI_SAT,
        "QID",
        DateOrder::DayFirst,
    ),
    c(
        "SA",
        "Saudi Arabia",
        "Asia/Riyadh",
        "SAR",
        FRI_SAT,
        "Iqama / National ID",
        DateOrder::DayFirst,
    ),
    c(
        "SG",
        "Singapore",
        "Asia/Singapore",
        "SGD",
        SAT_SUN,
        "NRIC",
        DateOrder::DayFirst,
    ),
    c(
        "ZA",
        "South Africa",
        "Africa/Johannesburg",
        "ZAR",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "LK",
        "Sri Lanka",
        "Asia/Colombo",
        "LKR",
        SAT_SUN,
        "NIC",
        DateOrder::DayFirst,
    ),
    c(
        "TR",
        "Türkiye",
        "Europe/Istanbul",
        "TRY",
        SAT_SUN,
        "T.C. Kimlik",
        DateOrder::DayFirst,
    ),
    c(
        "AE",
        "United Arab Emirates",
        "Asia/Dubai",
        "AED",
        SAT_SUN,
        "Emirates ID",
        DateOrder::DayFirst,
    ),
    c(
        "GB",
        "United Kingdom",
        "Europe/London",
        "GBP",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::DayFirst,
    ),
    c(
        "US",
        "United States",
        "America/New_York",
        "USD",
        SAT_SUN,
        NATIONAL_ID,
        DateOrder::MonthFirst,
    ),
];

/// The default when the school's country is unknown or not listed.
pub const DEFAULT_COUNTRY: &str = "PK";

pub fn country(code: &str) -> Option<&'static CountryInfo> {
    let code = code.trim().to_ascii_uppercase();
    COUNTRIES.iter().find(|c| c.code == code)
}

/// The country row for a code, falling back to Pakistan's row for unknown codes.
pub fn country_or_default(code: &str) -> &'static CountryInfo {
    country(code).unwrap_or(&COUNTRIES[0])
}

/// Default weekly off days for a country (Saturday and Sunday when not listed).
pub fn default_weekend(code: &str) -> Vec<u32> {
    match country(code) {
        Some(c) => c.weekend.to_vec(),
        None => SAT_SUN.to_vec(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrencyInfo {
    /// ISO 4217, upper case.
    pub code: &'static str,
    pub name: &'static str,
    pub symbol: &'static str,
}

const fn m(code: &'static str, name: &'static str, symbol: &'static str) -> CurrencyInfo {
    CurrencyInfo { code, name, symbol }
}

/// Currencies with two minor units. Pakistan's first, then alphabetical by code.
pub const CURRENCIES: &[CurrencyInfo] = &[
    m("PKR", "Pakistani rupee", "Rs"),
    m("AED", "UAE dirham", "AED"),
    m("AFN", "Afghan afghani", "؋"),
    m("AUD", "Australian dollar", "A$"),
    m("BDT", "Bangladeshi taka", "৳"),
    m("CAD", "Canadian dollar", "C$"),
    m("EGP", "Egyptian pound", "E£"),
    m("EUR", "Euro", "€"),
    m("GBP", "Pound sterling", "£"),
    m("IDR", "Indonesian rupiah", "Rp"),
    m("INR", "Indian rupee", "₹"),
    m("KES", "Kenyan shilling", "KSh"),
    m("LKR", "Sri Lankan rupee", "Rs"),
    m("MVR", "Maldivian rufiyaa", "Rf"),
    m("MYR", "Malaysian ringgit", "RM"),
    m("NGN", "Nigerian naira", "₦"),
    m("NPR", "Nepalese rupee", "Rs"),
    m("NZD", "New Zealand dollar", "NZ$"),
    m("PHP", "Philippine peso", "₱"),
    m("QAR", "Qatari riyal", "QR"),
    m("SAR", "Saudi riyal", "SR"),
    m("SGD", "Singapore dollar", "S$"),
    m("TRY", "Turkish lira", "₺"),
    m("USD", "US dollar", "$"),
    m("ZAR", "South African rand", "R"),
];

pub fn currency(code: &str) -> Option<&'static CurrencyInfo> {
    let code = code.trim().to_ascii_uppercase();
    CURRENCIES.iter().find(|c| c.code == code)
}

/// Parse an IANA time zone name such as `Asia/Karachi` or `America/Los_Angeles`.
pub fn time_zone(name: &str) -> Option<Tz> {
    Tz::from_str(name.trim()).ok()
}

/// Every IANA time zone name chrono-tz knows, for a picker.
pub fn time_zone_names() -> Vec<&'static str> {
    chrono_tz::TZ_VARIANTS.iter().map(|tz| tz.name()).collect()
}

/// Normalise a national identity number for a country. Pakistan's CNIC / B-Form must have 13
/// digits and is stored as `12345-1234567-1`; elsewhere the number is kept as typed (trimmed,
/// single spaces, upper case, 3–40 characters), since formats differ by country.
pub fn normalise_national_id(country: &str, input: &str) -> Option<String> {
    if country_or_default(country).code == "PK" {
        return crate::students::normalise_cnic(input);
    }
    let s = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let len = s.chars().count();
    ((3..=40).contains(&len)).then(|| s.to_uppercase())
}

/// Normalise a phone number for a country. Pakistan: 11 digits starting with 0 (see
/// [`crate::students::normalise_phone`]). Elsewhere: digits with an optional leading `+`,
/// 7–15 digits, e.g. `(503) 555-0142` → `5035550142`, `+1 503 555 0142` → `+15035550142`.
pub fn normalise_phone_for(country: &str, input: &str) -> Option<String> {
    if country_or_default(country).code == "PK" {
        return crate::students::normalise_phone(input);
    }
    let trimmed = input.trim();
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    if !(7..=15).contains(&digits.len()) {
        return None;
    }
    Some(if trimmed.starts_with('+') {
        format!("+{digits}")
    } else {
        digits
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countries_have_valid_zones_and_currencies() {
        for c in COUNTRIES {
            assert!(time_zone(c.time_zone).is_some(), "{} zone", c.code);
            assert!(currency(c.currency).is_some(), "{} currency", c.code);
            assert!(!c.weekend.is_empty() && c.weekend.iter().all(|d| (1..=7).contains(d)));
        }
        assert_eq!(COUNTRIES[0].code, DEFAULT_COUNTRY);
        assert_eq!(country("us").unwrap().name, "United States");
        assert!(country("XX").is_none());
        assert_eq!(country_or_default("XX").code, "PK");
    }

    #[test]
    fn weekend_defaults_by_country() {
        assert_eq!(default_weekend("PK"), vec![7]);
        assert_eq!(default_weekend("US"), vec![6, 7]);
        assert_eq!(default_weekend("SA"), vec![5, 6]);
        assert_eq!(default_weekend("NP"), vec![6]);
        assert_eq!(default_weekend("??"), vec![6, 7]);
    }

    #[test]
    fn time_zones_parse() {
        assert!(time_zone("Asia/Karachi").is_some());
        assert!(time_zone(" America/Los_Angeles ").is_some());
        assert!(time_zone("Mars/Olympus").is_none());
        assert!(time_zone_names().contains(&"Asia/Karachi"));
    }

    #[test]
    fn national_ids_and_phones_follow_the_country() {
        assert_eq!(
            normalise_national_id("PK", "4210112345671").as_deref(),
            Some("42101-1234567-1")
        );
        assert_eq!(normalise_national_id("PK", "12345"), None);
        assert_eq!(
            normalise_national_id("US", "  a12  b34 ").as_deref(),
            Some("A12 B34")
        );
        assert_eq!(normalise_national_id("US", "12"), None);
        assert_eq!(
            normalise_phone_for("PK", "+92 300 1234567").as_deref(),
            Some("03001234567")
        );
        assert_eq!(
            normalise_phone_for("US", "(503) 555-0142").as_deref(),
            Some("5035550142")
        );
        assert_eq!(
            normalise_phone_for("US", "+1 503 555 0142").as_deref(),
            Some("+15035550142")
        );
        assert_eq!(normalise_phone_for("US", "12345"), None);
    }
}
