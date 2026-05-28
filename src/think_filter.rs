pub struct ThinkBlockFilter {
    expose: bool,
    buf: String,
    mode: String, // "inside" | "outside"
}

#[derive(Debug, PartialEq, Clone)]
pub struct FilterOutput {
    pub event: String, // "think" | "delta"
    pub content: String,
}

impl ThinkBlockFilter {
    const OPEN_TAG: &'static str = "<think>";
    const CLOSE_TAG: &'static str = "</think>";

    pub fn new(expose: bool, start_inside: bool) -> Self {
        Self {
            expose,
            buf: String::new(),
            mode: if start_inside {
                "inside".to_string()
            } else {
                "outside".to_string()
            },
        }
    }

    pub fn feed(&mut self, piece: &str) -> Vec<FilterOutput> {
        self.buf.push_str(piece);
        self.drain(false)
    }

    pub fn flush(&mut self) -> Vec<FilterOutput> {
        let mut out = self.drain(true);
        if !self.buf.is_empty() {
            let ev = if self.mode == "inside" && self.expose {
                "think"
            } else {
                "delta"
            };
            if self.mode != "inside" || self.expose {
                out.push(FilterOutput {
                    event: ev.to_string(),
                    content: self.buf.clone(),
                });
            }
            self.buf.clear();
        }
        out
    }

    fn drain(&mut self, final_drain: bool) -> Vec<FilterOutput> {
        let mut out = Vec::new();
        while !self.buf.is_empty() {
            if self.mode == "outside" {
                if let Some(idx) = self.buf.find(Self::OPEN_TAG) {
                    if idx > 0 {
                        let text = self.buf[..idx].to_string();
                        out.push(FilterOutput {
                            event: "delta".to_string(),
                            content: text,
                        });
                    }
                    self.buf = self.buf[idx + Self::OPEN_TAG.len()..].to_string();
                    self.mode = "inside".to_string();
                } else {
                    let safe_len = self.safe_emit_len(&self.buf, Self::OPEN_TAG);
                    if safe_len == 0 {
                        if final_drain && !self.buf.is_empty() {
                            out.push(FilterOutput {
                                event: "delta".to_string(),
                                content: self.buf.clone(),
                            });
                            self.buf.clear();
                        }
                        return out;
                    }
                    let text = self.buf[..safe_len].to_string();
                    self.buf = self.buf[safe_len..].to_string();
                    if !text.is_empty() {
                        out.push(FilterOutput {
                            event: "delta".to_string(),
                            content: text,
                        });
                    }
                    return out;
                }
            } else {
                // inside <think>
                if let Some(idx) = self.buf.find(Self::CLOSE_TAG) {
                    let head = self.buf[..idx].to_string();
                    self.buf = self.buf[idx + Self::CLOSE_TAG.len()..].to_string();
                    if !head.is_empty() && self.expose {
                        out.push(FilterOutput {
                            event: "think".to_string(),
                            content: head,
                        });
                    }
                    self.mode = "outside".to_string();
                    // Eat exactly one trailing "\n\n" or "\n" after </think>
                    if self.buf.starts_with("\n\n") {
                        self.buf = self.buf[2..].to_string();
                    } else if self.buf.starts_with('\n') {
                        self.buf = self.buf[1..].to_string();
                    }
                } else {
                    let safe_len = self.safe_emit_len(&self.buf, Self::CLOSE_TAG);
                    if safe_len == 0 {
                        return out;
                    }
                    let text = self.buf[..safe_len].to_string();
                    self.buf = self.buf[safe_len..].to_string();
                    if !text.is_empty() && self.expose {
                        out.push(FilterOutput {
                            event: "think".to_string(),
                            content: text,
                        });
                    }
                    return out;
                }
            }
        }
        out
    }

    fn safe_emit_len(&self, buf: &str, tag: &str) -> usize {
        let max_keep = tag.len() - 1;
        let char_indices: Vec<(usize, char)> = buf.char_indices().collect();
        let tag_chars: Vec<char> = tag.chars().collect();
        let limit = std::cmp::min(max_keep, char_indices.len());

        for keep in (1..=limit).rev() {
            let buf_tail: Vec<char> = char_indices[char_indices.len() - keep..]
                .iter()
                .map(|&(_, c)| c)
                .collect();
            let tag_head = &tag_chars[..keep];
            if buf_tail == tag_head {
                return char_indices[char_indices.len() - keep].0;
            }
        }
        buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_no_think() {
        let mut filter = ThinkBlockFilter::new(true, false);
        let out1 = filter.feed("Hello ");
        let out2 = filter.feed("world!");
        let out3 = filter.flush();

        let mut combined = Vec::new();
        combined.extend(out1);
        combined.extend(out2);
        combined.extend(out3);

        // Verify combined output pieces
        let merged_delta: String = combined
            .iter()
            .filter(|o| o.event == "delta")
            .map(|o| o.content.as_str())
            .collect();
        assert_eq!(merged_delta, "Hello world!");
    }

    #[test]
    fn test_filter_with_think_expose() {
        let mut filter = ThinkBlockFilter::new(true, false);
        let out1 = filter.feed("Answer: <think>let me think</think> yes");
        let out2 = filter.flush();

        let mut combined = Vec::new();
        combined.extend(out1);
        combined.extend(out2);

        assert!(combined
            .iter()
            .any(|o| o.event == "think" && o.content == "let me think"));
        assert!(combined
            .iter()
            .any(|o| o.event == "delta" && o.content == "Answer: "));
        assert!(combined
            .iter()
            .any(|o| o.event == "delta" && o.content == " yes"));
    }

    #[test]
    fn test_filter_with_think_hidden() {
        let mut filter = ThinkBlockFilter::new(false, false);
        let out1 = filter.feed("Answer: <think>secret thought</think> yes");
        let out2 = filter.flush();

        let mut combined = Vec::new();
        combined.extend(out1);
        combined.extend(out2);

        // Assert think output is hidden
        for o in &combined {
            assert_ne!(o.event, "think");
            assert!(!o.content.contains("secret"));
        }
    }
}
