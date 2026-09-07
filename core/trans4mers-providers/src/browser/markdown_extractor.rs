pub const DEFAULT_MAX_CHARS: usize = 16_000;

pub struct MarkdownExtractor;

impl MarkdownExtractor {
    /// JS script that converts the current DOM tree into clean, human-readable Markdown.
    pub fn build_markdown_extract_script() -> &'static str {
        r#"(() => {
            const root = document.body || document.documentElement;
            if (!root) return '';

            const clone = root.cloneNode(true);
            const toRemove = clone.querySelectorAll('script, style, svg, noscript, iframe, link, template');
            toRemove.forEach(el => el.remove());

            function nodeToMd(node) {
                if (node.nodeType === 3) { // Text node
                    return node.textContent.replace(/\s+/g, ' ');
                }
                if (node.nodeType !== 1) return '';

                const tag = node.tagName.toLowerCase();
                const children = Array.from(node.childNodes).map(nodeToMd).join('');
                const cleanChildren = children.trim();

                switch(tag) {
                    case 'h1': return '\n# ' + cleanChildren + '\n\n';
                    case 'h2': return '\n## ' + cleanChildren + '\n\n';
                    case 'h3': return '\n### ' + cleanChildren + '\n\n';
                    case 'h4': return '\n#### ' + cleanChildren + '\n\n';
                    case 'h5': return '\n##### ' + cleanChildren + '\n\n';
                    case 'h6': return '\n###### ' + cleanChildren + '\n\n';
                    case 'p': return '\n' + cleanChildren + '\n\n';
                    case 'li': return '* ' + cleanChildren + '\n';
                    case 'ul':
                    case 'ol': return '\n' + children + '\n';
                    case 'blockquote': return '\n> ' + cleanChildren + '\n\n';
                    case 'pre': return '\n```\n' + node.innerText + '\n```\n\n';
                    case 'code': return ' `' + cleanChildren + '` ';
                    case 'a': {
                        const href = node.getAttribute('href');
                        if (href && cleanChildren) {
                            return `[${cleanChildren}](${href})`;
                        }
                        return cleanChildren;
                    }
                    case 'button': return ` [Button: ${cleanChildren}] `;
                    case 'input': {
                        const type = node.getAttribute('type') || 'text';
                        const val = node.value || node.getAttribute('placeholder') || '';
                        return ` [Input (${type}): ${val}] `;
                    }
                    case 'br': return '\n';
                    case 'hr': return '\n---\n\n';
                    default: return children;
                }
            }

            const raw = nodeToMd(clone);
            return raw.replace(/\n{3,}/g, '\n\n').trim();
        })()"#
    }

    /// JS script for structured page extraction returning JSON with provenance.
    pub fn build_structured_extract_script(schema_hint: &str) -> String {
        let escaped_hint =
            serde_json::to_string(schema_hint).unwrap_or_else(|_| "\"\"".to_string());
        format!(
            r#"(() => {{
                const schemaHint = {escaped_hint};
                const headings = Array.from(document.querySelectorAll('h1, h2, h3, h4'))
                    .slice(0, 30)
                    .map(h => ({{ level: h.tagName.toLowerCase(), text: (h.innerText || '').trim() }}))
                    .filter(h => h.text.length > 0);

                const links = Array.from(document.querySelectorAll('a[href]'))
                    .slice(0, 50)
                    .map(a => ({{ text: (a.innerText || '').trim(), href: a.href }}))
                    .filter(l => l.text.length > 0 && !l.href.startsWith('javascript:'));

                const metaTags = {{}};
                document.querySelectorAll('meta[name], meta[property]').forEach(m => {{
                    const key = m.getAttribute('name') || m.getAttribute('property');
                    const val = m.getAttribute('content');
                    if (key && val) metaTags[key] = val;
                }});

                return JSON.stringify({{
                    url: window.location.href,
                    title: document.title,
                    timestamp: new Date().toISOString(),
                    schema_hint: schemaHint,
                    meta: metaTags,
                    headings: headings,
                    links: links
                }});
            }})()"#
        )
    }

    /// Enforces length cap with visible truncation marker.
    pub fn cap_output(content: &str, max_chars: usize) -> String {
        let char_count = content.chars().count();
        if char_count <= max_chars {
            content.to_string()
        } else {
            let truncated: String = content.chars().take(max_chars).collect();
            let remaining = char_count - max_chars;
            format!(
                "{}\n\n[TRUNCATED: remaining {} characters omitted for safety]",
                truncated, remaining
            )
        }
    }
}
