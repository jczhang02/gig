//! Display-width helpers. Titles may be Chinese, so columns are measured in
//! terminal cells (unicode-width), never in bytes or chars (spec section 3).

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The ellipsis appended when text is cut. One cell wide.
pub const ELLIPSIS: char = '\u{2026}';

/// Cells `s` takes on screen once control characters are shown as spaces.
pub fn width(s: &str) -> usize {
    s.chars().map(cell_width).sum()
}

/// `s` on one line, cut to at most `max` cells with a trailing ellipsis when
/// it does not fit. Control characters (newlines, tabs) become spaces so a
/// multi-line note cannot break a row. The result may be one cell short of
/// `max` when a double-width character would straddle the edge.
pub fn truncate(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if flat.width() <= max {
        return flat;
    }
    if max == 0 {
        return String::new();
    }
    let budget = max - 1;
    let mut out = String::new();
    let mut used = 0;
    for c in flat.chars() {
        let w = cell_width(c);
        if used + w > budget {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push(ELLIPSIS);
    out
}

/// `truncate`, then right-pad with spaces to exactly `cells` wide.
pub fn fit(s: &str, cells: usize) -> String {
    let mut out = truncate(s, cells);
    let w = width(&out);
    out.extend(std::iter::repeat_n(' ', cells.saturating_sub(w)));
    out
}

fn cell_width(c: char) -> usize {
    if c.is_control() {
        1
    } else {
        c.width().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_untouched() {
        assert_eq!(truncate("tk-dtf", 10), "tk-dtf");
        assert_eq!(truncate("", 0), "");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn ascii_is_cut_with_ellipsis() {
        assert_eq!(truncate("abcdef", 4), "abc\u{2026}");
        assert_eq!(width(&truncate("abcdef", 4)), 4);
        assert_eq!(truncate("abcdef", 1), "\u{2026}");
        assert_eq!(truncate("abcdef", 0), "");
    }

    #[test]
    fn chinese_titles_use_display_width() {
        let title = "图像去噪工具开发"; // 8 chars, 16 cells
        assert_eq!(width(title), 16);
        assert_eq!(truncate(title, 16), title);
        // 7 cells: three wide chars (6) + ellipsis.
        assert_eq!(truncate(title, 7), "图像去\u{2026}");
        // 8 cells: the fourth wide char would need cells 7-8 plus the
        // ellipsis, so the result is one short.
        let cut = truncate(title, 8);
        assert_eq!(cut, "图像去\u{2026}");
        assert_eq!(width(&cut), 7);
        for max in 0..20 {
            assert!(width(&truncate(title, max)) <= max, "max {max}");
        }
        // One cell cannot hold a wide char: only the ellipsis remains.
        assert_eq!(truncate(title, 1), "\u{2026}");
    }

    #[test]
    fn mixed_text_and_padding() {
        let s = "SERS 数据分析";
        assert_eq!(width(s), 13);
        assert_eq!(truncate(s, 9), "SERS 数\u{2026}");
        let padded = fit(s, 9);
        assert_eq!(width(&padded), 9);
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(width(&fit("图像去噪", 5)), 5);
    }

    #[test]
    fn control_characters_become_spaces() {
        assert_eq!(truncate("a\nb\tc", 10), "a b c");
        assert_eq!(width("a\nb"), 3);
        assert_eq!(truncate("line one\nline two", 9), "line one\u{2026}");
    }
}
