//! Builds the reverse geocoder's data file from GeoNames dumps
//! (`cities500.txt`, `admin1CodesASCII.txt`, `countryInfo.txt`).
//!
//! The inputs are downloaded files: every line is checked, names are cleaned
//! of control characters, and nothing in them can break the output format.

use std::collections::HashMap;
use std::io::{BufRead, Write};

use crate::geo::GeoError;

/// `cities500.txt` columns (GeoNames' `geoname` table, 19 columns).
const CITY_NAME: usize = 1;
const CITY_LATITUDE: usize = 4;
const CITY_LONGITUDE: usize = 5;
const CITY_COUNTRY: usize = 8;
const CITY_ADMIN1: usize = 10;
const CITY_TIMEZONE: usize = 17;
const CITY_COLUMNS: usize = 19;
/// `countryInfo.txt`: ISO code and country name.
const COUNTRY_ISO: usize = 0;
const COUNTRY_NAME: usize = 4;

/// One output line, before sorting.
struct Row {
    latitude: String,
    longitude: String,
    name: String,
    admin1: String,
    country_code: String,
    country: String,
    timezone: String,
}

/// Replaces control characters and line/paragraph separators (which could
/// split a line or a field) with spaces, and trims.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// A coordinate kept as written (no float round trip), if it is a number in `range`.
fn coordinate(raw: &str, limit: f64) -> Option<String> {
    let value: f64 = raw.trim().parse().ok()?;
    (value.is_finite() && value.abs() <= limit).then(|| raw.trim().to_owned())
}

/// `CC.code` → admin1 name.
fn read_admin1(reader: impl BufRead) -> Result<HashMap<String, String>, GeoError> {
    let mut names = HashMap::new();
    for (number, line) in reader.lines().enumerate() {
        let line = line?;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let (Some(code), Some(name)) = (parts.next(), parts.next()) else {
            return Err(GeoError::Malformed(number + 1));
        };
        names.insert(code.to_owned(), clean(name));
    }
    Ok(names)
}

/// ISO code → country name.
fn read_countries(reader: impl BufRead) -> Result<HashMap<String, String>, GeoError> {
    let mut names = HashMap::new();
    for (number, line) in reader.lines().enumerate() {
        let line = line?;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        let (Some(code), Some(name)) = (parts.get(COUNTRY_ISO), parts.get(COUNTRY_NAME)) else {
            return Err(GeoError::Malformed(number + 1));
        };
        names.insert((*code).to_owned(), clean(name));
    }
    Ok(names)
}

fn is_country_code(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|b| b.is_ascii_uppercase())
}

fn read_city(
    line: &str,
    admin1: &HashMap<String, String>,
    countries: &HashMap<String, String>,
) -> Option<Row> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() < CITY_COLUMNS {
        return None;
    }
    let country_code = parts[CITY_COUNTRY];
    if !is_country_code(country_code) {
        return None;
    }
    let name = clean(parts[CITY_NAME]);
    if name.is_empty() {
        return None;
    }
    let region_key = format!("{country_code}.{}", parts[CITY_ADMIN1]);
    Some(Row {
        latitude: coordinate(parts[CITY_LATITUDE], 90.0)?,
        longitude: coordinate(parts[CITY_LONGITUDE], 180.0)?,
        name,
        admin1: admin1.get(&region_key).cloned().unwrap_or_default(),
        country_code: country_code.to_owned(),
        country: countries.get(country_code).cloned().unwrap_or_default(),
        timezone: clean(parts[CITY_TIMEZONE]),
    })
}

