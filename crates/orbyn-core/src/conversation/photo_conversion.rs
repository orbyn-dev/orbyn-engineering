//! Photo conversion for the composer (parity B8, docs/attachments-transcripts.md, "Photo conversion").
//! A HEIC photo cannot come back as a JPEG yet (see below), and a PNG, JPEG or WebP over the image
//! limit is made smaller, so the engine reads a photo it can. The work is bounded before it starts:
//! the input size is checked first, the declared pixels are capped before a decode allocates, and
//! every answer is written again from decoded pixels, so no metadata (EXIF GPS) travels with it.

use std::io::Cursor;
use std::sync::{Condvar, Mutex};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use orbyn_contracts::{ConvertedPhoto, PhotoAction};
use serde_json::Value;

use crate::app::Core;
use crate::error::{CoreError, Result};

use super::composer::{MAX_IMAGE_BYTES, safe_name};

/// The largest photo this route accepts before any work; anything bigger is `413 file_too_large`.
pub const MAX_PHOTO_INPUT_BYTES: u64 = 50 * 1024 * 1024;
/// The most pixels a photo may decode to. A 100-megapixel photo is 400 MiB as RGBA, so the cap
/// refuses a small file that would expand into memory before a decoder allocates anything.
pub const MAX_PHOTO_PIXELS: u64 = 100_000_000;
/// What one decode may allocate. A photo at [`MAX_PHOTO_PIXELS`] needs 400 MiB as RGBA.
const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;
/// Bounds concurrent decoded photos across all Core instances in the service process.
pub const MAX_CONCURRENT_PHOTOS: usize = 2;
static PHOTO_SLOTS: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());
#[cfg(test)]
static PHOTO_WAIT_OBSERVER: Mutex<Option<std::sync::mpsc::Sender<()>>> = Mutex::new(None);

/// A slot stays held until all decoded and intermediate images have been dropped.
struct PhotoPermit;

impl PhotoPermit {
    fn acquire() -> Self {
        let (count, available) = &PHOTO_SLOTS;
        let mut count = count.lock().unwrap_or_else(|error| error.into_inner());
        while *count >= MAX_CONCURRENT_PHOTOS {
            #[cfg(test)]
            if let Some(observer) = PHOTO_WAIT_OBSERVER.lock().unwrap().as_ref() {
                let _ = observer.send(());
            }
            count = available
                .wait(count)
                .unwrap_or_else(|error| error.into_inner());
        }
        *count += 1;
        Self
    }
}

impl Drop for PhotoPermit {
    fn drop(&mut self) {
        let (count, available) = &PHOTO_SLOTS;
        let mut count = count.lock().unwrap_or_else(|error| error.into_inner());
        *count -= 1;
        available.notify_one();
    }
}
/// Quality of a JPEG written again: high enough that a photo shown to the engine still looks right.
const JPEG_QUALITY: u8 = 82;

/// The input kinds this route writes again, decided from the bytes alone. `image` is built
/// with the JPEG, PNG and WebP codecs (Cargo.toml); WebP is decoded by the pure-Rust
/// `image-webp` crate behind that feature, not by a C library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhotoKind {
    Jpeg,
    Png,
    WebP,
}

impl PhotoKind {
    fn format(self) -> ImageFormat {
        match self {
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Png => ImageFormat::Png,
            Self::WebP => ImageFormat::WebP,
        }
    }

    fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::WebP => "image/webp",
        }
    }

    fn ending(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
        }
    }
}

fn unreadable() -> CoreError {
    CoreError::Refused {
        status: 422,
        code: "unreadable_image",
        message: "This image could not be read.".into(),
        detail: Value::Null,
    }
}

fn still_too_large() -> CoreError {
    CoreError::Refused {
        status: 422,
        code: "still_too_large",
        message: "Still over 10 MiB after shrinking.".into(),
        detail: Value::Null,
    }
}

fn unsupported(message: &str) -> CoreError {
    CoreError::Refused {
        status: 415,
        code: "unsupported_type",
        message: message.to_owned(),
        detail: Value::Null,
    }
}

