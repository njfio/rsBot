use std::{fmt, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub trait Component {
    fn render(&self, width: usize) -> Vec<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorBuffer {
    lines: Vec<String>,
    cursor: Cursor,
}

impl Default for EditorBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: Cursor::default(),
        }
    }

    pub fn from_text(text: &str) -> Self {
        let mut lines = text
            .split('\n')
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }

        Self {
            lines,
            cursor: Cursor::default(),
        }
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn insert_text(&mut self, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                self.insert_newline();
            } else {
                self.insert_char(ch);
            }
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        let line = &mut self.lines[self.cursor.line];
        let byte_index = char_to_byte_index(line, self.cursor.column);
        line.insert(byte_index, ch);
        self.cursor.column += 1;
    }

    pub fn insert_newline(&mut self) {
        let current = &mut self.lines[self.cursor.line];
        let split_index = char_to_byte_index(current, self.cursor.column);
        let tail = current.split_off(split_index);
        self.cursor.line += 1;
        self.cursor.column = 0;
        self.lines.insert(self.cursor.line, tail);
    }

    pub fn delete_backward(&mut self) {
        if self.cursor.column > 0 {
            let line = &mut self.lines[self.cursor.line];
            let start = char_to_byte_index(line, self.cursor.column - 1);
            let end = char_to_byte_index(line, self.cursor.column);
            line.replace_range(start..end, "");
            self.cursor.column -= 1;
            return;
        }

        if self.cursor.line == 0 {
            return;
        }

        let current = self.lines.remove(self.cursor.line);
        self.cursor.line -= 1;
        let previous = &mut self.lines[self.cursor.line];
        let previous_len = previous.chars().count();
        previous.push_str(&current);
        self.cursor.column = previous_len;
    }

    pub fn delete_forward(&mut self) {
        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.column < line_len {
            let line = &mut self.lines[self.cursor.line];
            let start = char_to_byte_index(line, self.cursor.column);
            let end = char_to_byte_index(line, self.cursor.column + 1);
            line.replace_range(start..end, "");
            return;
        }

        if self.cursor.line + 1 >= self.lines.len() {
            return;
        }

        let next = self.lines.remove(self.cursor.line + 1);
        self.lines[self.cursor.line].push_str(&next);
    }

    pub fn move_left(&mut self) {
        if self.cursor.column > 0 {
            self.cursor.column -= 1;
            return;
        }

        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
        }
    }

    pub fn move_right(&mut self) {
        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.column < line_len {
            self.cursor.column += 1;
            return;
        }

        if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.column = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor.line == 0 {
            return;
        }

        self.cursor.line -= 1;
        let line_len = self.lines[self.cursor.line].chars().count();
        self.cursor.column = self.cursor.column.min(line_len);
    }

    pub fn move_down(&mut self) {
        if self.cursor.line + 1 >= self.lines.len() {
            return;
        }

        self.cursor.line += 1;
        let line_len = self.lines[self.cursor.line].chars().count();
        self.cursor.column = self.cursor.column.min(line_len);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub palette: ThemePalette,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            palette: ThemePalette::default(),
        }
    }
}

impl Theme {
    pub fn from_json(source: &str) -> Result<Self, ThemeError> {
        let theme = serde_json::from_str::<Theme>(source)?;
        theme.validate()?;
        Ok(theme)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ThemeError> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|source| ThemeError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_json(&raw)
    }

    pub fn paint(&self, role: ThemeRole, text: &str) -> String {
        let color = self.palette.color_code(role);
        format!("\x1b[{color}m{text}\x1b[0m")
    }

