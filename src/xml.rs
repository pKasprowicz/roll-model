//! Minimal, dependency-free XML DOM parser.
//!
//! MusicXML is plain, namespace-free XML, so a small parser that handles
//! elements, attributes, text, CDATA, comments, processing instructions,
//! a DOCTYPE and the standard/numeric entities is all we need.

#[derive(Debug, Clone)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Child elements in document order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements().filter(move |e| e.name == name)
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    pub fn has_child(&self, name: &str) -> bool {
        self.child(name).is_some()
    }

    /// Concatenated text of all descendants, trimmed.
    pub fn text(&self) -> String {
        let mut out = String::new();
        collect_text(self, &mut out);
        out.trim().to_string()
    }

    pub fn child_text(&self, name: &str) -> Option<String> {
        self.child(name).map(|c| c.text())
    }

    /// Follow a path of child names, e.g. `["unpitched", "display-step"]`.
    pub fn path(&self, names: &[&str]) -> Option<&Element> {
        let mut cur = self;
        for n in names {
            cur = cur.child(n)?;
        }
        Some(cur)
    }

    /// Depth-first search for the first descendant with this name.
    pub fn find(&self, name: &str) -> Option<&Element> {
        for e in self.elements() {
            if e.name == name {
                return Some(e);
            }
            if let Some(f) = e.find(name) {
                return Some(f);
            }
        }
        None
    }
}

fn collect_text(e: &Element, out: &mut String) {
    for c in &e.children {
        match c {
            Node::Text(t) => out.push_str(t),
            Node::Element(el) => collect_text(el, out),
        }
    }
}

pub fn parse(input: &str) -> Result<Element, String> {
    let s = input.strip_prefix('\u{feff}').unwrap_or(input);
    let b = s.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    while i < b.len() {
        if b[i] == b'<' {
            if s[i..].starts_with("<?") {
                i = find_from(s, i + 2, "?>").ok_or("unterminated processing instruction")? + 2;
            } else if s[i..].starts_with("<!--") {
                i = find_from(s, i + 4, "-->").ok_or("unterminated comment")? + 3;
            } else if s[i..].starts_with("<![CDATA[") {
                let end = find_from(s, i + 9, "]]>").ok_or("unterminated CDATA")?;
                if let Some(top) = stack.last_mut() {
                    top.children.push(Node::Text(s[i + 9..end].to_string()));
                }
                i = end + 3;
            } else if s[i..].starts_with("<!") {
                i = skip_declaration(b, i)?;
            } else if s[i..].starts_with("</") {
                let end = find_from(s, i, ">").ok_or("unterminated end tag")?;
                let name = s[i + 2..end].trim();
                let el = stack.pop().ok_or_else(|| format!("unexpected </{name}>"))?;
                if el.name != name {
                    return Err(format!("mismatched tag: <{}> closed by </{}>", el.name, name));
                }
                attach(&mut stack, &mut root, el);
                i = end + 1;
            } else {
                let (el, self_closing, next) = parse_start_tag(s, i)?;
                i = next;
                if self_closing {
                    attach(&mut stack, &mut root, el);
                } else {
                    stack.push(el);
                }
            }
        } else {
            let end = s[i..].find('<').map(|p| p + i).unwrap_or(b.len());
            if let Some(top) = stack.last_mut() {
                let raw = &s[i..end];
                if !raw.trim().is_empty() {
                    top.children.push(Node::Text(decode_entities(raw)));
                }
            }
            i = end;
        }
    }
    if let Some(open) = stack.last() {
        return Err(format!("unclosed element <{}>", open.name));
    }
    root.ok_or_else(|| "document has no root element".to_string())
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, el: Element) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(Node::Element(el)),
        None => {
            if root.is_none() {
                *root = Some(el);
            }
        }
    }
}

fn find_from(s: &str, from: usize, pat: &str) -> Option<usize> {
    s.get(from..)?.find(pat).map(|p| p + from)
}

/// Skips `<!DOCTYPE ...>` including an optional `[...]` internal subset.
fn skip_declaration(b: &[u8], start: usize) -> Result<usize, String> {
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = start + 2;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'[' => depth += 1,
                b']' => depth -= 1,
                b'>' if depth <= 0 => return Ok(i + 1),
                _ => {}
            },
        }
        i += 1;
    }
    Err("unterminated <! declaration".into())
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':') || c >= 0x80
}

fn parse_start_tag(s: &str, start: usize) -> Result<(Element, bool, usize), String> {
    let b = s.as_bytes();
    let mut i = start + 1;
    let name_start = i;
    while i < b.len() && is_name_char(b[i]) {
        i += 1;
    }
    if i == name_start {
        return Err(format!("invalid tag at byte {start}"));
    }
    let mut el = Element {
        name: s[name_start..i].to_string(),
        ..Default::default()
    };
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return Err(format!("unterminated tag <{}>", el.name));
        }
        if b[i] == b'>' {
            return Ok((el, false, i + 1));
        }
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'>' {
            return Ok((el, true, i + 2));
        }
        let an_start = i;
        while i < b.len() && is_name_char(b[i]) {
            i += 1;
        }
        if i == an_start {
            return Err(format!("bad attribute in <{}>", el.name));
        }
        let key = s[an_start..i].to_string();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            // Attribute without value (not valid XML, tolerate it).
            el.attrs.push((key, String::new()));
            continue;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            return Err(format!("unquoted attribute {key} in <{}>", el.name));
        }
        let q = b[i];
        let v_start = i + 1;
        let v_end = s[v_start..]
            .find(q as char)
            .map(|p| p + v_start)
            .ok_or("unterminated attribute value")?;
        el.attrs.push((key, decode_entities(&s[v_start..v_end])));
        i = v_end + 1;
    }
}

pub fn decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        if let Some(semi) = after.find(';').filter(|&n| n <= 10) {
            let ent = &after[..semi];
            let decoded = match ent {
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                    u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
                }
                _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            match decoded {
                Some(c) => {
                    out.push(c);
                    rest = &after[semi + 1..];
                }
                None => {
                    out.push('&');
                    rest = after;
                }
            }
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_document() {
        let doc = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">
<!-- comment -->
<root a="1" b='two &amp; three'>
  <child>Hello &lt;world&gt; &#65;&#x42;</child>
  <empty/>
  <x><![CDATA[<raw>]]></x>
</root>"#;
        let r = parse(doc).unwrap();
        assert_eq!(r.name, "root");
        assert_eq!(r.attr("a"), Some("1"));
        assert_eq!(r.attr("b"), Some("two & three"));
        assert_eq!(r.child_text("child").unwrap(), "Hello <world> AB");
        assert!(r.has_child("empty"));
        assert_eq!(r.child_text("x").unwrap(), "<raw>");
    }

    #[test]
    fn rejects_mismatched_tags() {
        assert!(parse("<a><b></a></b>").is_err());
        assert!(parse("<a>").is_err());
    }
}
