//! Coloration syntaxique des configurations wg-quick dans l'éditeur.

use eframe::egui::{text::LayoutJob, Color32, Stroke};

use super::theme;

const SECTION: Color32 = Color32::from_rgb(0xc4, 0xb5, 0xfd);
const KEY: Color32 = Color32::from_rgb(0x67, 0xe8, 0xf9);
const VALUE: Color32 = Color32::from_rgb(0xe2, 0xe8, 0xf0);
const NUMBER: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
const COMMENT: Color32 = Color32::from_rgb(0x5b, 0x61, 0x89);
const DANGER: Color32 = theme::ERROR_TEXT;

pub const FONT_SIZE: f32 = 13.0;
pub const LINE_HEIGHT: f32 = 13.0 * 1.65;

fn fmt(color: Color32) -> eframe::egui::TextFormat {
    let mut f = theme::mono(FONT_SIZE, color);
    f.line_height = Some(LINE_HEIGHT);
    f
}

/// Mise en forme d'une configuration : sections, clés, valeurs, nombres,
/// commentaires, et directives ignorées (PreUp, PostUp…) barrées en rouge.
pub fn highlight(text: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            job.append("\n", 0.0, fmt(theme::TEXT_2));
        }
        let (code, comment) = match line.find(['#', ';']) {
            Some(i) => line.split_at(i),
            None => (line, ""),
        };
        let indent_len = code.len() - code.trim_start().len();
        job.append(&code[..indent_len], 0.0, fmt(theme::TEXT_2));
        let body = &code[indent_len..];
        let trimmed = body.trim_end();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            job.append(body, 0.0, fmt(SECTION));
        } else if let Some(eq) = body.find('=') {
            let (key, value) = (&body[..eq], &body[eq + 1..]);
            let k = key.trim().to_ascii_lowercase();
            if matches!(k.as_str(), "preup" | "postup" | "predown" | "postdown") {
                let mut f = fmt(DANGER);
                f.strikethrough = Stroke::new(1.0, theme::rgba(253, 164, 175, 0.5));
                job.append(body, 0.0, f);
            } else {
                job.append(key, 0.0, fmt(KEY));
                job.append("=", 0.0, fmt(theme::MUTED));
                let numeric = !value.trim().is_empty() && value.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | ' ' | '\t'));
                job.append(value, 0.0, fmt(if numeric { NUMBER } else { VALUE }));
            }
        } else {
            job.append(body, 0.0, fmt(theme::TEXT_2));
        }
        if !comment.is_empty() {
            let mut f = fmt(COMMENT);
            f.italics = true;
            job.append(comment, 0.0, f);
        }
    }
    job
}
