//! Plain PNG and path helpers for the Windows capturer. No `cfg`, so every OS compiles and tests this code.

pub fn bgra_to_rgba(pixels: &mut [u8]) {
    let (chunks, _partial) = pixels.as_chunks_mut::<4>();
    for pixel in chunks {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
}

pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }
    let expected = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)?;
    if rgba.len() != expected {
        return None;
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
        writer.finish().ok()?;
    }
    Some(png)
}

pub fn app_name_from_path(path: &str) -> String {
    let name = file_name_of(path);
    let stem_len = name.len().saturating_sub(4);
    let is_exe = name
        .get(stem_len..)
        .is_some_and(|extension| extension.eq_ignore_ascii_case(".exe"));
    if is_exe {
        name[..stem_len].to_owned()
    } else {
        name
    }
}

pub fn file_name_of(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn bgra_pixels_swap_to_opaque_rgba_and_ignore_a_partial_pixel() {
        let mut pixels = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        bgra_to_rgba(&mut pixels);
        assert_eq!(pixels, [3, 2, 1, 255, 7, 6, 5, 255, 9, 10]);
    }

    #[test]
    fn png_round_trips_through_the_decoder() {
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 12, 34, 56, 78,
        ];
        let encoded = encode_png(2, 2, &rgba).expect("an RGBA image encodes");
        let decoder = png::Decoder::new(Cursor::new(encoded));
        let mut reader = decoder.read_info().expect("a PNG header reads");
        assert_eq!((reader.info().width, reader.info().height), (2, 2));
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
        let mut decoded = vec![0; reader.output_buffer_size().expect("the frame fits")];
        let frame = reader.next_frame(&mut decoded).expect("the frame decodes");
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(&decoded[..frame.buffer_size()], &rgba[..]);
    }

    #[test]
    fn zero_sides_and_wrong_lengths_do_not_encode() {
        assert!(encode_png(0, 1, &[]).is_none());
        assert!(encode_png(1, 0, &[]).is_none());
        assert!(encode_png(1, 1, &[0, 0, 0]).is_none());
        assert!(encode_png(1, 1, &[0, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn an_overflowing_size_does_not_panic() {
        assert!(encode_png(u32::MAX, u32::MAX, &[]).is_none());
    }

    #[test]
    fn app_names_come_from_both_path_styles() {
        assert_eq!(
            app_name_from_path("C:\\Program Files\\1Password\\1Password.exe"),
            "1Password"
        );
        assert_eq!(app_name_from_path("KeePassXC.EXE"), "KeePassXC");
        assert_eq!(app_name_from_path("/usr/bin/tool"), "tool");
        assert_eq!(app_name_from_path(""), "");
        assert_eq!(file_name_of("C:\\Windows\\notepad.exe"), "notepad.exe");
    }
}
