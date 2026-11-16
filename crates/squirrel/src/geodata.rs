//! `squirrel admin geodata …`: the offline reverse-geocoding dataset.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::Path;

use anyhow::Context;
use squirrel_media::geo_build;
use squirrel_media::geo_data::PINNED;

use crate::admin::GeodataAction;

pub fn run(action: GeodataAction) -> anyhow::Result<()> {
    match action {
        GeodataAction::Build {
            cities,
            admin1,
            countries,
            date,
            out,
        } => {
            let count = build(&cities, &admin1, &countries, &date, &out)?;
            println!("wrote {count} places to {}", out.display());
            Ok(())
        }
        GeodataAction::Pinned => {
            println!("{}", pinned_line());
            Ok(())
        }
    }
}

/// One source of truth for `scripts/install-geodata.sh`.
fn pinned_line() -> String {
    format!("{}  {}", PINNED.sha256, PINNED.file_name)
}

fn open(path: &Path) -> anyhow::Result<BufReader<File>> {
    Ok(BufReader::new(
        File::open(path).with_context(|| format!("opening {}", path.display()))?,
    ))
}

fn build(
    cities: &Path,
    admin1: &Path,
    countries: &Path,
    date: &str,
    out: &Path,
) -> anyhow::Result<usize> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .with_context(|| format!("creating {}", out.display()))?;
    let result = geo_build::build(
        open(cities)?,
        open(admin1)?,
        open(countries)?,
        date,
        BufWriter::new(file),
    );
    if result.is_err() {
        // Never leave half a dataset behind.
        let _ = std::fs::remove_file(out);
    }
    result.with_context(|| format!("building {}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_line_is_a_shasum_check_line() {
        let line = pinned_line();
        let (hash, name) = line.split_once("  ").unwrap();
        assert_eq!(hash.len(), 64);
        assert!(name.starts_with("geonames-cities500-") && name.ends_with(".tsv"));
    }

    #[test]
    fn builds_a_file_and_refuses_to_overwrite_it() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, text: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, text).unwrap();
            path
        };
        let cities = write(
            "cities.txt",
            "1\tParis\tParis\t\t48.85341\t2.3488\tP\tPPLC\tFR\t\t11\t\t\t\t0\t\t42\tEurope/Paris\t2024-06-05\n",
        );
        let admin1 = write("admin1.txt", "FR.11\tÎle-de-France\tx\t1\n");
        let countries = write("countries.txt", "FR\tFRA\t250\tFR\tFrance\n");
        let out = dir.path().join("out.tsv");
        assert_eq!(build(&cities, &admin1, &countries, "d", &out).unwrap(), 1);
        assert!(build(&cities, &admin1, &countries, "d", &out).is_err());
        let bad = write("bad.txt", "garbage\n");
        let out2 = dir.path().join("out2.tsv");
        assert!(build(&bad, &admin1, &countries, "d", &out2).is_err());
        assert!(!out2.exists());
    }
}
