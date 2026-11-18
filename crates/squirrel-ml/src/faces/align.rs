//! 5-point face alignment onto the ArcFace template (112 × 112), as OpenCV's
//! `FaceRecognizerSF::alignCrop`: a least-squares similarity transform
//! (rotation, uniform scale, translation; no reflection), then a bilinear
//! warp with black borders.

use image::{Rgb, RgbImage};

/// Side of the aligned crop.
pub const SIZE: u32 = 112;
/// Where the eyes, nose tip and mouth corners land (ArcFace).
pub const TEMPLATE: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

/// `x' = a·x − b·y + tx`, `y' = b·x + a·y + ty`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Similarity {
    pub a: f32,
    pub b: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Similarity {
    pub fn apply(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        [
            self.a * x - self.b * y + self.tx,
            self.b * x + self.a * y + self.ty,
        ]
    }

    /// The inverse transform (`None` for a degenerate one).
    pub fn inverse(&self) -> Option<Self> {
        let det = self.a * self.a + self.b * self.b;
        if det <= f32::EPSILON {
            return None;
        }
        let (a, b) = (self.a / det, -self.b / det);
        Some(Self {
            a,
            b,
            tx: -(a * self.tx - b * self.ty),
            ty: -(b * self.tx + a * self.ty),
        })
    }
}

/// The similarity mapping `from` onto `to` with the least squared error
/// (closed form of Umeyama's method in 2-D, without reflection).
pub fn estimate(from: &[[f32; 2]; 5], to: &[[f32; 2]; 5]) -> Option<Similarity> {
    let mean = |points: &[[f32; 2]; 5]| {
        let (sx, sy) = points
            .iter()
            .fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
        [sx / 5.0, sy / 5.0]
    };
    let (mf, mt) = (mean(from), mean(to));
    let (mut dot, mut cross, mut var) = (0.0f32, 0.0f32, 0.0f32);
    for (p, q) in from.iter().zip(to) {
        let (px, py) = (p[0] - mf[0], p[1] - mf[1]);
        let (qx, qy) = (q[0] - mt[0], q[1] - mt[1]);
        dot += px * qx + py * qy;
        cross += px * qy - py * qx;
        var += px * px + py * py;
    }
    if var <= f32::EPSILON {
        return None;
    }
    let (a, b) = (dot / var, cross / var);
    Some(Similarity {
        a,
        b,
        tx: mt[0] - (a * mf[0] - b * mf[1]),
        ty: mt[1] - (b * mf[0] + a * mf[1]),
    })
}

fn sample(image: &RgbImage, x: f32, y: f32) -> Rgb<u8> {
    let (w, h) = (image.width() as i64, image.height() as i64);
    let (x0, y0) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let pixel = |px: i64, py: i64| -> [f32; 3] {
        if px < 0 || py < 0 || px >= w || py >= h {
            [0.0; 3]
        } else {
            image.get_pixel(px as u32, py as u32).0.map(f32::from)
        }
    };
    let (p00, p10, p01, p11) = (
        pixel(x0, y0),
        pixel(x0 + 1, y0),
        pixel(x0, y0 + 1),
        pixel(x0 + 1, y0 + 1),
    );
    Rgb(std::array::from_fn(|c| {
        let top = p00[c] * (1.0 - fx) + p10[c] * fx;
        let bottom = p01[c] * (1.0 - fx) + p11[c] * fx;
        (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
    }))
}

/// Warps `image` by `transform` (source → crop) into a `SIZE` × `SIZE` crop.
pub fn warp(image: &RgbImage, transform: &Similarity) -> Option<RgbImage> {
    let back = transform.inverse()?;
    Some(RgbImage::from_fn(SIZE, SIZE, |x, y| {
        let [sx, sy] = back.apply([x as f32, y as f32]);
        sample(image, sx, sy)
    }))
}

/// The aligned 112 × 112 face for `landmarks` (in `image`'s pixels).
pub fn align(image: &RgbImage, landmarks: &[[f32; 2]; 5]) -> Option<RgbImage> {
    warp(image, &estimate(landmarks, &TEMPLATE)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn the_template_maps_onto_itself() {
        let t = estimate(&TEMPLATE, &TEMPLATE).unwrap();
        assert!((t.a - 1.0).abs() < 1e-5 && t.b.abs() < 1e-5, "{t:?}");
        assert!(t.tx.abs() < 1e-3 && t.ty.abs() < 1e-3, "{t:?}");
    }

    #[test]
    fn a_rotated_scaled_template_recovers_its_transform() {
        let (angle, scale) = (0.3f32, 2.5f32);
        let known = Similarity {
            a: scale * angle.cos(),
            b: scale * angle.sin(),
            tx: 40.0,
            ty: -12.0,
        };
        let moved = TEMPLATE.map(|p| known.apply(p));
        let back = estimate(&moved, &TEMPLATE).unwrap();
        for (p, q) in moved.iter().zip(&TEMPLATE) {
            assert!(close(back.apply(*p), *q));
        }
        let inverse = known.inverse().unwrap();
        assert!((back.a - inverse.a).abs() < 1e-4 && (back.b - inverse.b).abs() < 1e-4);
        assert!(close([back.tx, back.ty], [inverse.tx, inverse.ty]));
    }

    #[test]
    fn warping_by_a_translation_moves_the_pixels() {
        let image = RgbImage::from_fn(200, 200, |x, y| Rgb([x as u8, y as u8, 7]));
        let shift = Similarity {
            a: 1.0,
            b: 0.0,
            tx: -50.0,
            ty: -20.0,
        };
        let crop = warp(&image, &shift).unwrap();
        assert_eq!(crop.dimensions(), (SIZE, SIZE));
        assert_eq!(crop.get_pixel(0, 0).0, [50, 20, 7]);
        assert_eq!(crop.get_pixel(10, 5).0, [60, 25, 7]);
        // Outside the source: black, as OpenCV's constant border.
        let far = Similarity { tx: 150.0, ..shift };
        assert_eq!(warp(&image, &far).unwrap().get_pixel(0, 0).0, [0, 0, 0]);
    }

    #[test]
    fn degenerate_landmarks_have_no_alignment() {
        let image = RgbImage::new(10, 10);
        assert!(align(&image, &[[5.0, 5.0]; 5]).is_none());
    }
}
