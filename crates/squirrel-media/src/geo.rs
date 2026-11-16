//! Offline reverse geocoding over a GeoNames cities500 extract.
//!
//! Data file (UTF-8 TSV, one place per line, `#` lines are comments):
//! `latitude \t longitude \t name \t admin1 name \t country code \t country name \t timezone`

use std::collections::HashMap;
use std::io::BufRead;

/// Matches farther than this are not a place (sea, wilderness).
pub const MAX_DISTANCE_KM: f64 = 50.0;
const EARTH_RADIUS_KM: f64 = 6371.0;
const KM_PER_DEGREE: f64 = 111.19;

#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub admin1: String,
    pub country_code: String,
    pub country: String,
    pub timezone: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum GeoError {
    #[error("line {0}: malformed place")]
    Malformed(usize),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// Places bucketed in 1° × 1° cells.
pub struct GeoIndex {
    places: Vec<Place>,
    cells: HashMap<(i16, i16), Vec<u32>>,
}

fn cell(latitude: f64, longitude: f64) -> (i16, i16) {
    (latitude.floor() as i16, longitude.floor() as i16)
}

/// Great-circle distance in km (haversine).
pub fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().min(1.0).asin()
}

impl GeoIndex {
    pub fn load(reader: impl BufRead) -> Result<Self, GeoError> {
        let mut places = Vec::new();
        for (number, line) in reader.lines().enumerate() {
            let line = line?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            places.push(parse_line(&line).ok_or(GeoError::Malformed(number + 1))?);
        }
        Ok(Self::from_places(places))
    }

    pub fn from_places(places: Vec<Place>) -> Self {
        let mut cells: HashMap<(i16, i16), Vec<u32>> = HashMap::new();
        for (i, place) in places.iter().enumerate() {
            let index = u32::try_from(i).expect("fewer than 4 billion places");
            cells
                .entry(cell(place.latitude, place.longitude))
                .or_default()
                .push(index);
        }
        Self { places, cells }
    }

    pub fn len(&self) -> usize {
        self.places.len()
    }

    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// The nearest place within [`MAX_DISTANCE_KM`], if any.
    pub fn nearest(&self, latitude: f64, longitude: f64) -> Option<&Place> {
        let (row, col) = cell(latitude, longitude);
        // 50 km is < 1° of latitude; in longitude a degree shrinks with cos(latitude).
        let shrink = latitude.abs().min(89.0).to_radians().cos();
        let span = ((MAX_DISTANCE_KM / (KM_PER_DEGREE * shrink)).ceil() as i16 + 1).min(180);
        let mut best: Option<(f64, &Place)> = None;
        for r in row - 1..=row + 1 {
            for c in col - span..=col + span {
                let wrapped = (c + 180).rem_euclid(360) - 180;
                for &i in self.cells.get(&(r, wrapped)).into_iter().flatten() {
                    let place = &self.places[i as usize];
                    let d = distance_km(latitude, longitude, place.latitude, place.longitude);
                    if d <= MAX_DISTANCE_KM && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, place));
                    }
                }
            }
        }
        best.map(|(_, place)| place)
    }
}

fn parse_line(line: &str) -> Option<Place> {
    let mut parts = line.split('\t');
    let latitude: f64 = parts.next()?.parse().ok()?;
    let longitude: f64 = parts.next()?.parse().ok()?;
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    let mut text = || parts.next().map(str::to_owned);
    Some(Place {
        name: text()?,
        admin1: text()?,
        country_code: text()?,
        country: text()?,
        timezone: text()?,
        latitude,
        longitude,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# GeoNames cities500 extract, CC BY 4.0\n\
        48.85341\t2.3488\tParis\tÎle-de-France\tFR\tFrance\tEurope/Paris\n\
        48.80359\t2.13424\tVersailles\tÎle-de-France\tFR\tFrance\tEurope/Paris\n\
        -33.86785\t151.20732\tSydney\tNew South Wales\tAU\tAustralia\tAustralia/Sydney\n\
        -16.5\t179.9\tLabasa\tNorthern\tFJ\tFiji\tPacific/Fiji\n";

    fn index() -> GeoIndex {
        GeoIndex::load(SAMPLE.as_bytes()).unwrap()
    }

    #[test]
    fn finds_the_nearest_place() {
        let geo = index();
        assert_eq!(geo.len(), 4);
        assert_eq!(geo.nearest(48.8606, 2.3376).unwrap().name, "Paris");
        assert_eq!(geo.nearest(48.79, 2.12).unwrap().name, "Versailles");
        assert_eq!(
            geo.nearest(-33.85, 151.21).unwrap().admin1,
            "New South Wales"
        );
    }

    #[test]
    fn far_from_everything_is_no_place() {
        assert!(index().nearest(0.0, -30.0).is_none());
    }

    #[test]
    fn searches_across_the_antimeridian() {
        assert_eq!(index().nearest(-16.5, -179.9).unwrap().name, "Labasa");
    }

    #[test]
    fn malformed_lines_are_reported_with_their_number() {
        let err = GeoIndex::load("# c\n1\t2\tonly-three\n".as_bytes())
            .err()
            .unwrap();
        assert!(matches!(err, GeoError::Malformed(2)), "{err}");
    }

    #[test]
    fn haversine_is_right_for_a_known_pair() {
        let d = distance_km(48.85341, 2.3488, 51.50853, -0.12574); // Paris → London
        assert!((d - 343.5).abs() < 2.0, "{d}");
    }
}
