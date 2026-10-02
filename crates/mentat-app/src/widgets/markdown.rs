use egui::{Color32, RichText, ScrollArea, Ui};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

const MAX_MARKDOWN_BYTES: usize = 1024 * 1024;
const MAX_BLOCKS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    ParagraphEnd,
    InlineCode(String),
    Styled {
        text: String,
        strong: bool,
        emphasis: bool,
    },
    Link {
        label: String,
        destination: String,
    },
    Paragraph(String),
    Heading {
        level: u8,
        text: String,
    },
    Code {
        language: String,
        text: String,
    },
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
    let mut strong = false;
    let mut emphasis = false;
    let mut lists: Vec<Option<u64>> = Vec::new();

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
            Event::Start(Tag::List(start)) => {
                flush_paragraph(&mut blocks, &mut text);
                lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                flush_paragraph(&mut blocks, &mut text);
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                flush_paragraph(&mut blocks, &mut text);
                text.push_str(&"  ".repeat(lists.len().saturating_sub(1)));
                if let Some(Some(number)) = lists.last_mut() {
                    text.push_str(&format!("{number}. "));
                    *number += 1;
                } else {
                    text.push_str("• ");
                }
            }
            Event::Start(Tag::Strong) => {
                flush_paragraph(&mut blocks, &mut text);
                strong = true;
            }
            Event::Start(Tag::Emphasis) => {
                flush_paragraph(&mut blocks, &mut text);
                emphasis = true;
            }
            Event::End(TagEnd::Strong) => {
                strong = false;
            }
            Event::End(TagEnd::Emphasis) => {
                emphasis = false;
            }
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
                flush_paragraph(&mut blocks, &mut text);
                link_destination = Some(dest_url.into_string());
            }
            Event::End(TagEnd::Link) => {
                if let Some(destination) = link_destination.take() {
                    if is_safe_http_link(&destination) {
                        blocks.push(MarkdownBlock::Link {
                            label: std::mem::take(&mut text),
                            destination,
                        });
                    }
                }
            }
            Event::Start(Tag::Image { .. }) => text.push_str("[이미지 자동 로드 차단: "),
            Event::End(TagEnd::Image) => text.push(']'),
            Event::Html(value) | Event::InlineHtml(value) => {
                text.push_str(readable_html(&value).trim_matches('\n'));
            }
            Event::Text(value) => {
                if let Some((_, code_text)) = code.as_mut() {
                    code_text.push_str(&value);
                } else if (strong || emphasis) && heading.is_none() && link_destination.is_none() {
                    blocks.push(MarkdownBlock::Styled {
                        text: value.into_string(),
                        strong,
                        emphasis,
                    });
                } else {
                    text.push_str(&value);
                }
            }
            Event::Code(value) => {
                if heading.is_some() {
                    text.push_str(&value);
                } else {
                    flush_paragraph(&mut blocks, &mut text);
                    blocks.push(MarkdownBlock::InlineCode(value.into_string()));
                }
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
                blocks.push(MarkdownBlock::ParagraphEnd);
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
    let mut inline = Vec::new();
    for block in parse_markdown_blocks(markdown) {
        if matches!(
            block,
            MarkdownBlock::Paragraph(_)
                | MarkdownBlock::Styled { .. }
                | MarkdownBlock::Link { .. }
                | MarkdownBlock::InlineCode(_)
        ) {
            inline.push(block);
            continue;
        }
        render_inline(ui, &mut inline);
        match block {
            MarkdownBlock::ParagraphEnd => continue,
            MarkdownBlock::InlineCode(_) => unreachable!(),
            MarkdownBlock::Styled {
                text,
                strong,
                emphasis,
            } => {
                let mut text = RichText::new(text);
                if strong {
                    text = text.strong();
                }
                if emphasis {
                    text = text.italics();
                }
                ui.label(text);
            }
            MarkdownBlock::Link { label, destination } => {
                ui.hyperlink_to(label, destination);
            }
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
    render_inline(ui, &mut inline);
}

fn render_inline(ui: &mut Ui, blocks: &mut Vec<MarkdownBlock>) {
    if blocks.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for block in blocks.drain(..) {
            let (text, strong, emphasis, code) = match block {
                MarkdownBlock::Paragraph(text) => (text, false, false, false),
                MarkdownBlock::Styled {
                    text,
                    strong,
                    emphasis,
                } => (text, strong, emphasis, false),
                MarkdownBlock::InlineCode(text) => (text, false, false, true),
                MarkdownBlock::Link { label, destination } => {
                    ui.hyperlink_to(label, destination);
                    continue;
                }
                _ => unreachable!(),
            };
            for word in text.split_inclusive(char::is_whitespace) {
                let mut rich = RichText::new(word);
                if strong {
                    rich = rich.strong();
                }
                if emphasis {
                    rich = rich.italics();
                }
                if code {
                    rich = rich
                        .monospace()
                        .background_color(ui.visuals().code_bg_color);
                }
                ui.add(egui::Label::new(rich).wrap());
            }
        }
    });
    ui.add_space(4.0);
}

