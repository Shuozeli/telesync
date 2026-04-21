use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};

use crate::error::TelesyncError;
use crate::types::{Node, NodeAttrs, NodeElement};

/// Parsed YAML frontmatter from a markdown file.
pub struct Frontmatter {
    pub title: Option<String>,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
}

/// A validation issue for an unsupported markdown feature.
pub struct ValidationIssue {
    pub feature: String,
    pub line: usize,
}

/// Extract YAML frontmatter from markdown input.
/// Returns the parsed frontmatter and a reference to the remaining body.
pub fn extract_frontmatter(input: &str) -> (Frontmatter, &str) {
    let trimmed = input.trim_start();
    if !trimmed.starts_with("---") {
        return (
            Frontmatter {
                title: None,
                author_name: None,
                author_url: None,
            },
            input,
        );
    }

    // Find the opening delimiter
    let after_opening = &trimmed[3..];
    let after_opening = after_opening.strip_prefix('\n').unwrap_or(after_opening);

    // Find the closing delimiter
    let closing_pos = after_opening.find("\n---");
    let (yaml_block, body) = match closing_pos {
        Some(pos) => {
            let yaml = &after_opening[..pos];
            let rest = &after_opening[pos + 4..]; // skip "\n---"
            let rest = rest.strip_prefix('\n').unwrap_or(rest);
            (yaml, rest)
        }
        None => {
            // No closing delimiter -- treat entire input as body (no frontmatter)
            return (
                Frontmatter {
                    title: None,
                    author_name: None,
                    author_url: None,
                },
                input,
            );
        }
    };

    let mut title = None;
    let mut author_name = None;
    let mut author_url = None;

    for line in yaml_block.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            // Strip surrounding quotes if present
            let value = strip_quotes(value);
            match key {
                "title" => title = Some(value.to_string()),
                "author_name" => author_name = Some(value.to_string()),
                "author_url" => author_url = Some(value.to_string()),
                _ => {}
            }
        }
    }

    // Calculate the byte offset of body within the original input
    let body_offset = body.as_ptr() as usize - input.as_ptr() as usize;
    let body_ref = &input[body_offset..];

    (
        Frontmatter {
            title,
            author_name,
            author_url,
        },
        body_ref,
    )
}

fn strip_quotes(s: &str) -> &str {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Resolve the page title with priority: frontmatter > first heading > filename-derived.
pub fn resolve_title(frontmatter: &Frontmatter, body: &str, filename: &str) -> String {
    if let Some(ref title) = frontmatter.title {
        if !title.is_empty() {
            return title.clone();
        }
    }

    // Try to extract first heading from body
    if let Some(heading) = extract_first_heading(body) {
        return heading;
    }

    // Derive from filename
    title_from_filename(filename)
}

fn extract_first_heading(body: &str) -> Option<String> {
    let parser = Parser::new(body);
    let mut in_heading = false;
    let mut heading_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                heading_text.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if in_heading && !heading_text.is_empty() {
                    return Some(heading_text);
                }
                in_heading = false;
            }
            Event::Text(text) if in_heading => {
                heading_text.push_str(&text);
            }
            Event::Code(code) if in_heading => {
                heading_text.push_str(&code);
            }
            _ => {}
        }
    }
    None
}

fn title_from_filename(filename: &str) -> String {
    // Strip extension
    let name = filename
        .rsplit('/')
        .next()
        .unwrap_or(filename)
        .trim_end_matches(".md")
        .trim_end_matches(".markdown");

    // Strip date prefix (YYYY-MM-DD-)
    let name = if name.len() > 11 && name[..10].chars().all(|c| c.is_ascii_digit() || c == '-') {
        // Verify it looks like a date: NNNN-NN-NN-
        let bytes = name.as_bytes();
        if bytes[4] == b'-' && bytes[7] == b'-' && bytes.get(10) == Some(&b'-') {
            &name[11..]
        } else {
            name
        }
    } else {
        name
    };

    // Replace hyphens with spaces and title case
    name.split('-')
        .filter(|s| !s.is_empty())
        .map(title_case_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn title_case_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = String::with_capacity(word.len());
            for c in first.to_uppercase() {
                result.push(c);
            }
            for c in chars {
                result.push(c);
            }
            result
        }
    }
}

