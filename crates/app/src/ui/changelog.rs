//! Lecture de `CHANGELOG.md` pour la fenêtre « Patch notes ».

/// Une version : `## X.Y.Z — date`, puis des rubriques `### Titre` et des entrées `- …`.
pub struct Release {
    pub version: String,
    pub date: Option<String>,
    pub sections: Vec<Section>,
}

pub struct Section {
    pub title: String,
    pub items: Vec<String>,
}

pub fn parse(md: &str) -> Vec<Release> {
    let mut releases: Vec<Release> = Vec::new();
    for raw in md.lines() {
        let line = raw.trim_end();
        if let Some(head) = line.strip_prefix("## ") {
            let mut parts = head.splitn(2, [' ']);
            let version = parts.next().unwrap_or_default().trim().to_string();
            let date = parts
                .next()
                .map(|rest| rest.trim().trim_start_matches(['—', '–', '-']).trim().to_string())
                .filter(|d| !d.is_empty());
            releases.push(Release {
                version,
                date,
                sections: Vec::new(),
            });
            continue;
        }
        let Some(release) = releases.last_mut() else { continue };
        if let Some(title) = line.strip_prefix("### ") {
            release.sections.push(Section {
                title: title.trim().to_string(),
                items: Vec::new(),
            });
        } else if let Some(item) = line.trim_start().strip_prefix("- ").or_else(|| line.trim_start().strip_prefix("* ")) {
            if release.sections.is_empty() {
                release.sections.push(Section {
                    title: String::new(),
                    items: Vec::new(),
                });
            }
            if let Some(section) = release.sections.last_mut() {
                section.items.push(item.trim().to_string());
            }
        } else if line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
            // Suite d'une entrée sur plusieurs lignes.
            if let Some(last) = release.sections.last_mut().and_then(|s| s.items.last_mut()) {
                last.push(' ');
                last.push_str(line.trim());
            }
        }
    }
    releases
}

/// Segment d'une entrée : texte simple, **gras**, `code` ou *italique*.
#[derive(Debug, PartialEq)]
pub enum Span<'a> {
    Plain(&'a str),
    Bold(&'a str),
    Code(&'a str),
    Italic(&'a str),
}

/// Découpe le Markdown en ligne d'une entrée.
pub fn spans(text: &str) -> Vec<Span<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let next = [("**", 0), ("`", 1), ("*", 2)]
            .iter()
            .filter_map(|(m, k)| rest.find(m).map(|i| (i, *m, *k)))
            .min_by_key(|(i, m, _)| (*i, std::cmp::Reverse(m.len())));
        let Some((start, marker, kind)) = next else {
            out.push(Span::Plain(rest));
            break;
        };
        let after = &rest[start + marker.len()..];
        let Some(end) = after.find(marker) else {
            out.push(Span::Plain(rest));
            break;
        };
        if start > 0 {
            out.push(Span::Plain(&rest[..start]));
        }
        let inner = &after[..end];
        out.push(match kind {
            0 => Span::Bold(inner),
            1 => Span::Code(inner),
            _ => Span::Italic(inner),
        });
        rest = &after[end + marker.len()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_changelog() {
        let releases = parse(include_str!("../../../../CHANGELOG.md"));
        assert!(releases.len() >= 7);
        let first = &releases[0];
        assert!(first.date.as_deref().is_some_and(|d| d.starts_with("20")));
        assert!(first.sections.iter().all(|s| !s.items.is_empty()));
        let oldest = releases.last().unwrap();
        assert_eq!(oldest.version, "0.1.0");
        assert!(oldest.date.is_none());
    }

    #[test]
    fn inline_markdown() {
        assert_eq!(
            spans("Bouton **Patch notes** et `.deb` *ici*."),
            vec![
                Span::Plain("Bouton "),
                Span::Bold("Patch notes"),
                Span::Plain(" et "),
                Span::Code(".deb"),
                Span::Plain(" "),
                Span::Italic("ici"),
                Span::Plain("."),
            ]
        );
    }
}
