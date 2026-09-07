use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomActionResult {
    pub success: bool,
    pub message: String,
    pub tag: Option<String>,
}

pub struct DomEval;

impl DomEval {
    /// Generates JS script to resolve and click an element with visibility and enabled checks.
    pub fn build_click_script(selector: &str) -> String {
        let escaped_sel =
            serde_json::to_string(selector).unwrap_or_else(|_| format!("\"{}\"", selector));
        format!(
            r#"(() => {{
                const target = {escaped_sel};
                let el = null;
                try {{ el = document.querySelector(target); }} catch(e) {{}}
                if (!el) {{
                    const candidates = Array.from(document.querySelectorAll('button, a, input, select, textarea, [role="button"], [tabindex]'));
                    const lower = target.toLowerCase().trim();
                    el = candidates.find(e => {{
                        const txt = (e.innerText || e.textContent || '').toLowerCase().trim();
                        const aria = (e.getAttribute('aria-label') || '').toLowerCase().trim();
                        const title = (e.getAttribute('title') || '').toLowerCase().trim();
                        const placeholder = (e.getAttribute('placeholder') || '').toLowerCase().trim();
                        return txt === lower || txt.includes(lower) || aria.includes(lower) || title.includes(lower) || placeholder.includes(lower);
                    }});
                }}
                if (!el) {{
                    return JSON.stringify({{ success: false, message: 'Element not found: ' + target, tag: null }});
                }}
                const style = window.getComputedStyle(el);
                const rect = el.getBoundingClientRect();
                const isVisible = style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0' && rect.width > 0 && rect.height > 0;
                if (!isVisible) {{
                    return JSON.stringify({{ success: false, message: 'Element is not visible: ' + target, tag: el.tagName }});
                }}
                if (el.disabled) {{
                    return JSON.stringify({{ success: false, message: 'Element is disabled: ' + target, tag: el.tagName }});
                }}
                try {{
                    el.scrollIntoView({{ block: 'center', inline: 'center' }});
                    el.click();
                    return JSON.stringify({{ success: true, message: 'Clicked ' + el.tagName, tag: el.tagName }});
                }} catch(e) {{
                    return JSON.stringify({{ success: false, message: 'Click dispatch error: ' + e.toString(), tag: el.tagName }});
                }}
            }})()"#
        )
    }

    /// Generates JS script to resolve an input element, focus it, set its value, and dispatch synthetic events.
    pub fn build_type_script(selector: &str, text: &str) -> String {
        let escaped_sel =
            serde_json::to_string(selector).unwrap_or_else(|_| format!("\"{}\"", selector));
        let escaped_text = serde_json::to_string(text).unwrap_or_else(|_| format!("\"{}\"", text));
        format!(
            r#"(() => {{
                const target = {escaped_sel};
                const val = {escaped_text};
                let el = null;
                try {{ el = document.querySelector(target); }} catch(e) {{}}
                if (!el) {{
                    const inputs = Array.from(document.querySelectorAll('input, textarea, [contenteditable="true"]'));
                    const lower = target.toLowerCase().trim();
                    el = inputs.find(e => {{
                        const name = (e.getAttribute('name') || '').toLowerCase().trim();
                        const placeholder = (e.getAttribute('placeholder') || '').toLowerCase().trim();
                        const aria = (e.getAttribute('aria-label') || '').toLowerCase().trim();
                        const id = (e.id || '').toLowerCase().trim();
                        return name.includes(lower) || placeholder.includes(lower) || aria.includes(lower) || id.includes(lower);
                    }});
                }}
                if (!el) {{
                    return JSON.stringify({{ success: false, message: 'Input element not found: ' + target, tag: null }});
                }}
                const style = window.getComputedStyle(el);
                const isVisible = style.display !== 'none' && style.visibility !== 'hidden';
                if (!isVisible) {{
                    return JSON.stringify({{ success: false, message: 'Input element is not visible: ' + target, tag: el.tagName }});
                }}
                if (el.disabled || el.readOnly) {{
                    return JSON.stringify({{ success: false, message: 'Input element is disabled or read-only: ' + target, tag: el.tagName }});
                }}
                try {{
                    el.focus();
                    if (el.isContentEditable) {{
                        el.textContent = val;
                    }} else {{
                        el.value = val;
                    }}
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    return JSON.stringify({{ success: true, message: 'Set input value on ' + el.tagName, tag: el.tagName }});
                }} catch(e) {{
                    return JSON.stringify({{ success: false, message: 'Type dispatch error: ' + e.toString(), tag: el.tagName }});
                }}
            }})()"#
        )
    }

    /// Script to check if an element is present and visible in the DOM.
    pub fn build_check_visibility_script(selector: &str) -> String {
        let escaped_sel =
            serde_json::to_string(selector).unwrap_or_else(|_| format!("\"{}\"", selector));
        format!(
            r#"(() => {{
                const target = {escaped_sel};
                let el = null;
                try {{ el = document.querySelector(target); }} catch(e) {{}}
                if (!el) {{
                    const all = Array.from(document.querySelectorAll('button, a, input, select, textarea, [role="button"]'));
                    const lower = target.toLowerCase().trim();
                    el = all.find(e => {{
                        const txt = (e.innerText || e.textContent || '').toLowerCase().trim();
                        const aria = (e.getAttribute('aria-label') || '').toLowerCase().trim();
                        return txt.includes(lower) || aria.includes(lower);
                    }});
                }}
                if (!el) return false;
                const style = window.getComputedStyle(el);
                const rect = el.getBoundingClientRect();
                return style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0' && rect.width > 0 && rect.height > 0;
            }})()"#
        )
    }
}