/// Convert markdown body to Telegraph DOM nodes.
/// Returns Err with all validation issues if unsupported features are found.
pub fn markdown_to_nodes(body: &str) -> Result<Vec<Node>, Vec<ValidationIssue>> {
    let parser = Parser::new(body);
    let mut issues: Vec<ValidationIssue> = Vec::new();
    let mut converter = NodeConverter::new();

    // Track line numbers via byte offsets
    let line_starts = compute_line_starts(body);

    for (event, range) in parser.into_offset_iter() {
        let line = offset_to_line(&line_starts, range.start);

        match event {
            // Unsupported features
            Event::Start(Tag::Table(_)) => {
                issues.push(ValidationIssue {
                    feature: "table".to_string(),
                    line,
                });
            }
            Event::Start(Tag::Image { .. }) => {
                issues.push(ValidationIssue {
                    feature: "image".to_string(),
                    line,
                });
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                issues.push(ValidationIssue {
                    feature: "raw HTML".to_string(),
                    line,
                });
            }
            Event::FootnoteReference(_) => {
                issues.push(ValidationIssue {
                    feature: "footnote".to_string(),
                    line,
                });
            }
            Event::TaskListMarker(_) => {
                issues.push(ValidationIssue {
                    feature: "task list".to_string(),
                    line,
                });
            }

            // Skip events from unsupported elements (table internals)
            Event::Start(Tag::TableHead | Tag::TableRow | Tag::TableCell) => {}
            Event::End(TagEnd::Table | TagEnd::TableHead | TagEnd::TableRow | TagEnd::TableCell) => {
            }
            Event::End(TagEnd::Image) => {}

            // Supported elements
            Event::Start(Tag::Heading { level, .. }) => {
                let tag = heading_level_to_tag(level);
                converter.push_element(tag);
            }
            Event::End(TagEnd::Heading(_)) => {
                converter.pop_element();
            }

            Event::Start(Tag::Paragraph) => {
                converter.push_element("p");
            }
            Event::End(TagEnd::Paragraph) => {
                converter.pop_element_skip_empty();
            }

            Event::Start(Tag::BlockQuote(_)) => {
                converter.push_element("blockquote");
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                converter.pop_element();
            }

            Event::Start(Tag::CodeBlock(_)) => {
                converter.push_element("pre");
                converter.push_element("code");
            }
            Event::End(TagEnd::CodeBlock) => {
                converter.pop_element(); // code
                converter.pop_element(); // pre
            }

            Event::Start(Tag::List(None)) => {
                converter.push_element("ul");
            }
            Event::Start(Tag::List(Some(_))) => {
                converter.push_element("ol");
            }
            Event::End(TagEnd::List(_)) => {
                converter.pop_element();
            }

            Event::Start(Tag::Item) => {
                converter.push_element("li");
            }
            Event::End(TagEnd::Item) => {
                converter.pop_element();
            }

            Event::Start(Tag::Emphasis) => {
                converter.push_element("em");
            }
            Event::End(TagEnd::Emphasis) => {
                converter.pop_element();
            }

            Event::Start(Tag::Strong) => {
                converter.push_element("strong");
            }
            Event::End(TagEnd::Strong) => {
                converter.pop_element();
            }

            Event::Start(Tag::Strikethrough) => {
                converter.push_element("s");
            }
            Event::End(TagEnd::Strikethrough) => {
                converter.pop_element();
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                converter.push_element_with_attrs(
                    "a",
                    Some(NodeAttrs {
                        href: Some(dest_url.to_string()),
                        src: None,
                    }),
                );
            }
            Event::End(TagEnd::Link) => {
                converter.pop_element();
            }

            Event::Text(text) => {
                converter.append_text(&text);
            }

            Event::Code(code) => {
                converter.push_element("code");
                converter.append_text(&code);
                converter.pop_element();
            }

            Event::SoftBreak => {
                converter.append_text("\n");
            }

            Event::HardBreak => {
                converter.append_node(Node::Element(NodeElement {
                    tag: "br".to_string(),
                    attrs: None,
                    children: None,
                }));
            }

            Event::Rule => {
                converter.append_node(Node::Element(NodeElement {
                    tag: "hr".to_string(),
                    attrs: None,
                    children: None,
                }));
            }

            // Ignore metadata events
            Event::Start(Tag::MetadataBlock(_)) | Event::End(TagEnd::MetadataBlock(_)) => {}

            // Catch-all for any other events
            _ => {}
        }
    }

    if !issues.is_empty() {
        return Err(issues);
    }

    Ok(converter.finish())
}

fn heading_level_to_tag(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 | HeadingLevel::H2 => "h3",
        HeadingLevel::H3 => "h4",
        HeadingLevel::H4 | HeadingLevel::H5 | HeadingLevel::H6 => "h4",
    }
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn offset_to_line(line_starts: &[usize], offset: usize) -> usize {
    match line_starts.binary_search(&offset) {
        Ok(idx) => idx + 1,
        Err(idx) => idx,
    }
}

/// Stack-based node builder for converting pulldown-cmark events to Telegraph nodes.
struct NodeConverter {
    /// Stack of (tag, attrs, children) for open elements.
    stack: Vec<BuildingElement>,
    /// Top-level nodes (result).
    root: Vec<Node>,
}

struct BuildingElement {
    tag: String,
    attrs: Option<NodeAttrs>,
    children: Vec<Node>,
}

