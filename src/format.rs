#[derive(Debug, Clone)]
pub enum Seg {
    Lit(String),
    Tok {
        name: String,
        styles: Vec<String>,
    },
    Group {
        styles: Vec<String>,
        inner: Vec<Seg>,
    },
    LineBreak,
}

pub fn parse(input: &str) -> Vec<Seg> {
    let mut chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let (segs, _) = parse_inner(&mut chars, &mut i, None);
    segs
}

fn read_ident(chars: &[char], i: &mut usize) -> String {
    let mut s = String::new();
    while *i < chars.len() {
        let c = chars[*i];
        if c.is_ascii_alphanumeric() || c == '_' {
            s.push(c);
            *i += 1;
        } else {
            break;
        }
    }
    s
}

fn read_styles(chars: &[char], i: &mut usize) -> Vec<String> {
    let mut out = Vec::new();
    while *i < chars.len() && chars[*i] == ':' {
        *i += 1;
        let st = read_ident(chars, i);
        if st.is_empty() {
            break;
        }
        out.push(st);
    }
    out
}

fn parse_inner(chars: &mut Vec<char>, i: &mut usize, term: Option<char>) -> (Vec<Seg>, bool) {
    let mut out: Vec<Seg> = Vec::new();
    let mut lit = String::new();
    let flush = |lit: &mut String, out: &mut Vec<Seg>| {
        if !lit.is_empty() {
            out.push(Seg::Lit(std::mem::take(lit)));
        }
    };
    while *i < chars.len() {
        let c = chars[*i];
        if Some(c) == term {
            *i += 1;
            flush(&mut lit, &mut out);
            return (out, true);
        }
        match c {
            '%' => {
                let next = chars.get(*i + 1).copied();
                if next == Some('%') {
                    *i += 2;
                    lit.push('%');
                    continue;
                }
                if next == Some('[') {
                    *i += 2;
                    // Read styles: colon-prefixed OR first word-until-space
                    let styles = if chars.get(*i) == Some(&':') {
                        read_styles(chars, i)
                    } else {
                        // First whitespace-delimited token = style chain "a:b:c"
                        let mut raw = String::new();
                        while *i < chars.len() && chars[*i] != ' ' && chars[*i] != ']' {
                            raw.push(chars[*i]);
                            *i += 1;
                        }
                        raw.split(':')
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect()
                    };
                    if chars.get(*i) == Some(&' ') {
                        *i += 1;
                    }
                    flush(&mut lit, &mut out);
                    let (inner, _) = parse_inner(chars, i, Some(']'));
                    out.push(Seg::Group { styles, inner });
                    continue;
                }
                // Token %name[:style...]
                *i += 1;
                let name = read_ident(chars, i);
                if name.is_empty() {
                    lit.push('%');
                    continue;
                }
                let styles = read_styles(chars, i);
                flush(&mut lit, &mut out);
                out.push(Seg::Tok { name, styles });
            }
            '\\' if chars.get(*i + 1) == Some(&'n') => {
                *i += 2;
                flush(&mut lit, &mut out);
                out.push(Seg::LineBreak);
            }
            _ => {
                lit.push(c);
                *i += 1;
            }
        }
    }
    flush(&mut lit, &mut out);
    (out, false)
}

pub fn render_segs<F>(segs: &[Seg], resolve: &F) -> Vec<String>
where
    F: Fn(&str, &[String]) -> Option<String>,
{
    let mut lines: Vec<String> = vec![String::new()];
    let mut last_empty_token = false;
    for seg in segs {
        match seg {
            Seg::Lit(s) => {
                if last_empty_token && s.chars().all(|c| c == ' ') {
                    continue;
                }
                let line = lines.last_mut().unwrap();
                let mut s = s.as_str();
                if last_empty_token && s.starts_with(' ') {
                    s = &s[1..];
                }
                if !s.is_empty() {
                    line.push_str(s);
                    last_empty_token = false;
                }
            }
            Seg::Tok { name, styles } => {
                let line = lines.last_mut().unwrap();
                match resolve(name, styles) {
                    Some(v) if !v.is_empty() => {
                        line.push_str(&v);
                        last_empty_token = false;
                    }
                    _ => {
                        last_empty_token = true;
                    }
                }
            }
            Seg::Group { styles, inner } => {
                let rendered = render_segs(inner, resolve);
                // Groups shouldn't introduce line breaks; join back
                let joined = rendered.join("\n");
                let styled = crate::color::apply_styles(&joined, styles);
                let line = lines.last_mut().unwrap();
                if !styled.is_empty() {
                    line.push_str(&styled);
                    last_empty_token = false;
                }
            }
            Seg::LineBreak => {
                lines.push(String::new());
                last_empty_token = false;
            }
        }
    }
    for l in lines.iter_mut() {
        while l.ends_with(' ') {
            l.pop();
        }
    }
    lines
}
