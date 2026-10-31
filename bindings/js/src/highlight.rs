use serde::{Deserialize, Serialize};

/// A range of the source to color. Offsets are bytes of the source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HighlightSpan {
    pub from: usize,
    pub to: usize,
    pub tag: String,
}

/// The ranges of `src` that have a tag, in order and without overlap. The
/// definition of what is what lives in `tomet-highlight`; this is its shape
/// for JavaScript.
pub fn compute_highlight_spans(src: &str) -> Vec<HighlightSpan> {
    tomet::highlight::highlight(src)
        .into_iter()
        .map(|s| HighlightSpan {
            from: s.from,
            to: s.to,
            tag: s.tag.as_str().to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_highlight_spans() {
        let src = "=[ Heading ]\n-| Official: @link(id: \"tasks\")[Tasks]\n";
        let spans = compute_highlight_spans(src);

        let tags: Vec<(&str, &str)> = spans
            .iter()
            .map(|s| (&src[s.from..s.to], s.tag.as_str()))
            .collect();

        assert!(tags.contains(&("=", "heading")));
        assert!(tags.contains(&("-", "list")));
        assert!(tags.contains(&("@link", "tagName")));
        assert!(tags.contains(&("id", "propertyName")));
        assert!(tags.contains(&("\"tasks\"", "string")));
    }
}
