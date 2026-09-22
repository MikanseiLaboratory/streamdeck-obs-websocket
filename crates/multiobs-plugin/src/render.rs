use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::kind::ActionKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentState {
    Active,
    Inactive,
    Intermediate,
    Neutral,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub label: String,
    pub color: String,
    pub state: SegmentState,
}

pub fn key_image(kind: ActionKind, segments: &[Segment], foreground: &str) -> String {
    let strip = if segments.is_empty() {
        0.0
    } else if segments.len() <= 6 {
        22.0
    } else {
        10.0
    };
    let width = if segments.is_empty() {
        0.0
    } else {
        144.0 / segments.len() as f32
    };
    let mut bars = String::new();
    for (index, segment) in segments.iter().enumerate() {
        let x = width * index as f32;
        let fill = fill_for(segment);
        bars.push_str(&format!(
            r#"<rect x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{strip:.2}" fill="{fill}"/>"#,
            y = 144.0 - strip,
        ));
        if segment.state == SegmentState::Unavailable {
            bars.push_str(&format!(
                r#"<rect x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{strip:.2}" fill="url(#hatch)"/>"#,
                y = 144.0 - strip,
            ));
        }
        if segments.len() <= 6 && strip > 0.0 {
            let label = escape(&initial(&segment.label));
            bars.push_str(&format!(
                r#"<text x="{tx:.2}" y="138" text-anchor="middle" font-size="12" font-family="sans-serif" font-weight="700" fill="{foreground}">{label}</text>"#,
                tx = x + width / 2.0,
            ));
        }
    }

    let icon = icon_path(kind);
    let hatch = "#111";
    let background = "#1b1f27";
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144"><defs><pattern id="hatch" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="6" stroke="{hatch}" stroke-width="2"/></pattern></defs><rect width="144" height="144" rx="18" fill="{background}"/><g fill="none" stroke="{foreground}" stroke-width="6" stroke-linecap="round" stroke-linejoin="round" transform="translate(40 28)">{icon}</g>{bars}</svg>"#
    );
    format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg.as_bytes()))
}

fn fill_for(segment: &Segment) -> String {
    match segment.state {
        SegmentState::Active | SegmentState::Neutral => segment.color.clone(),
        SegmentState::Inactive => "#2a3140".into(),
        SegmentState::Intermediate => "#d08a2f".into(),
        SegmentState::Unavailable => "#3a241f".into(),
    }
}

fn initial(label: &str) -> String {
    label
        .chars()
        .find(|char| !char.is_whitespace())
        .map(|char| char.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn icon_path(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::Stream => r#"<path d="M8 18h28l10 10V18h10v28H8z"/>"#,
        ActionKind::Record => r#"<circle cx="32" cy="32" r="16"/>"#,
        ActionKind::RecordPause => r#"<path d="M20 16v32M44 16v32"/>"#,
        ActionKind::Replay => r#"<path d="M16 32a16 16 0 1 0 4-10M16 14v10h10"/>"#,
        ActionKind::SaveReplay => r#"<path d="M16 44V20h32v24M24 20v12h16"/>"#,
        ActionKind::VirtualCam => r#"<rect x="10" y="18" width="30" height="24" rx="4"/><path d="M40 26l14-6v24l-14-6"/>"#,
        ActionKind::StudioMode => r#"<rect x="8" y="16" width="20" height="28"/><rect x="36" y="16" width="20" height="28"/>"#,
        ActionKind::StudioTransition => r#"<path d="M14 32h36M38 22l12 10-12 10"/>"#,
        ActionKind::Scene => r#"<rect x="10" y="14" width="44" height="32" rx="4"/>"#,
        ActionKind::Source => r#"<rect x="12" y="16" width="18" height="18"/><rect x="34" y="30" width="18" height="18"/>"#,
        ActionKind::Mute => r#"<path d="M24 14v20M18 24a14 10 0 0 0 12 0M16 46l32-28"/>"#,
        ActionKind::Filter => r#"<path d="M12 16h40l-14 18v14l-12 6V34z"/>"#,
        ActionKind::Collection => r#"<rect x="12" y="18" width="28" height="28"/><path d="M24 18V12h28v28h-6"/>"#,
        ActionKind::Profile => r#"<circle cx="32" cy="24" r="10"/><path d="M14 50c2-12 12-16 18-16s16 4 18 16"/>"#,
        ActionKind::Screenshot => r#"<rect x="12" y="20" width="40" height="28" rx="4"/><circle cx="32" cy="34" r="6"/>"#,
        ActionKind::Hotkey => r#"<rect x="10" y="22" width="44" height="22" rx="4"/><path d="M20 28v8M32 28v8M44 28v8"/>"#,
        ActionKind::RefreshBrowser | ActionKind::RefreshCapture => {
            r#"<path d="M18 24a16 16 0 0 1 26-4M46 40a16 16 0 0 1-26 4M44 14v10H34M20 50V40h10"/>"#
        }
        ActionKind::Chapter => r#"<path d="M16 14h24l8 8v28H16zM40 14v8h8M24 32h16M24 40h10"/>"#,
        ActionKind::Media => r#"<path d="M22 16v32l24-16z"/>"#,
        ActionKind::Stats => r#"<path d="M12 46V30M28 46V18M44 46V26"/>"#,
        ActionKind::Volume => r#"<path d="M12 26h10l12-10v32L22 38H12zM42 24a12 12 0 0 1 0 16"/>"#,
        ActionKind::Raw | ActionKind::RawBatch => r#"<path d="M18 20l-8 12 8 12M46 20l8 12-8 12M36 16l-8 32"/>"#,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_one_segment_per_instance() {
        let image = key_image(
            ActionKind::Stream,
            &[
                Segment {
                    label: "Stream PC".into(),
                    color: "#4c8dff".into(),
                    state: SegmentState::Active,
                },
                Segment {
                    label: "Record".into(),
                    color: "#ef5b5b".into(),
                    state: SegmentState::Inactive,
                },
                Segment {
                    label: "Offline".into(),
                    color: "#888".into(),
                    state: SegmentState::Unavailable,
                },
            ],
            "#f4f7fb",
        );
        let svg = String::from_utf8(STANDARD.decode(image.trim_start_matches("data:image/svg+xml;base64,")).unwrap()).unwrap();
        assert_eq!(svg.matches("<rect x=").count(), 4);
        assert!(svg.contains("url(#hatch)"));
        assert!(svg.contains(">S<"));
        assert!(svg.contains(">R<"));
    }
}