impl NodeConverter {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            root: Vec::new(),
        }
    }

    fn push_element(&mut self, tag: &str) {
        self.stack.push(BuildingElement {
            tag: tag.to_string(),
            attrs: None,
            children: Vec::new(),
        });
    }

    fn push_element_with_attrs(&mut self, tag: &str, attrs: Option<NodeAttrs>) {
        self.stack.push(BuildingElement {
            tag: tag.to_string(),
            attrs,
            children: Vec::new(),
        });
    }

    fn pop_element(&mut self) {
        if let Some(elem) = self.stack.pop() {
            let node = Node::Element(NodeElement {
                tag: elem.tag,
                attrs: elem.attrs,
                children: if elem.children.is_empty() {
                    None
                } else {
                    Some(elem.children)
                },
            });
            self.append_node(node);
        }
    }

    fn pop_element_skip_empty(&mut self) {
        if let Some(elem) = self.stack.pop() {
            // Skip empty paragraphs
            if elem.children.is_empty() {
                return;
            }
            let node = Node::Element(NodeElement {
                tag: elem.tag,
                attrs: elem.attrs,
                children: Some(elem.children),
            });
            self.append_node(node);
        }
    }

    fn append_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let target = match self.stack.last_mut() {
            Some(elem) => &mut elem.children,
            None => &mut self.root,
        };

        // Merge consecutive text nodes
        if let Some(Node::Text(existing)) = target.last_mut() {
            existing.push_str(text);
        } else {
            target.push(Node::Text(text.to_string()));
        }
    }

    fn append_node(&mut self, node: Node) {
        match self.stack.last_mut() {
            Some(elem) => elem.children.push(node),
            None => self.root.push(node),
        }
    }

    fn finish(self) -> Vec<Node> {
        self.root
    }
}

/// Check content for patterns that should never appear on public pages.
pub fn check_content_safety(content: &str, filename: &str) -> Result<(), TelesyncError> {
    for (line_num, line) in content.lines().enumerate() {
        let line_number = line_num + 1;

        // Check for API key / secret prefixes
        for prefix in &["sk-", "AIza", "ghp_", "GOCSPX-", "sk-or-v1-"] {
            if line.contains(prefix) {
                return Err(TelesyncError::ContentSafety {
                    file: filename.to_string(),
                    reason: format!("potential secret (starts with '{prefix}')"),
                    line: line_number,
                });
            }
        }

        // Check for sensitive keywords (case-insensitive)
        let lower = line.to_lowercase();
        for keyword in &["password", "secret", "access_token", "private_key"] {
            if lower.contains(keyword) {
                return Err(TelesyncError::ContentSafety {
                    file: filename.to_string(),
                    reason: format!("sensitive keyword '{keyword}'"),
                    line: line_number,
                });
            }
        }

        // Check for private IP ranges
        if let Some(reason) = check_private_ip(line) {
            return Err(TelesyncError::ContentSafety {
                file: filename.to_string(),
                reason,
                line: line_number,
            });
        }
    }

    Ok(())
}

fn check_private_ip(line: &str) -> Option<String> {
    // Simple scan for private IP patterns
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for digit sequences that could be IPs
        if bytes[i].is_ascii_digit() {
            if let Some(ip_end) = try_parse_ip_at(line, i) {
                let candidate = &line[i..ip_end];
                if is_private_ip(candidate) {
                    return Some(format!("private IP address '{candidate}'"));
                }
                i = ip_end;
                continue;
            }
        }
        i += 1;
    }
    None
}

fn try_parse_ip_at(line: &str, start: usize) -> Option<usize> {
    // Try to parse an IPv4 address starting at `start`
    let rest = &line[start..];
    let mut octets = 0;
    let mut pos = 0;

    for segment in rest.split('.') {
        if octets >= 4 {
            break;
        }
        // Each octet must be 1-3 digits
        let digits: &str = segment
            .get(..segment.len().min(3))
            .unwrap_or(segment);
        let digit_count = digits.chars().take_while(|c| c.is_ascii_digit()).count();
        if digit_count == 0 {
            break;
        }
        let octet_str = &segment[..digit_count];
        if octet_str.parse::<u16>().unwrap_or(256) > 255 {
            break;
        }
        octets += 1;
        pos += digit_count;
        if octets < 4 {
            pos += 1; // for the dot
        }
        // Only consume the digit portion of this segment
        if digit_count < segment.len() {
            break;
        }
    }

    if octets == 4 {
        Some(start + pos)
    } else {
        None
    }
}

fn is_private_ip(ip: &str) -> bool {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let octets: Vec<u8> = match parts.iter().map(|p| p.parse::<u8>()).collect() {
        Ok(v) => v,
        Err(_) => return false,
    };

    // 10.0.0.0/8
    if octets[0] == 10 {
        return true;
    }
    // 192.168.0.0/16
    if octets[0] == 192 && octets[1] == 168 {
        return true;
    }
    // 172.16.0.0/12 (172.16-31.x.x)
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return true;
    }

    false
}

/// Compute a content hash from the nodes, title, and author name.
/// Returns `"sha256:{hex_digest}"`.
pub fn content_hash(nodes: &[Node], title: &str, author_name: &str) -> String {
    let canonical_json = serde_json::to_string(nodes).unwrap_or_default();
    let input = format!("{canonical_json}\n{title}\n{author_name}");
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    format!("sha256:{:x}", digest)
}