fn flush_paragraph(blocks: &mut Vec<MarkdownBlock>, text: &mut String) {
    if !text.is_empty() {
        blocks.push(MarkdownBlock::Paragraph(std::mem::take(text)));
    }
}

fn readable_html(mut html: &str) -> String {
    let mut output = String::new();
    while let Some(start) = html.find('<') {
        output.push_str(&html[..start]);
        let Some(end) = html[start..].find('>').map(|end| start + end) else {
            output.push_str(&html[start..]);
            return output;
        };
        let tag = &html[start..=end];
        let name = tag[1..tag.len() - 1]
            .trim()
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "details" | "summary" | "strong" | "em" | "b" | "i"
        ) {
            if matches!(name.as_str(), "details" | "summary") {
                output.push('\n');
            }
        } else {
            output.push_str(tag);
        }
        html = &html[end + 1..];
    }
    output.push_str(html);
    output
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
    fn inline_emphasis_does_not_turn_one_sentence_into_multiple_rows() {
        let ctx = egui::Context::default();
        let mut height = 0.0;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(560.0, 300.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    height = ui
                        .scope(|ui| render_markdown(ui, "Hello **bold** and *italic* world."))
                        .response
                        .rect
                        .height();
                });
            },
        );
        assert!(height < 40.0, "one sentence occupied {height}pt");
    }

    #[test]
    fn fenced_code_is_a_dedicated_non_wrapping_block() {
        let blocks = parse_markdown_blocks("## 제목\n\n```rust\nfn main() {}\n```");

        assert!(matches!(
            &blocks[0],
            MarkdownBlock::Heading { level: 2, text } if text == "제목"
        ));
        assert!(matches!(
            blocks.iter().find(|block| matches!(block, MarkdownBlock::Code { .. })).unwrap(),
            MarkdownBlock::Code { language, text }
                if language == "rust" && text == "fn main() {}\n"
        ));
    }

    #[test]
    fn lists_emphasis_and_explicit_http_links_survive_parsing() {
        let blocks = parse_markdown_blocks(
            "1. first\n2. second\n\n**strong** and *emphasis* [link](https://example.com)",
        );
        assert!(blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::Paragraph(t) if t.contains("1. first"))));
        assert!(blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::Styled { strong: true, .. })));
        assert!(blocks.iter().any(|b| matches!(b, MarkdownBlock::Link { destination, .. } if destination == "https://example.com")));
    }

    #[test]
    fn image_and_unsafe_link_never_become_fetch_instructions() {
        let blocks = parse_markdown_blocks("![alt](file:///secret) [x](javascript:alert(1))");
        let text = format!("{blocks:?}");

        assert!(text.contains("이미지 자동 로드 차단"));
        assert!(!text.contains("file:///secret"));
        assert!(!text.contains("javascript:alert"));
    }

    #[test]
    fn harmless_html_layout_tags_show_reference_text_without_raw_tags() {
        let blocks = parse_markdown_blocks("<details>\n<summary><strong>참고 코드</strong></summary>\n\n```rust\nfn main() {}\n```\n</details>");
        let display = format!("{blocks:?}");
        assert!(display.contains("참고 코드"));
        assert!(!display.contains("<details>"));
        assert!(!display.contains("<summary>"));
        assert!(blocks
            .iter()
            .any(|block| matches!(block, MarkdownBlock::Code { .. })));
    }
}