/// Whether the bytes begin with an ISO base media brand HEIC and HEIF use. The `image` crate has no
/// decoder for them, so they are named here rather than reported as an unreadable image.
fn heic_brand(bytes: &[u8]) -> bool {
    const BRANDS: [&[u8]; 11] = [
        b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"heif", b"mif1",
        b"msf1",
    ];
    bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && BRANDS.contains(&&bytes[8..12])
}

/// The kind from the bytes, never from the claimed type. A recognised image this route does not
/// write (a GIF would need a codec this build does not carry, and shrinking one would drop its
/// frames) and a HEIC or HEIF are refused with their own code; anything that is not an image at
/// all is unreadable.
fn kind_of(bytes: &[u8]) -> Result<PhotoKind> {
    match image::guess_format(bytes) {
        Ok(ImageFormat::Jpeg) => Ok(PhotoKind::Jpeg),
        Ok(ImageFormat::Png) => Ok(PhotoKind::Png),
        Ok(ImageFormat::WebP) => Ok(PhotoKind::WebP),
        Ok(_) => Err(unsupported(
            "This image type cannot be converted here. Send a JPEG, PNG or WebP.",
        )),
        Err(_) if heic_brand(bytes) => Err(unsupported(
            "HEIC photos need a decoder this build does not carry. Save it as JPEG, PNG or WebP first.",
        )),
        Err(_) => Err(unreadable()),
    }
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

/// A reader that carries the decode limits, so neither the header read nor the decode can allocate
/// past them.
fn reader(bytes: &[u8], format: ImageFormat) -> ImageReader<Cursor<&[u8]>> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    reader
}

/// The declared pixels first, so a small file that would expand into memory never reaches a decoder,
/// then the pixels themselves.
fn decode(bytes: &[u8], format: ImageFormat) -> Result<DynamicImage> {
    let (width, height) = reader(bytes, format)
        .into_dimensions()
        .map_err(|_| unreadable())?;
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_PHOTO_PIXELS {
        return Err(CoreError::Refused {
            status: 422,
            code: "unreadable_image",
            message: format!(
                "That photo is larger than {} megapixels. Shrink it before sending it.",
                MAX_PHOTO_PIXELS / 1_000_000
            ),
            detail: Value::Null,
        });
    }
    reader(bytes, format).decode().map_err(|_| unreadable())
}

/// Writes the decoded pixels in the kind's own format. Nothing of the original file is copied, so its
/// metadata, including a GPS location, is not in the answer.
fn encode(image: &DynamicImage, kind: PhotoKind) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    let written = match kind {
        // A JPEG has no alpha channel, so the colour is what the engine should see.
        PhotoKind::Jpeg => JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .encode_image(&DynamicImage::ImageRgb8(image.to_rgb8())),
        // PNG and WebP keep alpha; both encoders are lossless, so a photo is not degraded
        // by being written again.
        PhotoKind::Png | PhotoKind::WebP => image.to_rgba8().write_to(&mut out, kind.format()),
    };
    written.map_err(|error| CoreError::Internal(format!("photo could not be written: {error}")))?;
    Ok(out.into_inner())
}

/// Halves the photo until it fits, and refuses once it is one pixel and still over. Every iteration
/// strictly shrinks it, so this ends.
fn shrink(mut image: DynamicImage, kind: PhotoKind, limit: u64) -> Result<(Vec<u8>, PhotoAction)> {
    loop {
        let bytes = encode(&image, kind)?;
        if bytes.len() as u64 <= limit {
            return Ok((bytes, PhotoAction::Shrunk));
        }
        let (width, height) = (image.width(), image.height());
        if width <= 1 && height <= 1 {
            return Err(still_too_large());
        }
        image = image.resize_exact(
            (width / 2).max(1),
            (height / 2).max(1),
            image::imageops::FilterType::Lanczos3,
        );
    }
}

/// The stored bytes and what was done to get them. A photo already within the limit is only written
/// again, which is what drops its metadata; if writing it again is itself over the limit it is shrunk.
fn write_within(
    image: DynamicImage,
    kind: PhotoKind,
    limit: u64,
    must_shrink: bool,
) -> Result<(Vec<u8>, PhotoAction)> {
    if must_shrink {
        return shrink(image, kind, limit);
    }
    let bytes = encode(&image, kind)?;
    if bytes.len() as u64 <= limit {
        return Ok((bytes, PhotoAction::None));
    }
    shrink(image, kind, limit)
}

