use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub stage: &'static str,
    pub message: String,
    pub span: Span,
    pub hint: Option<String>,
    pub file: Option<String>,
}

impl Diagnostic {
    pub fn new(stage: &'static str, span: Span, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
            span,
            hint: None,
            file: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        if self.file.is_none() {
            self.file = Some(file.into());
        }
        self
    }

    pub fn format_with_source(&self, source: &str) -> String {
        let mut out = String::new();
        let loc = match (&self.file, self.span.line > 0) {
            (Some(f), true) => format!("{}:{}:{}", f, self.span.line, self.span.col),
            (Some(f), false) => f.clone(),
            (None, true) => format!("line {}:{}", self.span.line, self.span.col),
            (None, false) => "unknown location".to_string(),
        };

        out.push_str(&format!("[{} error] {}\n", self.stage, self.message));
        out.push_str(&format!("  --> {}\n", loc));

        if self.span.line > 0 {
            let line_idx = self.span.line - 1;
            if let Some(raw_line) = source.split('\n').nth(line_idx) {
                let src_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
                let line_num = self.span.line.to_string();
                let gutter = " ".repeat(line_num.len());
                let caret_offset = self.span.col.saturating_sub(1);
                let padding = src_line
                    .chars()
                    .take(caret_offset)
                    .map(|c| if c == '\t' { '\t' } else { ' ' })
                    .collect::<String>();

                out.push_str(&format!("  {} |\n", gutter));
                out.push_str(&format!("  {} | {}\n", line_num, src_line));
                out.push_str(&format!("  {} | {}^", gutter, padding));
            }
        }

        if let Some(hint) = &self.hint {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!("  help: {}", hint));
        }

        out
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.span.line > 0 {
            write!(
                f,
                "[{} error at {}:{}] {}",
                self.stage, self.span.line, self.span.col, self.message
            )
        } else {
            write!(f, "[{} error] {}", self.stage, self.message)
        }
    }
}
