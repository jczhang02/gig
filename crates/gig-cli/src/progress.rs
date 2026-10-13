//! Upload progress on stderr for `package upload --yes` and
//! `artifact upload --yes`. stdout stays the one JSON document.
//!
//! The line, in both modes:
//! `upload <sent> / <total> (<pct>%) <rate>/s ETA <eta>`, for example
//! `upload 12.3 GiB / 32.7 GiB (37%) 18.2 MiB/s ETA 19m05s`. Agents read it,
//! so its fields and their order stay fixed. A rate or ETA not known yet
//! shows as `--`; a total of 0 counts as done (100%, ETA 0s).
//!
//! The rate is the average since the first report, so time the uploader
//! spends before it sends (hashing, say) does not count. When the first
//! report is the only one (an uploader that reports once at the end), the
//! rate is the average since the renderer started.

use gig_core::delivery::{Uploader, WithProgress};
use gig_core::Result;
use std::io::{IsTerminal, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Run `upload` with `uploader` reporting to stderr: [`Mode::Terminal`] when
/// stderr is a terminal, else [`Mode::Plain`].
pub fn to_stderr<T>(
    uploader: &dyn Uploader,
    upload: impl FnOnce(&dyn Uploader) -> Result<T>,
) -> Result<T> {
    let stderr = std::io::stderr();
    let mode = if stderr.is_terminal() {
        Mode::Terminal
    } else {
        Mode::Plain
    };
    let start = Instant::now();
    let renderer = Renderer::new(stderr, move || start.elapsed(), mode);
    with_progress(&Arc::new(Mutex::new(renderer)), uploader, upload)
}

/// Run `upload` with `uploader` reporting to `renderer`, then finish the
/// renderer, whether the upload worked or not.
pub fn with_progress<T, W, C>(
    renderer: &Arc<Mutex<Renderer<W, C>>>,
    uploader: &dyn Uploader,
    upload: impl FnOnce(&dyn Uploader) -> Result<T>,
) -> Result<T>
where
    W: Write + Send + 'static,
    C: Fn() -> Duration + Send + 'static,
{
    let sink = Arc::clone(renderer);
    let wrapped = WithProgress::new(
        uploader,
        Arc::new(move |sent, total| {
            if let Ok(mut r) = sink.lock() {
                r.report(sent, total);
            }
        }),
    );
    let result = upload(&wrapped);
    if let Ok(mut r) = renderer.lock() {
        r.finish();
    }
    result
}

/// How the progress goes out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// stderr is a terminal: one line redrawn after `\r`, at most 4 times a
    /// second, the last report always drawn but 100% only once, a newline
    /// at the end.
    Terminal,
    /// stderr is a pipe or file (an agent runs gig): a line each time
    /// another 10% is done or 60 s have passed since the last line (since
    /// the start for the first), whichever comes first. No `\r`.
    Plain,
}

/// Turns `(sent, total)` reports into progress text on `out`. `clock` gives
/// the time since the upload started. Write errors are ignored: progress
/// must never fail an upload.
pub struct Renderer<W, C> {
    out: W,
    clock: C,
    mode: Mode,
    /// Time and bytes of the first report.
    first: Option<(Duration, u64)>,
    /// When the last line was written (plain) or drawn (terminal).
    last_line_at: Option<Duration>,
    /// Tenths done at the last plain line.
    last_tenth: u64,
    /// Terminal: length of the line on screen, 0 before the first draw.
    drawn_len: usize,
    /// Terminal: the last report's line when it was not drawn.
    skipped: Option<String>,
    /// Terminal: a 100% line is on screen; later 100% reports are not drawn.
    done_drawn: bool,
}

impl<W: Write, C: Fn() -> Duration> Renderer<W, C> {
    pub fn new(out: W, clock: C, mode: Mode) -> Self {
        Self {
            out,
            clock,
            mode,
            first: None,
            last_line_at: None,
            last_tenth: 0,
            drawn_len: 0,
            skipped: None,
            done_drawn: false,
        }
    }