/// The stored name: the label's stem plus the kind's ending, so a photo no longer claims one it is not.
fn name_for(label: &str, kind: PhotoKind) -> String {
    let stem = label
        .rsplit_once('.')
        .map_or(label, |(stem, _)| stem)
        .trim();
    let stem = if stem.is_empty() { "photo" } else { stem };
    format!("{stem}.{}", kind.ending())
}

/// Keep the permit over decoding and encoding, including all intermediate images.
fn convert_pixels(
    bytes: &[u8],
    kind: PhotoKind,
    must_shrink: bool,
    decoder: impl FnOnce(&[u8], ImageFormat) -> Result<DynamicImage>,
) -> Result<(Vec<u8>, PhotoAction)> {
    let _permit = PhotoPermit::acquire();
    let image = decoder(bytes, kind.format())?;
    write_within(image, kind, MAX_IMAGE_BYTES, must_shrink)
}

impl Core {
    /// Converts or shrinks one photo and stores the answer as this thread's attachment, so the run
    /// sends the stored photo rather than the one the person picked. The pick is only a label and a
    /// size: no path from the request is read, and the bytes land under a service-made id.
    pub fn convert_photo(
        &self,
        thread_id: &str,
        label: &str,
        bytes: &[u8],
    ) -> Result<ConvertedPhoto> {
        self.store.thread(thread_id)?;
        let original_size = bytes.len() as u64;
        if original_size > MAX_PHOTO_INPUT_BYTES {
            return Err(CoreError::FileTooLarge {
                size: original_size,
                limit: MAX_PHOTO_INPUT_BYTES,
            });
        }
        let original_name = safe_name(label);
        let kind = kind_of(bytes)?;
        let must_shrink = original_size > MAX_IMAGE_BYTES;
        let (stored, action) = convert_pixels(bytes, kind, must_shrink, decode)?;
        let attachment = self.add_attachment(
            thread_id,
            &name_for(&original_name, kind),
            kind.mime(),
            &stored,
        )?;
        Ok(ConvertedPhoto {
            id: attachment.id,
            name: attachment.name,
            size: attachment.size,
            original_name,
            original_size,
            action,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    #[test]
    fn only_two_photo_conversions_enter_at_once_and_waiters_continue() {
        use std::sync::{Arc, Barrier, mpsc};
        use std::time::Duration;

        let (entered_tx, entered_rx) = mpsc::channel();
        let release = Arc::new(Barrier::new(3));
        let bytes = Arc::new(written(&photo(8, 8), PhotoKind::Png));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let entered = entered_tx.clone();
            let release = Arc::clone(&release);
            let bytes = Arc::clone(&bytes);
            workers.push(std::thread::spawn(move || {
                convert_pixels(&bytes, PhotoKind::Png, false, |bytes, format| {
                    entered.send(()).unwrap();
                    release.wait();
                    decode(bytes, format)
                })
                .unwrap();
            }));
        }
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (waiting_tx, waiting_rx) = mpsc::channel();
        *PHOTO_WAIT_OBSERVER.lock().unwrap() = Some(waiting_tx);
        workers.push(std::thread::spawn(move || {
            convert_pixels(&bytes, PhotoKind::Png, false, |bytes, format| {
                entered_tx.send(()).unwrap();
                decode(bytes, format)
            })
            .unwrap();
        }));
        waiting_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        *PHOTO_WAIT_OBSERVER.lock().unwrap() = None;
        assert!(matches!(
            entered_rx.recv_timeout(Duration::from_millis(200)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        release.wait();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        for worker in workers {
            worker.join().unwrap();
        }
    }

    /// An image that does not compress to nothing, so a small limit forces a real shrink.
    fn photo(width: u32, height: u32) -> DynamicImage {
        let rgb: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([
                ((x * 7 + y * 13) % 256) as u8,
                ((x * 31 + y * 3) % 256) as u8,
                ((x * 5 + y * 17) % 256) as u8,
            ])
        });
        DynamicImage::ImageRgb8(rgb)
    }

    fn written(image: &DynamicImage, kind: PhotoKind) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        match kind {
            PhotoKind::Jpeg => DynamicImage::ImageRgb8(image.to_rgb8()),
            PhotoKind::Png | PhotoKind::WebP => DynamicImage::ImageRgba8(image.to_rgba8()),
        }
        .write_to(&mut out, kind.format())
        .unwrap();
        out.into_inner()
    }

    /// A lossless WebP fixture, written here so no binary is committed.
    fn webp(image: &DynamicImage) -> Vec<u8> {
        written(image, PhotoKind::WebP)
    }

    /// A lossless WebP whose header declares `width` x `height` while its bitstream stays that
    /// of a small photo: the shape of a decompression bomb, small on disk and huge to decode.
    fn webp_declaring(image: &DynamicImage, width: u32, height: u32) -> Vec<u8> {
        let mut bytes = webp(image);
        // The `VP8L` chunk is "VP8L", its u32 payload length, then the payload: the 0x2F
        // signature byte and a u32 of width-1 (14 bits), height-1 (14 bits), an alpha bit and a
        // 3-bit version. Only that u32 changes, so the chunk length and payload stay valid.
        let tag = bytes
            .windows(4)
            .position(|window| window == b"VP8L")
            .expect("a lossless WebP chunk");
        let header = tag + 9;
        assert_eq!(bytes[header - 1], 0x2F, "the VP8L signature byte");
        let packed = u32::from_le_bytes([
            bytes[header],
            bytes[header + 1],
            bytes[header + 2],
            bytes[header + 3],
        ]);
        // Keep the alpha bit and the version bits; rewrite the two 14-bit dimensions.
        let kept = packed & 0xF000_0000;
        let packed = ((width - 1) & 0x3FFF) | (((height - 1) & 0x3FFF) << 14) | kept;
        bytes[header..header + 4].copy_from_slice(&packed.to_le_bytes());
        bytes
    }

    /// A structurally valid EXIF APP1 segment with a GPS IFD, inserted after the JPEG's SOI the way a
    /// camera writes one.
    fn with_exif_gps(jpeg: Vec<u8>) -> Vec<u8> {
        let mut exif = b"Exif\0\0".to_vec();
        // TIFF header (little-endian), IFD0 pointing at a GPS IFD, then that IFD and its rationals.
        exif.extend_from_slice(b"II");
        exif.extend_from_slice(&42u16.to_le_bytes());
        exif.extend_from_slice(&8u32.to_le_bytes());
        const GPS_IFD: u32 = 26;
        // IFD0: one entry, GPSInfo (0x8825, LONG), no next IFD.
        exif.extend_from_slice(&1u16.to_le_bytes());
        exif.extend_from_slice(&0x8825u16.to_le_bytes());
        exif.extend_from_slice(&4u16.to_le_bytes());
        exif.extend_from_slice(&1u32.to_le_bytes());
        exif.extend_from_slice(&GPS_IFD.to_le_bytes());
        exif.extend_from_slice(&0u32.to_le_bytes());
        // GPS IFD: latitude reference "N", then a latitude needing three rationals (48°51'24.5").
        let rationals: u32 = GPS_IFD + 2 + 2 * 12 + 4;
        exif.extend_from_slice(&2u16.to_le_bytes());
        exif.extend_from_slice(&0x0001u16.to_le_bytes());
        exif.extend_from_slice(&2u16.to_le_bytes());
        exif.extend_from_slice(&2u32.to_le_bytes());
        exif.extend_from_slice(b"N\0\0\0");
        exif.extend_from_slice(&0x0002u16.to_le_bytes());
        exif.extend_from_slice(&5u16.to_le_bytes());
        exif.extend_from_slice(&3u32.to_le_bytes());
        exif.extend_from_slice(&rationals.to_le_bytes());
        exif.extend_from_slice(&0u32.to_le_bytes());
        for (numerator, denominator) in [(48u32, 1u32), (51, 1), (245, 10)] {
            exif.extend_from_slice(&numerator.to_le_bytes());
            exif.extend_from_slice(&denominator.to_le_bytes());
        }
        let mut segment = vec![0xFF, 0xE1];
        segment.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        segment.extend_from_slice(&exif);
        let mut out = Vec::with_capacity(jpeg.len() + segment.len());
        out.extend_from_slice(&jpeg[..2]);
        out.extend_from_slice(&segment);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    /// A PNG whose header declares `width` × `height` with a valid CRC, so a decoder would try to
    /// allocate it: the shape of a decompression bomb.
    fn png_declaring(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = written(&photo(8, 8), PhotoKind::Png);
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        let crc = crc32(&bytes[12..29]);
        bytes[29..33].copy_from_slice(&crc.to_be_bytes());
        bytes
    }

    /// CRC-32/ISO-HDLC, so the patched header is one a real decoder accepts.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    fn code(error: &CoreError) -> &str {
        error.code()
    }

    #[test]
    fn the_type_comes_from_the_bytes_and_a_non_image_is_unreadable() {
        assert_eq!(
            kind_of(&written(&photo(8, 8), PhotoKind::Png)).unwrap(),
            PhotoKind::Png
        );
        assert_eq!(
            kind_of(&written(&photo(8, 8), PhotoKind::Jpeg)).unwrap(),
            PhotoKind::Jpeg
        );
        assert_eq!(kind_of(&webp(&photo(8, 8))).unwrap(), PhotoKind::WebP);
        let plain = b"this is not an image, whatever the browser said";
        assert_eq!(code(&kind_of(plain).unwrap_err()), "unreadable_image");
    }

    #[test]
    fn a_heic_is_refused_by_its_brand_rather_than_as_an_unreadable_file() {
        let mut heic = vec![0u8, 0, 0, 0x18];
        heic.extend_from_slice(b"ftypheic");
        heic.extend_from_slice(&[0u8; 16]);
        let error = kind_of(&heic).unwrap_err();
        assert_eq!(code(&error), "unsupported_type");
        assert_eq!(error.status(), 415);
        assert!(error.to_string().contains("HEIC"), "{error}");
        let mut heif = vec![0u8, 0, 0, 0x18];
        heif.extend_from_slice(b"ftypmif1");
        heif.extend_from_slice(&[0u8; 16]);
        assert_eq!(code(&kind_of(&heif).unwrap_err()), "unsupported_type");
    }

    #[test]
    fn an_image_this_route_does_not_write_is_unsupported() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&[0u8; 16]);
        let error = kind_of(&gif).unwrap_err();
        assert_eq!(code(&error), "unsupported_type");
        assert_eq!(error.status(), 415);
    }

