# Geodata

`geonames-cities500-20261116.tsv.gz` is the offline reverse-geocoding dataset:
every GeoNames place with a population of at least 500, joined with its
first-level region and country names, one line per place:

    latitude	longitude	name	admin1	country_code	country	timezone

- **Source:** GeoNames (https://www.geonames.org) dumps `cities500.zip`,
  `admin1CodesASCII.txt` and `countryInfo.txt` from
  https://download.geonames.org/export/dump/, downloaded 2026-11-16.
- **Licence:** Creative Commons Attribution 4.0 (CC BY 4.0,
  https://creativecommons.org/licenses/by/4.0/). Place names shown by
  Squirrel are credited "Place names: GeoNames (CC BY 4.0)". Changes: columns
  selected and joined, control characters in names replaced by spaces, lines
  sorted.
- **Pin:** `squirrel_media::geo_data::PINNED` holds the file name and the
  SHA-256 of the uncompressed TSV; a test checks this file against it.

Install it into a data directory (no network): `scripts/install-geodata.sh <data_dir>`.

Rebuild it (downloads from GeoNames): `scripts/build-geodata.sh <out_dir>`,
then replace the `.gz` here, update `PINNED` (name and hash) and this README.
Workers then re-geocode every geotagged photo on their next backfill.