    /// One report from the uploader.
    pub fn report(&mut self, sent: u64, total: u64) {
        let now = (self.clock)();
        let first = *self.first.get_or_insert((now, sent));
        let since_line = self.last_line_at.map(|at| now.saturating_sub(at));
        let line = line(sent, total, now, first);
        match self.mode {
            Mode::Terminal => {
                let done = sent >= total;
                let due = since_line.is_none_or(|d| d >= Duration::from_millis(250));
                if done && self.done_drawn {
                    // 100% is on screen already: no second redraw.
                    return;
                }
                if due || done {
                    self.draw(&line);
                    self.last_line_at = Some(now);
                    self.skipped = None;
                    self.done_drawn = done;
                } else {
                    self.skipped = Some(line);
                }
            }
            Mode::Plain => {
                let tenth = percent(sent, total) / 10;
                // The first line comes 60 s after the start.
                let since_line = since_line.unwrap_or(now);
                if tenth > self.last_tenth || since_line >= Duration::from_secs(60) {
                    let _ = writeln!(self.out, "{line}");
                    self.last_line_at = Some(now);
                    self.last_tenth = tenth;
                }
            }
        }
    }

    /// The upload is over (done or failed): the terminal line gets the last
    /// report and a newline.
    pub fn finish(&mut self) {
        if let Some(line) = self.skipped.take() {
            self.draw(&line);
        }
        if self.drawn_len > 0 {
            let _ = writeln!(self.out);
            self.drawn_len = 0;
        }
        let _ = self.out.flush();
    }

    /// Redraw the terminal line, blanking what a longer one left.
    fn draw(&mut self, line: &str) {
        let pad = self.drawn_len.saturating_sub(line.len());
        let _ = write!(self.out, "\r{line}{:pad$}", "");
        let _ = self.out.flush();
        self.drawn_len = line.len();
    }

    #[cfg(test)]
    pub fn into_inner(self) -> W {
        self.out
    }
}

/// Whole percent done; a total of 0 is done.
fn percent(sent: u64, total: u64) -> u64 {
    if total == 0 {
        return 100;
    }
    (u128::from(sent.min(total)) * 100 / u128::from(total)) as u64
}

/// The progress line at `now` for the report `(sent, total)`, the first
/// report being `first`.
fn line(sent: u64, total: u64, now: Duration, first: (Duration, u64)) -> String {
    // The only report so far: measure from the start.
    let (since, base) = if first == (now, sent) {
        (Duration::ZERO, 0)
    } else {
        first
    };
    let ms = now.saturating_sub(since).as_millis();
    let bytes = u128::from(sent.saturating_sub(base));
    let rate = match ms {
        0 => "--".to_string(),
        ms => human_size((bytes * 1000 / ms) as u64),
    };
    let left = u128::from(total.saturating_sub(sent));
    let eta = if left == 0 {
        "0s".to_string()
    } else if ms == 0 || bytes == 0 {
        "--".to_string()
    } else {
        human_duration((left * ms).div_ceil(bytes).div_ceil(1000) as u64)
    };
    format!(
        "upload {} / {} ({}%) {rate}/s ETA {eta}",
        human_size(sent),
        human_size(total),
        percent(sent, total)
    )
}

/// `812 B`, `1.5 KiB`, `12.3 GiB`.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1024.0;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

