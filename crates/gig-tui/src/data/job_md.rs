//! Read-only view of a project's `.gig/JOB.md`: the tail of "## Status" and
//! the unanswered items of "## Client questions". Older projects use the
//! Chinese headings "## 状态" and "## 待客户确认"; both are accepted.
//!
//! JOB.md is written by agents and by hand, so parsing never fails: a missing
//! or odd file just yields fewer entries.

use std::path::Path;

/// Entries shown in the detail's "Latest status" section.
pub const STATUS_ENTRIES: usize = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobMd {
    /// The file existed and was read.
    pub found: bool,
    /// The last `STATUS_ENTRIES` entries of "Status", newest first (the
    /// section is an append-only dated log, so file order is age order).
    pub status: Vec<String>,
    /// Items of "Client questions" not marked answered, in file order.
    pub open_questions: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Status,
    Questions,
    Other,
}

impl JobMd {
    /// Read and parse `path`; a missing or unreadable file gives the
    /// default. Invalid UTF-8 is read lossily: the file exists and its
    /// sections are still worth showing.
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => Self::parse(&String::from_utf8_lossy(&bytes)),
            Err(_) => Self::default(),
        }
    }

    pub fn parse(text: &str) -> Self {
        let mut status = Vec::new();
        let mut questions = Vec::new();
        let mut section = Section::Other;
        let mut current: Vec<&str> = Vec::new();
        let mut in_fence = false;

        let flush = |section: Section,
                     current: &mut Vec<&str>,
                     status: &mut Vec<String>,
                     questions: &mut Vec<String>| {
            if current.is_empty() {
                return;
            }
            let entry = current.join("\n");
            current.clear();
            if is_placeholder(&entry) {
                return;
            }
            match section {
                Section::Status => status.push(entry),
                Section::Questions => questions.push(entry),
                Section::Other => {}
            }
        };

        for line in text.lines() {
            let trimmed = line.trim();
            // Code blocks are skipped whole: they hold commands and logs,
            // and a "## " inside one is not a heading.
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence {
                continue;
            }
            if let Some(title) = line.strip_prefix("## ") {
                flush(section, &mut current, &mut status, &mut questions);
                section = heading_section(title);
                continue;
            }
            if line.starts_with("# ") {
                flush(section, &mut current, &mut status, &mut questions);
                section = Section::Other;
                continue;
            }
            if section == Section::Other {
                continue;
            }
            // A "### sub-heading" ends the entry but stays in the section.
            if trimmed.is_empty() || line.starts_with("###") {
                flush(section, &mut current, &mut status, &mut questions);
                continue;
            }
            if starts_item(line) {
                flush(section, &mut current, &mut status, &mut questions);
                current.push(strip_marker(line));
            } else {
                current.push(trimmed);
            }
        }
        flush(section, &mut current, &mut status, &mut questions);

        let status = status.into_iter().rev().take(STATUS_ENTRIES).collect();
        let open_questions = questions.into_iter().filter(|q| !is_answered(q)).collect();
        Self {
            found: true,
            status,
            open_questions,
        }
    }

    /// The newest status entry, for the Orders list's second line.
    pub fn latest_status(&self) -> Option<&str> {
        self.status.first().map(String::as_str)
    }
}

fn heading_section(title: &str) -> Section {
    let t = title.trim().trim_end_matches(':').trim().to_lowercase();
    match t.as_str() {
        "status" | "状态" => Section::Status,
        "client questions" | "待客户确认" => Section::Questions,
        _ => Section::Other,
    }
}

/// A top-level list item: `- `, `* `, `+ `, `1. ` or `1) ` at column 0.
fn starts_item(line: &str) -> bool {
    marker_len(line).is_some()
}

fn marker_len(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    if b.len() >= 2 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ' {
        return Some(2);
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0
        && b.len() > digits + 1
        && matches!(b[digits], b'.' | b')')
        && b[digits + 1] == b' '
    {
        return Some(digits + 2);
    }
    None
}

fn strip_marker(line: &str) -> &str {
    let n = marker_len(line).unwrap_or(0);
    line[n..].trim()
}

/// Template placeholders such as "(none)", "(无)" or "(to fill: ...)".
fn is_placeholder(entry: &str) -> bool {
    let e = entry.trim();
    e.starts_with('(') && e.ends_with(')') && !e.contains('\n')
        || e.starts_with('\u{ff08}') && e.ends_with('\u{ff09}')
}

