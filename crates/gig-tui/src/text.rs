//! Display-width helpers. Titles may be Chinese, so columns are measured in
//! terminal cells (unicode-width), never in bytes or chars (spec section 3).

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The ellipsis appended when text is cut. One cell wide.
pub const ELLIPSIS: char = '\u{2026}';

/// Cells `s` takes on screen once control characters are shown as spaces.
/// Measured per grapheme cluster, as ratatui draws it, so emoji with a
/// variation selector or a ZWJ sequence count as the terminal shows them.
pub fn width(s: &str) -> usize {
    flatten(s).graphemes(true).map(grapheme_width).sum()
}

/// `s` on one line, cut to at most `max` cells with a trailing ellipsis when
/// it does not fit. Control characters (newlines, tabs) become spaces so a
/// multi-line note cannot break a row. The result may be one cell short of
/// `max` when a double-width character would straddle the edge.
pub fn truncate(s: &str, max: usize) -> String {
    let flat = flatten(s);
    if width(&flat) <= max {
        return flat;
    }
    if max == 0 {
        return String::new();
    }
    let budget = max - 1;
    let mut out = String::new();
    let mut used = 0;
    for g in flat.graphemes(true) {
        let w = grapheme_width(g);
        if used + w > budget {
            break;
        }
        used += w;
        out.push_str(g);
    }
    out.push(ELLIPSIS);
    out
}

/// `s` on one line, cut from the left to at most `max` cells with a
/// leading ellipsis: for paths, where the end names the thing.
pub fn truncate_left(s: &str, max: usize) -> String {
    let flat = flatten(s);
    if width(&flat) <= max {
        return flat;
    }
    if max == 0 {
        return String::new();
    }
    format!("{ELLIPSIS}{}", tail(&flat, max - 1))
}

/// The last `max` cells of `s` (whole grapheme clusters only).
pub fn tail(s: &str, max: usize) -> String {
    let mut used = 0;
    let mut start = s.len();
    for (i, g) in s.grapheme_indices(true).rev() {
        let w = grapheme_width(g);
        if used + w > max {
            break;
        }
        used += w;
        start = i;
    }
    s[start..].to_string()
}

/// `truncate`, then right-pad with spaces to exactly `cells` wide.
pub fn fit(s: &str, cells: usize) -> String {
    let mut out = truncate(s, cells);
    let w = width(&out);
    out.extend(std::iter::repeat_n(' ', cells.saturating_sub(w)));
    out
}

/// `s` split into rows of at most `width` cells. Rows break after the last
/// space (or after a wide CJK character) that fits, so English words stay
/// whole; a word longer than the row is cut hard. Control characters become
/// spaces. An empty `s` is one empty row; a zero width is treated as 1.
pub fn wrap(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let flat = flatten(s);
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut used = 0;
    // Byte offset in `row` just after the last break opportunity, and the
    // cells used up to it.
    let mut brk: Option<(usize, usize)> = None;
    for g in flat.graphemes(true) {
        let w = grapheme_width(g);
        if used + w > width && used > 0 {
            if g == " " {
                // The space itself is the break; it is not carried over.
                rows.push(row.trim_end().to_string());
                row = String::new();
                used = 0;
                brk = None;
                continue;
            }
            match brk {
                Some((at, cells)) => {
                    let rest = row.split_off(at);
                    rows.push(row.trim_end().to_string());
                    row = rest;
                    used -= cells;
                }
                None => {
                    rows.push(std::mem::take(&mut row));
                    used = 0;
                }
            }
            brk = None;
            // The carried-over word may still leave no room.
            if used + w > width && used > 0 {
                rows.push(std::mem::take(&mut row));
                used = 0;
            }
        }
        row.push_str(g);
        used += w;
        if g == " " || w > 1 {
            brk = Some((row.len(), used));
        }
    }
    rows.push(row);
    rows
}

/// Control characters shown as spaces.
fn flatten(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn grapheme_width(g: &str) -> usize {
    g.width()
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
    fn wrap_by_cells() {
        assert_eq!(wrap("", 5), vec![""]);
        assert_eq!(wrap("abcdefg", 3), vec!["abc", "def", "g"]);
        // Wide characters never straddle a row end.
        assert_eq!(wrap("图像去噪", 5), vec!["图像", "去噪"]);
        for row in wrap("SERS 数据分析 and more text", 7) {
            assert!(width(&row) <= 7, "{row}");
        }
    }

    #[test]
    fn wrap_keeps_words_whole() {
        assert_eq!(
            wrap("cannot start. delivered", 16),
            vec!["cannot start.", "delivered"]
        );
        assert_eq!(
            wrap("the order was delivered today", 12),
            vec!["the order", "was", "delivered", "today"]
        );
        // A word longer than the row is still cut.
        assert_eq!(wrap("abcdefgh ij", 5), vec!["abcde", "fgh", "ij"]);
        // CJK wraps anywhere, so no row starts with the punctuation.
        assert_eq!(
            wrap("cannot start. 订单已交付", 16),
            vec!["cannot start. 订", "单已交付"]
        );
        assert_eq!(wrap("图像 去噪工具", 6), vec!["图像", "去噪工", "具"]);
    }

    #[test]
    fn graphemes_are_measured_whole() {
        // Heart with VS16: one cluster, two cells, as ratatui draws it.
        let heart = "\u{2764}\u{fe0f}";
        assert_eq!(width(heart), 2);
        assert_eq!(width(&fit(&format!("a{heart}b"), 6)), 6);
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        assert_eq!(width(family), 2);
        // Never split inside a cluster.
        assert_eq!(
            truncate(&format!("{family}{family}"), 3),
            format!("{family}\u{2026}")
        );
        assert_eq!(tail(&format!("x{family}"), 2), family);
    }

    #[test]
    fn left_truncation_keeps_the_end() {
        assert_eq!(truncate_left("/mnt/m/projects/ocr", 8), "\u{2026}cts/ocr");
        assert_eq!(truncate_left("/a/b", 8), "/a/b");
        assert_eq!(width(&truncate_left("/数据/发票识别", 7)), 7);
        assert_eq!(tail("图像去噪", 5), "去噪");
    }

    #[test]
    fn control_characters_become_spaces() {
        assert_eq!(truncate("a\nb\tc", 10), "a b c");
        assert_eq!(width("a\nb"), 3);
        assert_eq!(truncate("line one\nline two", 9), "line one\u{2026}");
    }
}