/// `45s`, `19m05s`, `2h03m09s`.
fn human_duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, _) => format!("{m}m{s:02}s"),
        _ => format!("{h}h{m:02}m{s:02}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gig_core::delivery::{UploadOpts, UploadResult};
    use gig_core::Error;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const GIB: u64 = 1024 * 1024 * 1024;

    /// A clock the test moves by hand, in milliseconds since the start.
    #[derive(Clone, Default)]
    struct FakeClock(Arc<AtomicU64>);

    impl FakeClock {
        fn set_ms(&self, ms: u64) {
            self.0.store(ms, Ordering::Relaxed);
        }

        fn clock(&self) -> impl Fn() -> Duration + Send + 'static {
            let t = Arc::clone(&self.0);
            move || Duration::from_millis(t.load(Ordering::Relaxed))
        }
    }

    fn text(r: Renderer<Vec<u8>, impl Fn() -> Duration + Send>) -> String {
        String::from_utf8(r.into_inner()).unwrap()
    }

    #[test]
    fn no_reports_no_output() {
        let t = FakeClock::default();
        for mode in [Mode::Terminal, Mode::Plain] {
            let mut r = Renderer::new(Vec::new(), t.clock(), mode);
            r.finish();
            assert_eq!(text(r), "");
        }
    }

    #[test]
    fn plain_writes_a_line_per_ten_percent() {
        let t = FakeClock::default();
        let mut r = Renderer::new(Vec::new(), t.clock(), Mode::Plain);
        t.set_ms(5_000);
        r.report(0, 10 * GIB);
        t.set_ms(30_000);
        r.report(GIB * 9 / 10, 10 * GIB);
        assert_eq!(r.out, b"", "0% and 9% write nothing");
        t.set_ms(45_000);
        r.report(GIB, 10 * GIB);
        r.finish();
        assert_eq!(
            text(r),
            "upload 1.0 GiB / 10.0 GiB (10%) 25.6 MiB/s ETA 6m00s\n"
        );
    }

    #[test]
    fn plain_writes_a_line_after_sixty_seconds_without_one() {
        let t = FakeClock::default();
        let mut r = Renderer::new(Vec::new(), t.clock(), Mode::Plain);
        t.set_ms(1_000);
        r.report(10, 1000);
        t.set_ms(61_000);
        r.report(50, 1000); // 60 s since the start
        t.set_ms(100_000);
        r.report(60, 1000); // 39 s since that line
        t.set_ms(121_000);
        r.report(70, 1000); // 60 s since that line
        t.set_ms(125_000);
        r.report(100, 1000); // 10%
        t.set_ms(130_000);
        r.report(150, 1000); // still 1 tenth
        r.finish();
        assert_eq!(
            text(r),
            "upload 50 B / 1000 B (5%) 0 B/s ETA 23m45s\n\
             upload 70 B / 1000 B (7%) 0 B/s ETA 31m00s\n\
             upload 100 B / 1000 B (10%) 0 B/s ETA 20m40s\n"
        );
    }

    #[test]
    fn a_zero_total_is_done_once() {
        let t = FakeClock::default();
        let mut r = Renderer::new(Vec::new(), t.clock(), Mode::Plain);
        t.set_ms(2_000);
        r.report(0, 0);
        r.report(0, 0);
        r.finish();
        assert_eq!(text(r), "upload 0 B / 0 B (100%) 0 B/s ETA 0s\n");
    }

    /// Reports `parts` of three 40 MiB parts, the clock moving 100 ms per
    /// part; fails when it stops before the third.
    struct ThreeParts {
        clock: FakeClock,
        parts: u64,
    }

    impl Uploader for ThreeParts {
        fn name(&self) -> &str {
            "three-parts"
        }

        fn upload(&self, _local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
            for part in 1..=self.parts {
                self.clock.set_ms(part * 100);
                opts.report(part * 40 * MIB, 120 * MIB);
            }
            if self.parts < 3 {
                return Err(Error::Upload("part 3 failed".into()));
            }
            Ok(UploadResult {
                url: "https://example.test/x".into(),
                short_url: None,
                expires_at: None,
                provider: "three-parts".into(),
                file_size: 120 * MIB,
                pwd: None,
            })
        }
    }

    fn upload_through(uploader: &ThreeParts, mode: Mode) -> (Result<UploadResult>, String) {
        let out = Arc::new(Mutex::new(Renderer::new(
            Vec::new(),
            uploader.clock.clock(),
            mode,
        )));
        let r = with_progress(&out, uploader, |u| {
            u.upload(Path::new("/tmp/x.zip"), &UploadOpts::default())
        });
        let renderer = Arc::try_unwrap(out).ok().unwrap().into_inner().unwrap();
        (r, text(renderer))
    }

    #[test]
    fn an_upload_through_the_renderer_writes_its_reports() {
        let up = ThreeParts {
            clock: FakeClock::default(),
            parts: 3,
        };
        let (r, plain) = upload_through(&up, Mode::Plain);
        assert_eq!(r.unwrap().file_size, 120 * MIB);
        assert_eq!(
            plain,
            "upload 40.0 MiB / 120.0 MiB (33%) 400.0 MiB/s ETA 1s\n\
             upload 80.0 MiB / 120.0 MiB (66%) 400.0 MiB/s ETA 1s\n\
             upload 120.0 MiB / 120.0 MiB (100%) 400.0 MiB/s ETA 0s\n"
        );
    }

    #[test]
    fn a_failed_upload_still_ends_the_terminal_line() {
        let up = ThreeParts {
            clock: FakeClock::default(),
            parts: 2,
        };
        let (r, term) = upload_through(&up, Mode::Terminal);
        assert!(r.is_err());
        assert_eq!(
            term,
            "\rupload 40.0 MiB / 120.0 MiB (33%) 400.0 MiB/s ETA 1s\
             \rupload 80.0 MiB / 120.0 MiB (66%) 400.0 MiB/s ETA 1s\n"
        );
    }

    #[test]
    fn long_etas_show_hours() {
        let t = FakeClock::default();
        let mut r = Renderer::new(Vec::new(), t.clock(), Mode::Plain);
        t.set_ms(1_000);
        r.report(0, 1000);
        t.set_ms(401_000);
        r.report(100, 1000);
        assert_eq!(text(r), "upload 100 B / 1000 B (10%) 0 B/s ETA 1h00m00s\n");
    }

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn terminal_redraws_at_most_four_times_a_second() {
        let t = FakeClock::default();
        let mut r = Renderer::new(Vec::new(), t.clock(), Mode::Terminal);
        r.report(0, 100 * MIB);
        t.set_ms(100);
        r.report(10 * MIB, 100 * MIB); // 100 ms after the last draw
        t.set_ms(300);
        r.report(20 * MIB, 100 * MIB);
        t.set_ms(310);
        r.report(100 * MIB, 100 * MIB); // the last report: drawn anyway
        r.finish();
        assert_eq!(
            text(r),
            "\rupload 0 B / 100.0 MiB (0%) --/s ETA --\
             \rupload 20.0 MiB / 100.0 MiB (20%) 66.7 MiB/s ETA 2s\
             \rupload 100.0 MiB / 100.0 MiB (100%) 322.6 MiB/s ETA 0s\n"
        );
    }

    #[test]
    fn terminal_draws_100_percent_once() {
        // baidupcs reports 100% from its progress line, then once more when
        // the upload is shared.
        let t = FakeClock::default();
        let mut r = Renderer::new(Vec::new(), t.clock(), Mode::Terminal);
        r.report(0, 100 * MIB);
        t.set_ms(1_000);
        r.report(100 * MIB, 100 * MIB);
        t.set_ms(1_100);
        r.report(100 * MIB, 100 * MIB);
        r.finish();
        assert_eq!(
            text(r),
            "\rupload 0 B / 100.0 MiB (0%) --/s ETA --\
             \rupload 100.0 MiB / 100.0 MiB (100%) 100.0 MiB/s ETA 0s\n"
        );
    }

    #[test]
    fn terminal_blanks_what_a_shorter_line_leaves() {
        let t = FakeClock::default();
        let mut r = Renderer::new(Vec::new(), t.clock(), Mode::Terminal);
        t.set_ms(1_000);
        r.report(1000 * 1024, 2 * MIB);
        t.set_ms(2_000);
        r.report(MIB, 2 * MIB);
        r.finish();
        assert_eq!(
            text(r),
            "\rupload 1000.0 KiB / 2.0 MiB (48%) 1000.0 KiB/s ETA 2s\
             \rupload 1.0 MiB / 2.0 MiB (50%) 24.0 KiB/s ETA 43s    \n"
        );
    }

    #[test]
    fn terminal_finish_draws_a_skipped_last_report() {
        let t = FakeClock::default();
        let mut r = Renderer::new(Vec::new(), t.clock(), Mode::Terminal);
        r.report(0, 100 * MIB);
        t.set_ms(100);
        r.report(10 * MIB, 100 * MIB); // skipped, then the upload fails
        r.finish();
        assert_eq!(
            text(r),
            "\rupload 0 B / 100.0 MiB (0%) --/s ETA --\
             \rupload 10.0 MiB / 100.0 MiB (10%) 100.0 MiB/s ETA 1s\n"
        );
    }
}
