//! Small previews for image clips.
//!
//! A captured screenshot is held as base64 in the clip's content and averages
//! nearly three megabytes. Handing fifty of those to the UI to draw postage
//! stamps is what made Clipz hang on startup, so each image clip gets one of
//! these instead, a few kilobytes apiece.

use crate::clipboard::{base64_decode, base64_encode};

/// Longest edge of a generated thumbnail, in pixels. The card previews are
/// about 40px, so this stays sharp on a retina display with room to spare.
const MAX_EDGE: u32 = 256;

/// Shrink a clip's image into a JPEG data URI.
///
/// Accepts either a bare base64 payload or a `data:image/...;base64,` URI, and
/// returns `None` for anything that is not a decodable image: a clip that
/// cannot be shrunk simply keeps no thumbnail.
pub fn shrink(content: &str) -> Option<String> {
    let payload = match content.find(";base64,") {
        Some(index) => &content[index + ";base64,".len()..],
        None => content,
    };

    let bytes = base64_decode(payload).ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let small = image.thumbnail(MAX_EDGE, MAX_EDGE);

    let mut jpeg = Vec::new();
    small
        .to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut jpeg, 70,
        ))
        .ok()?;

    Some(format!("data:image/jpeg;base64,{}", base64_encode(&jpeg)))
}