/// The skill marks answered questions by appending "answered, see decision N"
/// ("已答, 见决策 N" in Chinese projects); a ticked checkbox counts too.
/// "unanswered" does not count.
fn is_answered(entry: &str) -> bool {
    let lower = entry.to_lowercase();
    if lower.contains("已答") || lower.starts_with("[x]") {
        return true;
    }
    lower
        .match_indices("answered")
        .any(|(i, _)| !lower[..i].ends_with("un"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: &str = "# Denoise tool

- Slug: tk-dtf. Project type: tool.

## Client request

> make it faster
- not an entry: other section

## Client questions

- 2026-09-20: Which output format, PNG or TIFF? Default PNG.
- 2026-09-21: Keep the original file names? answered, see decision 3.
- 2026-09-22: Is Windows 7 still needed?
  If yes we ship a 32-bit build.
- 2026-09-23: Still unanswered: batch size limit?

## Status

- 2026-09-10: project created. Not sent out yet.
- 2026-09-12: parser done.
- 2026-09-15: preview built, 19 images.
  Commit abc123; remaining: manual.
- 2026-09-18: full package checked.

```text
## Status
- 2026-09-19: inside a code block, not a heading
```
";

    const ZH: &str = "# 图像去噪工具

## 待客户确认

(无)

## 状态

- 2026-08-01: 立项, 建立目录与基本文件. 尚未对外发送.
- 2026-08-03: 完成预览.
";

    #[test]
    fn english_status_tail_newest_first() {
        let j = JobMd::parse(EN);
        assert!(j.found);
        assert_eq!(j.status.len(), 3);
        assert_eq!(j.status[0], "2026-09-18: full package checked.");
        assert_eq!(
            j.status[1],
            "2026-09-15: preview built, 19 images.\nCommit abc123; remaining: manual."
        );
        assert_eq!(j.status[2], "2026-09-12: parser done.");
        assert_eq!(j.latest_status(), Some("2026-09-18: full package checked."));
    }

    #[test]
    fn english_open_questions() {
        let j = JobMd::parse(EN);
        assert_eq!(
            j.open_questions,
            vec![
                "2026-09-20: Which output format, PNG or TIFF? Default PNG.".to_string(),
                "2026-09-22: Is Windows 7 still needed?\nIf yes we ship a 32-bit build.".into(),
                "2026-09-23: Still unanswered: batch size limit?".into(),
            ]
        );
    }

    #[test]
    fn chinese_headings_and_placeholder() {
        let j = JobMd::parse(ZH);
        assert!(j.open_questions.is_empty());
        assert_eq!(
            j.status,
            vec![
                "2026-08-03: 完成预览.".to_string(),
                "2026-08-01: 立项, 建立目录与基本文件. 尚未对外发送.".into(),
            ]
        );
    }

    #[test]
    fn chinese_answered_marker() {
        let text = "## 待客户确认\n\n1. 2026-08-02: 输出格式? 已答, 见决策 2.\n2. 2026-08-02: 需要 GPU 吗?\n";
        let j = JobMd::parse(text);
        assert_eq!(
            j.open_questions,
            vec!["2026-08-02: 需要 GPU 吗?".to_string()]
        );
        assert!(j.status.is_empty());
    }

    #[test]
    fn paragraphs_without_bullets_are_entries() {
        let text = "## Status\n\n2026-07-01 kickoff.\n\n2026-07-02 done,\nall green.\n";
        let j = JobMd::parse(text);
        assert_eq!(
            j.status,
            vec![
                "2026-07-02 done,\nall green.".to_string(),
                "2026-07-01 kickoff.".into()
            ]
        );
    }

    #[test]
    fn sub_headings_are_not_entries() {
        let text =
            "## Status\n\n### Phase 1\n- 2026-07-01: kickoff.\n### Phase 2\n- 2026-07-05: done.\n";
        let j = JobMd::parse(text);
        assert_eq!(
            j.status,
            vec![
                "2026-07-05: done.".to_string(),
                "2026-07-01: kickoff.".into()
            ]
        );
    }

    #[test]
    fn missing_file_is_empty() {
        let j = JobMd::load(Path::new("/nonexistent/.gig/JOB.md"));
        assert_eq!(j, JobMd::default());
        assert!(!j.found);
        assert_eq!(JobMd::parse("").status.len(), 0);
    }
}
