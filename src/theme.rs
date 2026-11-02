//! The colours of the program that carry a meaning, in one place: the colours of a grade, of a state
//! (on, warning, error) and of the accent. The renderer and every tab take them from here.

use egui::Color32;

pub const GREEN: Color32 = Color32::from_rgb(0x2e, 0xb8, 0x5c);
pub const LIGHT_GREEN: Color32 = Color32::from_rgb(0x8b, 0xc3, 0x4a);
pub const AMBER: Color32 = Color32::from_rgb(0xf2, 0xb1, 0x34);
pub const ORANGE: Color32 = Color32::from_rgb(0xf2, 0x7a, 0x1d);
pub const RED: Color32 = Color32::from_rgb(0xe0, 0x3e, 0x3e);
pub const ACCENT: Color32 = Color32::from_rgb(0x3d, 0x8b, 0xfd);