/// Writes the joined extract (see [`crate::geo`] for the format) and returns
/// the number of places. Lines are sorted by country code, name and
/// coordinates, so two builds of the same inputs are identical.
pub fn build(
    cities: impl BufRead,
    admin1: impl BufRead,
    countries: impl BufRead,
    source_date: &str,
    mut out: impl Write,
) -> Result<usize, GeoError> {
    let admin1 = read_admin1(admin1)?;
    let countries = read_countries(countries)?;
    let mut rows = Vec::new();
    for (number, line) in cities.lines().enumerate() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        rows.push(read_city(&line, &admin1, &countries).ok_or(GeoError::Malformed(number + 1))?);
    }
    rows.sort_by(|a, b| {
        (&a.country_code, &a.name, &a.latitude, &a.longitude).cmp(&(
            &b.country_code,
            &b.name,
            &b.latitude,
            &b.longitude,
        ))
    });
    writeln!(
        out,
        "# GeoNames cities500 extract for Squirrel, source date {}.",
        clean(source_date)
    )?;
    writeln!(
        out,
        "# Data: GeoNames (https://www.geonames.org), licensed CC BY 4.0. Columns:"
    )?;
    writeln!(
        out,
        "# latitude\tlongitude\tname\tadmin1\tcountry_code\tcountry\ttimezone"
    )?;
    for row in &rows {
        writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.latitude,
            row.longitude,
            row.name,
            row.admin1,
            row.country_code,
            row.country,
            row.timezone
        )?;
    }
    out.flush()?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::GeoIndex;

    const CITIES: &str = "2988507\tParis\tParis\tParis,Pariz\t48.85341\t2.3488\tP\tPPLC\tFR\t\t11\t75\t751\t75056\t2138551\t\t42\tEurope/Paris\t2024-06-05\n\
        2147714\tSydney\tSydney\t\t-33.86785\t151.20732\tP\tPPLA\tAU\t\t02\t\t\t\t4627345\t\t58\tAustralia/Sydney\t2024-01-01\n";
    const ADMIN1: &str = "FR.11\tÎle-de-France\tIle-de-France\t3012874\nAU.02\tNew South Wales\tNew South Wales\t2155400\n";
    const COUNTRIES: &str = "# ISO\tISO3\tISO-Numeric\tfips\tCountry\n\
        FR\tFRA\t250\tFR\tFrance\tParis\n\
        AU\tAUS\t036\tAS\tAustralia\tCanberra\n";

    #[test]
    fn joins_cities_with_region_and_country_names() {
        let mut out = Vec::new();
        let count = build(
            CITIES.as_bytes(),
            ADMIN1.as_bytes(),
            COUNTRIES.as_bytes(),
            "2026-11-14",
            &mut out,
        )
        .unwrap();
        assert_eq!(count, 2);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("# GeoNames cities500"), "{text}");
        assert!(text.contains("CC BY 4.0"));
        assert!(
            text.contains("48.85341\t2.3488\tParis\tÎle-de-France\tFR\tFrance\tEurope/Paris\n")
        );
        let index = GeoIndex::load(text.as_bytes()).unwrap();
        assert_eq!(&*index.nearest(-33.9, 151.2).unwrap().country, "Australia");
    }

    #[test]
    fn tabs_and_newlines_in_names_cannot_break_the_format() {
        let cities = CITIES.replace("\tParis\tParis\t", "\tPa\u{2028}ris\tParis\t");
        let mut out = Vec::new();
        build(
            cities.as_bytes(),
            ADMIN1.as_bytes(),
            COUNTRIES.as_bytes(),
            "x\ny",
            &mut out,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\tPa ris\t"), "{text}");
        assert_eq!(text.lines().filter(|l| l.starts_with('#')).count(), 3);
        assert!(GeoIndex::load(text.as_bytes()).is_ok());
    }

    #[test]
    fn output_is_sorted_whatever_the_input_order() {
        let reversed: String = CITIES.lines().rev().map(|l| format!("{l}\n")).collect();
        let build_of = |cities: &str| {
            let mut out = Vec::new();
            build(
                cities.as_bytes(),
                ADMIN1.as_bytes(),
                COUNTRIES.as_bytes(),
                "d",
                &mut out,
            )
            .unwrap();
            out
        };
        assert_eq!(build_of(CITIES), build_of(&reversed));
    }

    #[test]
    fn unknown_regions_and_countries_are_left_empty() {
        let mut out = Vec::new();
        build(
            CITIES.as_bytes(),
            "".as_bytes(),
            "".as_bytes(),
            "d",
            &mut out,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\tParis\t\tFR\t\tEurope/Paris\n"), "{text}");
    }

    #[test]
    fn malformed_cities_are_refused_with_their_line() {
        for bad in [
            "1\tShort\n".to_owned(),
            CITIES.replace("48.85341", "north"),
            CITIES.replace("48.85341", "91"),
            CITIES.replace("\tFR\t\t11", "\tfr\t\t11"),
        ] {
            let err = build(
                bad.as_bytes(),
                ADMIN1.as_bytes(),
                COUNTRIES.as_bytes(),
                "d",
                Vec::new(),
            )
            .unwrap_err();
            assert!(matches!(err, GeoError::Malformed(1)), "{err}");
        }
    }
}