    pub fn validate(&self) -> Result<(), ThemeError> {
        if self.name.trim().is_empty() {
            return Err(ThemeError::EmptyName);
        }

        self.palette.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemePalette {
    pub primary: String,
    pub secondary: String,
    pub accent: String,
    pub muted: String,
    pub error: String,
}

impl Default for ThemePalette {
    fn default() -> Self {
        Self {
            primary: "36".to_string(),
            secondary: "37".to_string(),
            accent: "33".to_string(),
            muted: "90".to_string(),
            error: "31".to_string(),
        }
    }
}

impl ThemePalette {
    fn validate(&self) -> Result<(), ThemeError> {
        let pairs = [
            ("primary", self.primary.as_str()),
            ("secondary", self.secondary.as_str()),
            ("accent", self.accent.as_str()),
            ("muted", self.muted.as_str()),
            ("error", self.error.as_str()),
        ];

        for (field, code) in pairs {
            if !is_valid_ansi_color_code(code) {
                return Err(ThemeError::InvalidColorCode {
                    field,
                    code: code.to_string(),
                });
            }
        }

        Ok(())
    }

    fn color_code(&self, role: ThemeRole) -> &str {
        match role {
            ThemeRole::Primary => self.primary.as_str(),
            ThemeRole::Secondary => self.secondary.as_str(),
            ThemeRole::Accent => self.accent.as_str(),
            ThemeRole::Muted => self.muted.as_str(),
            ThemeRole::Error => self.error.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeRole {
    Primary,
    Secondary,
    Accent,
    Muted,
    Error,
}

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("failed to parse theme JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read theme file {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("theme name must not be empty")]
    EmptyName,
    #[error("invalid ANSI color code '{code}' for field '{field}'")]
    InvalidColorCode { field: &'static str, code: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    content: String,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }
}

impl Component for Text {
    fn render(&self, width: usize) -> Vec<String> {
        wrap_text(&self.content, width)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderOp {
    Update { line: usize, content: String },
    ClearFrom { line: usize },
}

impl fmt::Display for RenderOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderOp::Update { line, content } => write!(f, "update({line}):{content}"),
            RenderOp::ClearFrom { line } => write!(f, "clear_from({line})"),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct DiffRenderer {
    previous: Vec<String>,
}

impl DiffRenderer {
    pub fn new() -> Self {
        Self {
            previous: Vec::new(),
        }
    }

    pub fn diff(&mut self, next: Vec<String>) -> Vec<RenderOp> {
        let mut operations = Vec::new();
        let max_len = self.previous.len().max(next.len());

        for index in 0..max_len {
            match (self.previous.get(index), next.get(index)) {
                (Some(old), Some(new)) if old != new => operations.push(RenderOp::Update {
                    line: index,
                    content: new.clone(),
                }),
                (None, Some(new)) => operations.push(RenderOp::Update {
                    line: index,
                    content: new.clone(),
                }),
                _ => {}
            }
        }

        if next.len() < self.previous.len() {
            operations.push(RenderOp::ClearFrom { line: next.len() });
        }

        self.previous = next;
        operations
    }

    pub fn snapshot(&self) -> &[String] {
        &self.previous
    }
}

pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();

    for raw_line in text.lines() {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let required = if current.is_empty() {
                word.len()
            } else {
                current.len() + 1 + word.len()
            };

            if required <= width {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
                continue;
            }

            if !current.is_empty() {
                lines.push(current);
                current = String::new();
            }

            if word.len() > width {
                let mut start = 0;
                let bytes = word.as_bytes();
                while start < bytes.len() {
                    let end = (start + width).min(bytes.len());
                    let segment = &word[start..end];
                    lines.push(segment.to_string());
                    start = end;
                }
            } else {
                current.push_str(word);
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

pub fn apply_overlay(base: &[String], overlay: &[String], top: usize, left: usize) -> Vec<String> {
    let mut output = base.to_vec();

    for (row, overlay_line) in overlay.iter().enumerate() {
        let line_index = top + row;
        while output.len() <= line_index {
            output.push(String::new());
        }

        write_at(&mut output[line_index], left, overlay_line);
    }

    output
}

fn char_to_byte_index(line: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    line.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or_else(|| line.len())
}

fn is_valid_ansi_color_code(code: &str) -> bool {
    if code.is_empty() {
        return false;
    }

    code.split(';')
        .all(|segment| !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit()))
}

fn write_at(line: &mut String, left: usize, overlay: &str) {
    let mut chars = line.chars().collect::<Vec<_>>();
    while chars.len() < left {
        chars.push(' ');
    }

    for (index, ch) in overlay.chars().enumerate() {
        let position = left + index;
        if position < chars.len() {
            chars[position] = ch;
        } else {
            chars.push(ch);
        }
    }

    *line = chars.into_iter().collect();
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        apply_overlay, wrap_text, Cursor, DiffRenderer, EditorBuffer, RenderOp, Text, Theme,
        ThemeError, ThemeRole,
    };
    use crate::Component;

    #[test]
    fn wraps_text_to_width() {
        let lines = wrap_text("one two three", 7);
        assert_eq!(lines, vec!["one two", "three"]);
    }

    #[test]
    fn wraps_long_word() {
        let lines = wrap_text("abcdefghij", 4);
        assert_eq!(lines, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn text_component_renders_with_wrap() {
        let component = Text::new("hello world");
        assert_eq!(component.render(5), vec!["hello", "world"]);
    }

    #[test]
    fn renderer_outputs_only_changed_lines() {
        let mut renderer = DiffRenderer::new();
        let first = renderer.diff(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            first,
            vec![
                RenderOp::Update {
                    line: 0,
                    content: "a".to_string()
                },
                RenderOp::Update {
                    line: 1,
                    content: "b".to_string()
                }
            ]
        );

        let second = renderer.diff(vec!["a".to_string(), "c".to_string()]);
        assert_eq!(
            second,
            vec![RenderOp::Update {
                line: 1,
                content: "c".to_string()
            }]
        );

        let third = renderer.diff(vec!["a".to_string()]);
        assert_eq!(third, vec![RenderOp::ClearFrom { line: 1 }]);
    }

    #[test]
    fn unit_theme_from_json_parses_and_paints_text() {
        let theme = Theme::from_json(
            r#"{
                "name":"ocean",
                "palette":{
                    "primary":"36",
                    "secondary":"37",
                    "accent":"33",
                    "muted":"90",
                    "error":"31"
                }
            }"#,
        )
        .expect("theme should parse");

        let painted = theme.paint(ThemeRole::Primary, "hello");
        assert_eq!(painted, "\u{1b}[36mhello\u{1b}[0m");
    }

    #[test]
    fn functional_theme_from_path_loads_valid_file() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("theme.json");
        std::fs::write(
            &path,
            r#"{
                "name":"forest",
                "palette":{
                    "primary":"32",
                    "secondary":"37",
                    "accent":"33",
                    "muted":"90",
                    "error":"31"
                }
            }"#,
        )
        .expect("write theme file");

        let theme = Theme::from_path(&path).expect("theme should load");
        assert_eq!(theme.name, "forest");
    }

    #[test]
    fn regression_theme_rejects_invalid_color_code() {
        let error = Theme::from_json(
            r#"{
                "name":"bad",
                "palette":{
                    "primary":"36;foo",
                    "secondary":"37",
                    "accent":"33",
                    "muted":"90",
                    "error":"31"
                }
            }"#,
        )
        .expect_err("invalid color must fail");

        assert!(matches!(error, ThemeError::InvalidColorCode { .. }));
    }

    #[test]
    fn integration_default_theme_is_valid() {
        let theme = Theme::default();
        theme.validate().expect("default theme should be valid");
        assert_eq!(theme.paint(ThemeRole::Error, "x"), "\u{1b}[31mx\u{1b}[0m");
    }

    #[test]
    fn unit_apply_overlay_replaces_existing_text() {
        let base = vec!["hello world".to_string()];
        let output = apply_overlay(&base, &["rust".to_string()], 0, 6);
        assert_eq!(output, vec!["hello rustd".to_string()]);
    }

    #[test]
    fn functional_apply_overlay_extends_canvas_when_needed() {
        let base = vec!["abc".to_string()];
        let output = apply_overlay(&base, &["xyz".to_string(), "123".to_string()], 1, 2);
        assert_eq!(
            output,
            vec!["abc".to_string(), "  xyz".to_string(), "  123".to_string()]
        );
    }

    #[test]
    fn regression_apply_overlay_handles_unicode_content() {
        let base = vec!["status".to_string()];
        let output = apply_overlay(&base, &["🙂ok".to_string()], 0, 7);
        assert_eq!(output, vec!["status 🙂ok".to_string()]);
    }

    #[test]
    fn integration_renderer_diff_with_overlay_updates_only_changed_lines() {
        let mut renderer = DiffRenderer::new();
        let base = vec!["alpha".to_string(), "beta".to_string()];
        let first = apply_overlay(&base, &["ONE".to_string()], 1, 0);
        let second = apply_overlay(&base, &["TWO".to_string()], 1, 0);

        let initial = renderer.diff(first);
        assert_eq!(initial.len(), 2);
        let delta = renderer.diff(second);
        assert_eq!(
            delta,
            vec![RenderOp::Update {
                line: 1,
                content: "TWOa".to_string(),
            }]
        );
    }

    #[test]
    fn unit_editor_buffer_insert_and_delete_single_line() {
        let mut editor = EditorBuffer::new();
        editor.insert_text("rust");
        assert_eq!(editor.to_text(), "rust");
        assert_eq!(editor.cursor(), Cursor { line: 0, column: 4 });

        editor.delete_backward();
        assert_eq!(editor.to_text(), "rus");
        assert_eq!(editor.cursor(), Cursor { line: 0, column: 3 });
    }

    #[test]
    fn functional_editor_buffer_multiline_editing_and_navigation() {
        let mut editor = EditorBuffer::from_text("one\ntwo");
        editor.move_down();
        editor.move_right();
        editor.move_right();
        editor.insert_newline();
        editor.insert_text("x");

        assert_eq!(editor.lines().len(), 3);
        assert_eq!(editor.to_text(), "one\ntw\nxo");
        assert_eq!(editor.cursor(), Cursor { line: 2, column: 1 });
    }

    #[test]
    fn regression_editor_delete_backward_merges_lines_without_panic() {
        let mut editor = EditorBuffer::from_text("ab\ncd");
        editor.move_down();
        editor.delete_backward();
        assert_eq!(editor.to_text(), "abcd");
        assert_eq!(editor.cursor(), Cursor { line: 0, column: 2 });
    }

    #[test]
    fn integration_editor_buffer_diff_renderer_tracks_line_changes() {
        let mut renderer = DiffRenderer::new();
        let mut editor = EditorBuffer::from_text("a\nb");

        let initial = renderer.diff(editor.lines().to_vec());
        assert_eq!(initial.len(), 2);

        editor.move_down();
        editor.insert_text("!");
        let delta = renderer.diff(editor.lines().to_vec());
        assert_eq!(
            delta,
            vec![RenderOp::Update {
                line: 1,
                content: "!b".to_string(),
            }]
        );
    }
}