    #[test]
    fn a_webp_is_decoded_and_written_again() {
        let bytes = webp(&photo(64, 48));
        let kind = kind_of(&bytes).unwrap();
        assert_eq!(kind, PhotoKind::WebP);
        assert_eq!(kind.mime(), "image/webp");
        assert_eq!(kind.ending(), "webp");
        let decoded = decode(&bytes, kind.format()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 48));
        let (stored, action) = write_within(decoded, kind, MAX_IMAGE_BYTES, false).unwrap();
        assert_eq!(action, PhotoAction::None);
        assert_eq!(kind_of(&stored).unwrap(), PhotoKind::WebP);
        let pixels = image::load_from_memory_with_format(&stored, ImageFormat::WebP)
            .unwrap_or_else(|error| panic!("the answer is a WebP: {error}"));
        assert_eq!((pixels.width(), pixels.height()), (64, 48));
    }

    #[test]
    fn an_oversized_webp_is_shrunk_until_it_fits() {
        let kind = PhotoKind::WebP;
        let bytes = webp(&photo(512, 512));
        // Lossless WebP compresses this fixture harder than PNG does, so the limit is taken from
        // the fixture: half its size starts it over the limit and a 1x1 photo fits under it.
        let limit = (bytes.len() as u64 / 2).max(64);
        assert!(
            bytes.len() as u64 > limit,
            "the {} byte fixture starts over the limit",
            bytes.len()
        );
        let (stored, action) =
            write_within(decode(&bytes, kind.format()).unwrap(), kind, limit, true).unwrap();
        assert_eq!(action, PhotoAction::Shrunk);
        assert!(stored.len() as u64 <= limit, "{} bytes", stored.len());
        assert_eq!(kind_of(&stored).unwrap(), PhotoKind::WebP);
    }

    #[test]
    fn a_damaged_webp_fails_cleanly() {
        let mut bytes = webp(&photo(64, 64));
        bytes.truncate(bytes.len() / 2);
        let kind = kind_of(&bytes).unwrap();
        assert_eq!(kind, PhotoKind::WebP);
        let error = decode(&bytes, kind.format()).unwrap_err();
        assert_eq!(code(&error), "unreadable_image");
    }

    #[test]
    fn a_webp_that_declares_too_many_pixels_is_refused_before_a_decode() {
        let bytes = webp_declaring(&photo(8, 8), 12_000, 12_000);
        assert!(bytes.len() < 100_000, "the fixture is small on disk");
        let error = decode(&bytes, ImageFormat::WebP).unwrap_err();
        assert_eq!(code(&error), "unreadable_image");
        assert!(error.to_string().contains("megapixels"), "{error}");
    }

    #[test]
    fn a_photo_within_the_limit_is_written_again_without_its_exif_location() {
        let with_location = with_exif_gps(written(&photo(64, 48), PhotoKind::Jpeg));
        assert!(
            with_location.windows(2).any(|pair| pair == [0xFF, 0xE1]),
            "the fixture carries an APP1 segment"
        );
        let kind = kind_of(&with_location).unwrap();
        let decoded = decode(&with_location, kind.format()).unwrap();
        let (stored, action) = write_within(decoded, kind, MAX_IMAGE_BYTES, false).unwrap();
        assert_eq!(action, PhotoAction::None);
        assert!(
            !stored.windows(2).any(|pair| pair == [0xFF, 0xE1]),
            "the answer carries no EXIF segment"
        );
        assert!(!stored.windows(4).any(|window| window == b"Exif"));
        assert_eq!(kind_of(&stored).unwrap(), PhotoKind::Jpeg);
    }

    #[test]
    fn an_oversized_photo_is_shrunk_until_it_fits() {
        let kind = PhotoKind::Png;
        let bytes = written(&photo(512, 512), kind);
        let limit = 64 * 1024;
        assert!(
            bytes.len() as u64 > limit,
            "the fixture starts over the limit"
        );
        let (stored, action) =
            write_within(decode(&bytes, kind.format()).unwrap(), kind, limit, true).unwrap();
        assert_eq!(action, PhotoAction::Shrunk);
        assert!(stored.len() as u64 <= limit, "{} bytes", stored.len());
        assert_eq!(kind_of(&stored).unwrap(), PhotoKind::Png);
    }

    #[test]
    fn a_photo_that_cannot_fit_even_at_one_pixel_is_refused() {
        let kind = PhotoKind::Png;
        let bytes = written(&photo(64, 64), kind);
        let error =
            write_within(decode(&bytes, kind.format()).unwrap(), kind, 1, true).unwrap_err();
        assert_eq!(code(&error), "still_too_large");
        assert_eq!(error.status(), 422);
    }

    #[test]
    fn a_small_file_that_would_expand_is_refused_before_a_decode() {
        let bytes = png_declaring(20_000, 20_000);
        assert!(bytes.len() < 100_000, "the fixture is small on disk");
        let error = decode(&bytes, ImageFormat::Png).unwrap_err();
        assert_eq!(code(&error), "unreadable_image");
        assert!(error.to_string().contains("megapixels"), "{error}");
    }

    #[test]
    fn the_stored_name_takes_the_new_ending() {
        assert_eq!(name_for("IMG_4021.HEIC", PhotoKind::Jpeg), "IMG_4021.jpg");
        assert_eq!(name_for("shot", PhotoKind::Png), "shot.png");
        assert_eq!(
            name_for("archive.photo.PNG", PhotoKind::Png),
            "archive.photo.png"
        );
        assert_eq!(name_for(" . ", PhotoKind::Jpeg), "photo.jpg");
    }
}
