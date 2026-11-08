//! How the main window's frame is drawn. On macOS the web title bar is the window's only title bar:
//! the native one is merged into it and the traffic lights sit over its left end. Other systems keep
//! their native frame until the designs draw window controls for them.

/// The web title bar's height, in points (`--titlebar-h` in web/src/styles/tokens.css).
const BAR_HEIGHT: f64 = 44.0;
/// A traffic light's diameter, and the bar's left padding the lights start at (the design's `.lights`).
const LIGHT_DIAMETER: f64 = 12.0;
const LIGHTS_LEFT: f64 = 16.0;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Frame {
    /// The web bar is the title bar; the traffic lights are placed at `x`, `y` from the window's top left.
    Unified { x: f64, y: f64 },
    /// The system's own title bar and window controls.
    Native,
}

pub fn frame_for(os: &str) -> Frame {
    match os {
        "macos" => Frame::Unified {
            x: LIGHTS_LEFT,
            // Vertically centred in the bar.
            y: (BAR_HEIGHT - LIGHT_DIAMETER) / 2.0,
        },
        _ => Frame::Native,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_draws_one_title_bar_with_the_lights_centred_in_it() {
        assert_eq!(frame_for("macos"), Frame::Unified { x: 16.0, y: 16.0 });
    }

    #[test]
    fn windows_and_linux_keep_their_native_frame() {
        assert_eq!(frame_for("windows"), Frame::Native);
        assert_eq!(frame_for("linux"), Frame::Native);
        assert_eq!(frame_for("freebsd"), Frame::Native);
    }

    #[test]
    fn the_light_centre_line_matches_the_web_bar_height() {
        let Frame::Unified { y, .. } = frame_for("macos") else {
            panic!("macOS uses the unified frame");
        };
        assert_eq!(y + LIGHT_DIAMETER / 2.0, BAR_HEIGHT / 2.0);
        let tokens = include_str!("../../../web/src/styles/tokens.css");
        assert!(tokens.contains(&format!("--titlebar-h: {}px;", BAR_HEIGHT as u32)));
    }
}
