# Test fixtures

`exif.heic`, `exif.png` and `exif.webp` are copied unchanged from the
`kamadak-exif` 0.6.2 crate's `tests/` directory. They are distributed under
that crate's BSD-2-Clause licence:

> Copyright (c) 2016-2023 KAMADA Ken'ichi. All rights reserved.
>
> Redistribution and use in source and binary forms, with or without
> modification, are permitted provided that the conditions in the
> BSD-2-Clause licence (https://opensource.org/license/bsd-2-clause) are met.

Every other media fixture used by the tests is generated in code
(`squirrel_media::testutil`).

`geo-sample.tsv` is ten places from Squirrel's GeoNames cities500 extract
(`squirrel admin geodata build`), for tests and offline development
(`SQUIRREL_GEODATA_FILE`). Data: GeoNames (https://www.geonames.org),
licensed CC BY 4.0.

`rotated-source.jpg` is generated (`cargo run -p squirrel-media --example make_heic_source
--features test-util`; a test checks it is current): 64 × 32 with EXIF orientation 6.
`rotated.heic` is not committed: CI's `rust-heic` job encodes it from that JPEG with libheif's
`heif-enc -q 50` before running the HEIC tests, which fail on CI (and skip elsewhere) without it.
