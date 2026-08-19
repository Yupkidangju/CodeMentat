use egui::{Color32, RichText, ScrollArea, Ui};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

const MAX_MARKDOWN_BYTES: usize = 1024 * 1024;
const MAX_BLOCKS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    Paragraph(String),
    Heading { level: u8, text: String },
    Code { language: String, text: String },
    Rule,
}

pub fn parse_markdown_blocks(markdown: &str) -> Vec<MarkdownBlock> {
    let bounded = bounded_utf8(markdown, MAX_MARKDOWN_BYTES);
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut code: Option<(String, String)> = None;
    let mut heading: Option<u8> = None;
    let mut link_destination: Option<String> = None;
    let mut depth = 0usize;
    let mut event_count = 0usize;

    for event in Parser::new(bounded) {
        event_count += 1;
        if event_count > MAX_BLOCKS {
            break;
        }
        match &event {
            Event::Start(_) => {
                depth += 1;
                if depth > 32 {
                    blocks.push(MarkdownBlock::Paragraph(
                        "[Markdown nesting 32단계 상한으로 이후 내용이 생략되었습니다.]"
                            .to_string(),
                    ));
                    break;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_paragraph(&mut blocks, &mut text);
                heading = Some(heading_level(level));
            }
            Event::End(TagEnd::Heading(_)) => {
                let level = heading.take().unwrap_or(2);
                blocks.push(MarkdownBlock::Heading {
                    level,
                    text: std::mem::take(&mut text),
                });
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_paragraph(&mut blocks, &mut text);
                let language = match kind {
                    CodeBlockKind::Indented => String::new(),
                    CodeBlockKind::Fenced(language) => language.into_string(),
                };
                code = Some((language, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((language, text)) = code.take() {
                    blocks.push(MarkdownBlock::Code { language, text });
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_destination = Some(dest_url.into_string());
            }
            Event::End(TagEnd::Link) => {
                if let Some(destination) = link_destination.take() {
                    if is_safe_http_link(&destination) {
                        text.push_str(" (");
                        text.push_str(&destination);
                        text.push(')');
                    }
                }
            }
            Event::Start(Tag::Image { .. }) => text.push_str("[이미지 자동 로드 차단: "),
            Event::End(TagEnd::Image) => text.push(']'),
            Event::Text(value) | Event::Html(value) | Event::InlineHtml(value) => {
                if let Some((_, code_text)) = code.as_mut() {
                    code_text.push_str(&value);
                } else {
                    text.push_str(&value);
                }
            }
            Event::Code(value) => {
                text.push('`');
                text.push_str(&value);
                text.push('`');
            }
            Event::SoftBreak => text.push(' '),
            Event::HardBreak => text.push('\n'),
            Event::Rule => {
                flush_paragraph(&mut blocks, &mut text);
                blocks.push(MarkdownBlock::Rule);
            }
            Event::TaskListMarker(checked) => {
                text.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::End(TagEnd::Paragraph | TagEnd::Item) => {
                flush_paragraph(&mut blocks, &mut text);
            }
            _ => {}
        }
    }
    flush_paragraph(&mut blocks, &mut text);
    if markdown.len() > MAX_MARKDOWN_BYTES {
        blocks.push(MarkdownBlock::Paragraph(
            "[Markdown 1MiB 상한으로 이후 내용이 생략되었습니다.]".to_string(),
        ));
    }
    blocks
}

pub fn render_markdown(ui: &mut Ui, markdown: &str) {
    for block in parse_markdown_blocks(markdown) {
        match block {
            MarkdownBlock::Paragraph(text) => {
                ui.add(egui::Label::new(text).wrap());
            }
            MarkdownBlock::Heading { level, text } => {
                let size = match level {
                    1 => 19.0,
                    2 => 17.0,
                    _ => 15.0,
                };
                ui.label(RichText::new(text).size(size).strong());
            }
            MarkdownBlock::Code { language, text } => {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if language.is_empty() {
                                "code"
                            } else {
                                &language
                            })
                            .small()
                            .color(Color32::DARK_GRAY),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("복사").clicked() {
                                ui.ctx().copy_text(text.clone());
                            }
                        });
                    });
                    ScrollArea::horizontal()
                        .id_salt(("markdown_code", text.len()))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(text).monospace())
                                    .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        });
                });
            }
            MarkdownBlock::Rule => {
                ui.separator();
            }
        }
        ui.add_space(4.0);
    }
}

fn flush_paragraph(blocks: &mut Vec<MarkdownBlock>, text: &mut String) {
    if !text.is_empty() {
        blocks.push(MarkdownBlock::Paragraph(std::mem::take(text)));
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn bounded_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn is_safe_http_link(destination: &str) -> bool {
    destination.starts_with("https://") || destination.starts_with("http://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenced_code_is_a_dedicated_non_wrapping_block() {
        let blocks = parse_markdown_blocks("## 제목\n\n```rust\nfn main() {}\n```");

        assert!(matches!(
            &blocks[0],
            MarkdownBlock::Heading { level: 2, text } if text == "제목"
        ));
        assert!(matches!(
            &blocks[1],
            MarkdownBlock::Code { language, text }
                if language == "rust" && text == "fn main() {}\n"
        ));
    }

    #[test]
    fn image_and_unsafe_link_never_become_fetch_instructions() {
        let blocks = parse_markdown_blocks("![alt](file:///secret) [x](javascript:alert(1))");
        let text = format!("{blocks:?}");

        assert!(text.contains("이미지 자동 로드 차단"));
        assert!(!text.contains("file:///secret"));
        assert!(!text.contains("javascript:alert"));
    }
}
