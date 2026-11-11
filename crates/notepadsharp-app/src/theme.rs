//! Colors and widget styling.
//!
//! Two palettes ship: a Notepad++-style dark scheme and a light one. Token colors are
//! looked up by [`TokenKind`], so adding a language never touches the UI.

use egui::{Color32, Context, FontFamily, FontId, TextStyle, Visuals};
use notepadsharp_core::{Theme, TokenKind};

/// Every color the editor uses.
#[derive(Clone, Debug)]
pub struct Palette {
    pub background: Color32,
    pub panel: Color32,
    pub gutter_background: Color32,
    pub gutter_text: Color32,
    pub current_line: Color32,
    pub match_fill: Color32,
    pub active_match_fill: Color32,
    pub text: Color32,
    pub keyword: Color32,
    pub type_name: Color32,
    pub literal: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub function: Color32,
    pub macro_token: Color32,
    pub attribute: Color32,
    pub operator: Color32,
    pub tag: Color32,
    pub tag_name: Color32,
    pub heading: Color32,
    pub link: Color32,
    pub key: Color32,
    pub accent: Color32,
    pub error: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            background: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            panel: Color32::from_rgb(0x25, 0x25, 0x26),
            gutter_background: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            gutter_text: Color32::from_rgb(0x85, 0x85, 0x85),
            current_line: Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x12),
            match_fill: Color32::from_rgba_unmultiplied(0xFF, 0xD7, 0x00, 0x50),
            active_match_fill: Color32::from_rgba_unmultiplied(0xFF, 0x96, 0x32, 0x80),
            text: Color32::from_rgb(0xD4, 0xD4, 0xD4),
            keyword: Color32::from_rgb(0x56, 0x9C, 0xD6),
            type_name: Color32::from_rgb(0x4E, 0xC9, 0xB0),
            literal: Color32::from_rgb(0xB5, 0xCE, 0xA8),
            string: Color32::from_rgb(0xCE, 0x91, 0x78),
            number: Color32::from_rgb(0xB5, 0xCE, 0xA8),
            comment: Color32::from_rgb(0x6A, 0x99, 0x55),
            function: Color32::from_rgb(0xDC, 0xDC, 0xAA),
            macro_token: Color32::from_rgb(0xC5, 0x86, 0xC0),
            attribute: Color32::from_rgb(0x4F, 0xC1, 0xFF),
            operator: Color32::from_rgb(0xD4, 0xD4, 0xD4),
            tag: Color32::from_rgb(0x56, 0x9C, 0xD6),
            tag_name: Color32::from_rgb(0x4E, 0xC9, 0xB0),
            heading: Color32::from_rgb(0x4F, 0xC1, 0xFF),
            link: Color32::from_rgb(0x37, 0x94, 0xFF),
            key: Color32::from_rgb(0x9C, 0xDC, 0xFE),
            accent: Color32::from_rgb(0x00, 0x7A, 0xCC),
            error: Color32::from_rgb(0xF4, 0x87, 0x71),
        }
    }

    pub fn light() -> Self {
        Self {
            background: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            panel: Color32::from_rgb(0xF3, 0xF3, 0xF3),
            gutter_background: Color32::from_rgb(0xF7, 0xF7, 0xF7),
            gutter_text: Color32::from_rgb(0x8A, 0x8A, 0x8A),
            current_line: Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 0x10),
            match_fill: Color32::from_rgba_unmultiplied(0xFF, 0xC8, 0x00, 0x60),
            active_match_fill: Color32::from_rgba_unmultiplied(0xFF, 0x96, 0x32, 0x90),
            text: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            keyword: Color32::from_rgb(0x00, 0x00, 0xCC),
            type_name: Color32::from_rgb(0x26, 0x7F, 0x99),
            literal: Color32::from_rgb(0x09, 0x86, 0x58),
            string: Color32::from_rgb(0xA3, 0x15, 0x15),
            number: Color32::from_rgb(0x09, 0x86, 0x58),
            comment: Color32::from_rgb(0x00, 0x80, 0x00),
            function: Color32::from_rgb(0x79, 0x5E, 0x26),
            macro_token: Color32::from_rgb(0xAF, 0x00, 0xDB),
            attribute: Color32::from_rgb(0x00, 0x70, 0xC1),
            operator: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            tag: Color32::from_rgb(0x80, 0x00, 0x00),
            tag_name: Color32::from_rgb(0x80, 0x00, 0x00),
            heading: Color32::from_rgb(0x00, 0x00, 0xCC),
            link: Color32::from_rgb(0x05, 0x63, 0xC1),
            key: Color32::from_rgb(0x04, 0x51, 0xA5),
            accent: Color32::from_rgb(0x00, 0x66, 0xCC),
            error: Color32::from_rgb(0xCD, 0x31, 0x31),
        }
    }

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::dark(),
            Theme::Light => Self::light(),
        }
    }

    /// The color for a highlighted token.
    pub fn token(&self, kind: TokenKind) -> Color32 {
        match kind {
            TokenKind::Keyword => self.keyword,
            TokenKind::Type => self.type_name,
            TokenKind::Literal => self.literal,
            TokenKind::String => self.string,
            TokenKind::Number => self.number,
            TokenKind::Comment => self.comment,
            TokenKind::Function => self.function,
            TokenKind::Macro => self.macro_token,
            TokenKind::Attribute => self.attribute,
            TokenKind::Operator => self.operator,
            TokenKind::Tag => self.tag,
            TokenKind::TagName => self.tag_name,
            TokenKind::Heading => self.heading,
            TokenKind::Link => self.link,
            TokenKind::Key => self.key,
        }
    }
}

/// Installs the palette and font sizes on a context.
pub fn apply(ctx: &Context, theme: Theme, font_size: f32) {
    let palette = Palette::for_theme(theme);

    let mut visuals = match theme {
        Theme::Dark => Visuals::dark(),
        Theme::Light => Visuals::light(),
    };
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.panel;
    visuals.faint_bg_color = palette.panel;
    visuals.extreme_bg_color = palette.background;
    visuals.override_text_color = Some(palette.text);
    visuals.selection.bg_fill = palette.accent.gamma_multiply(0.55);
    visuals.hyperlink_color = palette.link;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    let ui_size = (font_size - 1.0).clamp(9.0, 20.0);
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(ui_size - 1.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(ui_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(ui_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(ui_size + 3.0, FontFamily::Proportional),
        ),
        (TextStyle::Monospace, FontId::monospace(font_size)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    ctx.set_style(style);
}
