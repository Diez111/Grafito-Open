//! Buffer de texto simple para la pizarra: edición inline, caret y auto-resize.

/// Buffer de texto con caret, para edición WYSIWYG-lite.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextBuffer {
    content: String,
    caret: usize,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    pub fn insert(&mut self, character: char) {
        let byte_index = self
            .content
            .chars()
            .take(self.caret)
            .map(char::len_utf8)
            .sum::<usize>();
        self.content.insert(byte_index, character);
        self.caret += 1;
    }

    pub fn backspace(&mut self) {
        if self.caret > 0 {
            self.caret -= 1;
            let byte_index = self
                .content
                .chars()
                .take(self.caret)
                .map(char::len_utf8)
                .sum::<usize>();
            self.content.remove(byte_index);
        }
    }

    pub fn move_caret(&mut self, delta: isize) {
        let count = self.content.chars().count() as isize;
        self.caret = (self.caret as isize + delta).clamp(0, count) as usize;
    }

    /// Caja estimada del texto en `at` para auto-resize del lienzo.
    pub fn estimated_bounds_at(&self, at: (f64, f64), font_size: f64) -> ((f64, f64), (f64, f64)) {
        let lines = self.content.lines().count().max(1) as f64;
        let width = self
            .content
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f64
            * font_size
            * 0.55;
        let height = (lines - 1.0) * font_size * 1.2 + font_size;
        (
            (at.0, at.1 - font_size),
            (at.0 + width, at.1 - font_size + height),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_buffer_edits_and_moves_the_caret() {
        let mut buffer = TextBuffer::new();
        for character in "y=".chars() {
            buffer.insert(character);
        }
        assert_eq!(buffer.content(), "y=");
        buffer.backspace();
        assert_eq!(buffer.content(), "y");
        buffer.move_caret(-1);
        buffer.insert('x');
        assert_eq!(buffer.content(), "xy");
        assert_eq!(buffer.caret(), 1);
    }

    #[test]
    fn text_bounds_grow_with_multiple_lines() {
        let mut buffer = TextBuffer::new();
        for character in "a\nb".chars() {
            buffer.insert(character);
        }
        let (min, max) = buffer.estimated_bounds_at((0.0, 10.0), 12.0);
        assert!(!buffer.is_empty());
        assert!(max.1 > min.1, "multi-line text grows vertically");
    }
}
