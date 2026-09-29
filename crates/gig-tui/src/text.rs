//! Display-width helpers. Titles may be Chinese, so columns are measured in
//! terminal cells (unicode-width), never in bytes or chars (spec section 3).

use std::ops::Range;
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
    let flat = flatten(s);
    wrap_ranges(&flat, width, width)
        .into_iter()
        .map(|r| flat[r].to_string())
        .collect()
}

/// Byte ranges of the rows `wrap` makes of `flat` (a string without control
/// characters), the first row `first` cells wide and the others `rest`
/// cells wide (a hanging indent is drawn in front of them). Spaces at a
/// break are left out of both rows.
pub fn wrap_ranges(flat: &str, first: usize, rest: usize) -> Vec<Range<usize>> {
    let mut width = first.max(1);
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0;
    // Byte offset just after the last break opportunity, and the cells used
    // up to it. Only set once the row has text, so a leading indent is
    // never a row of its own.
    let mut brk: Option<(usize, usize)> = None;
    let mut has_text = false;
    let trimmed = |start: usize, end: usize| start + flat[start..end].trim_end_matches(' ').len();
    for (i, g) in flat.grapheme_indices(true) {
        let w = g.width();
        if used + w > width && used > 0 {
            if g == " " {
                // The space itself is the break; it is not carried over.
                rows.push(start..trimmed(start, i));
                width = rest.max(1);
                start = i + g.len();
                used = 0;
                brk = None;
                has_text = false;
                continue;
            }
            match brk {
                Some((at, cells)) => {
                    rows.push(start..trimmed(start, at));
                    start = at;
                    used -= cells;
                }
                None => {
                    rows.push(start..i);
                    start = i;
                    used = 0;
                }
            }
            width = rest.max(1);
            brk = None;
            has_text = used > 0;
            // The carried-over word may still leave no room.
            if used + w > width && used > 0 {
                rows.push(start..i);
                start = i;
                used = 0;
                has_text = false;
            }
        }
        used += w;
        if g == " " {
            if has_text {
                brk = Some((i + g.len(), used));
            }
        } else {
            has_text = true;
            if w > 1 {
                brk = Some((i + g.len(), used));
            }
        }
    }
    rows.push(start..flat.len());
    rows
}

/// Whole units grouped by thousands with `,`: `29550` -> `29,550`.
pub fn group(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Minor units as grouped major units, without decimals when there are no
/// cents: `2955000` -> `29,550`, `80050` -> `800.50`.
pub fn money(minor: i64) -> String {
    let whole = group(minor / 100);
    let cents = (minor % 100).unsigned_abs();
    let whole = if minor < 0 && minor > -100 {
        format!("-{whole}")
    } else {
        whole
    };
    if cents == 0 {
        whole
    } else {
        format!("{whole}.{cents:02}")
    }
}

/// Compact whole units for chart labels (TUI-DESIGN.md section 11.2):
/// exact below 1,000, then one decimal and `k` or `M`, rounded half up.
pub fn compact(n: i64) -> String {
    let n = n.max(0);
    let scaled = |unit: i64, suffix: &str| {
        // Tenths, rounded half up on integers (floats print 12.85 as 12.8).
        let tenths = (n * 10 + unit / 2) / unit;
        format!("{}.{}{suffix}", tenths / 10, tenths % 10)
    };
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 && (n * 10 + 500) / 1000 < 10_000 {
        scaled(1_000, "k")
    } else {
        scaled(1_000_000, "M")
    }
}

/// `s` cut in the middle to at most `max` cells, keeping its last `keep`
/// cells whole (a package id keeps its date): `abc…2026-08-26`.
pub fn truncate_middle(s: &str, max: usize, keep: usize) -> String {
    let flat = flatten(s);
    if width(&flat) <= max {
        return flat;
    }
    if max <= keep + 1 {
        return truncate(&flat, max);
    }
    let end = tail(&flat, keep);
    let head = truncate(&flat, max - width(&end));
    format!("{head}{end}")
}

/// A link without its `https://` or `http://`.
pub fn strip_scheme(link: &str) -> &str {
    link.strip_prefix("https://")
        .or_else(|| link.strip_prefix("http://"))
        .unwrap_or(link)
}

/// Control characters shown as spaces.
pub fn flatten(s: &str) -> String {
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
    fn hanging_rows_are_narrower() {
        let s = "  score 4  decisions 3  repeat qs 0";
        let rows: Vec<&str> = wrap_ranges(s, 18, 16).into_iter().map(|r| &s[r]).collect();
        assert_eq!(rows, vec!["  score 4", "decisions 3", "repeat qs 0"]);
        // A leading indent is not a break opportunity.
        let s = "    abcdefghij";
        let rows: Vec<&str> = wrap_ranges(s, 8, 4).into_iter().map(|r| &s[r]).collect();
        assert_eq!(rows, vec!["    abcd", "efgh", "ij"]);
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
    fn numbers_group_and_compact() {
        assert_eq!(group(29550), "29,550");
        assert_eq!(group(800), "800");
        assert_eq!(group(1_234_567), "1,234,567");
        assert_eq!(group(-1600), "-1,600");
        assert_eq!(group(0), "0");
        assert_eq!(money(2_955_000), "29,550");
        assert_eq!(money(80050), "800.50");
        assert_eq!(money(-50), "-0.50");
        assert_eq!(compact(12850), "12.9k");
        assert_eq!(compact(800), "800");
        assert_eq!(compact(10000), "10.0k");
        assert_eq!(compact(3100), "3.1k");
        assert_eq!(compact(999_960), "1.0M");
        assert_eq!(compact(1_250_000), "1.3M");
    }

    #[test]
    fn middle_truncation_keeps_the_end() {
        let id = "sers-colitis-analysis-delivery-2026-08-26";
        let cut = truncate_middle(id, 38, 10);
        assert_eq!(cut, "sers-colitis-analysis-deliv\u{2026}2026-08-26");
        assert_eq!(width(&cut), 38);
        assert!(cut.ends_with("2026-08-26"));
        assert_eq!(truncate_middle("short", 38, 10), "short");
    }

    #[test]
    fn control_characters_become_spaces() {
        assert_eq!(truncate("a\nb\tc", 10), "a b c");
        assert_eq!(width("a\nb"), 3);
        assert_eq!(truncate("line one\nline two", 9), "line one\u{2026}");
    }
}
