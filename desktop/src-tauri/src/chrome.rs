//! How the main window's frame is drawn. On macOS the web title bar is the window's only title bar:
//! the native one is merged into it and the traffic lights sit over its left end. Other systems keep
//! their native frame until the designs draw window controls for them.

// Only macOS reads these outside the tests; the other systems keep their native frame.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

/// The web title bar's height, in points (`--titlebar-h` in web/src/styles/tokens.css).
const BAR_HEIGHT: f64 = 44.0;
/// A traffic light's drawn diameter and the bar's left padding the first one starts at (the design's `.lights`).
const LIGHT_DIAMETER: f64 = 12.0;
const LIGHTS_LEFT: f64 = 16.0;
/// AppKit's button frame is 14pt wide around the 12pt dot, so the frame starts 1pt before the dot.
const LIGHT_FRAME_PAD: f64 = (14.0 - LIGHT_DIAMETER) / 2.0;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Frame {
    /// The web bar is the title bar. `x` is the first button's left edge from the window's left. `y` is the
    /// distance from the window's top to the buttons' vertical centre: the windowing layer (tao) treats it so,
    /// which is why it equals half the bar and not the top of a button.
    Unified { x: f64, y: f64 },
    /// The system's own title bar and window controls.
    Native,
}

pub fn frame_for(os: &str) -> Frame {
    match os {
        "macos" => Frame::Unified {
            x: LIGHTS_LEFT - LIGHT_FRAME_PAD,
            // On the bar's centre line, with the sidebar toggle, back and forward and the page title.
            y: BAR_HEIGHT / 2.0,
        },
        _ => Frame::Native,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_draws_one_title_bar_with_the_lights_centred_in_it() {
        assert_eq!(frame_for("macos"), Frame::Unified { x: 15.0, y: 22.0 });
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
        assert_eq!(y, BAR_HEIGHT / 2.0);
        let tokens = include_str!("../../../web/src/styles/tokens.css");
        assert!(tokens.contains(&format!("--titlebar-h: {}px;", BAR_HEIGHT as u32)));
    }
}
