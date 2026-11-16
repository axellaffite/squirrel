//! The committed sample dataset loads and answers like the real one.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use squirrel_media::geo::GeoIndex;

fn sample() -> GeoIndex {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/geo-sample.tsv");
    GeoIndex::load(BufReader::new(File::open(path).unwrap())).unwrap()
}

#[test]
fn the_sample_names_its_places() {
    let geo = sample();
    assert_eq!(geo.len(), 10);
    let louvre = geo.nearest(48.8606, 2.3376).unwrap();
    assert_eq!(
        (
            louvre.name.as_str(),
            louvre.admin1.as_str(),
            louvre.country.as_str()
        ),
        ("Paris", "Île-de-France", "France")
    );
    assert_eq!(geo.nearest(64.1, -21.9).unwrap().name, "Reykjavík");
    assert_eq!(geo.nearest(-16.45, 179.4).unwrap().country_code, "FJ");
    assert!(geo.nearest(30.0, -40.0).is_none(), "mid-Atlantic");
}
