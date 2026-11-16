//! HEIC/HEIF/AVIF through libheif, registered as `image` decoding hooks so
//! `thumbs::decode` handles them like any other format.

/// Call once per process before decoding (the sandbox does it at start).
pub fn register() {
    libheif_rs::integration::image::register_all_decoding_hooks();
}
