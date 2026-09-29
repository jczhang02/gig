//! Money view (spec 2.3): header numbers with take-home, received per month
//! for the last 12 months as a bar chart, and the outstanding list.

use super::{cell, cell_right, days, price};
use crate::data::money::{major, Amount};
use crate::text;
use crate::ui::RenderCx;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, Block, BorderType, Paragraph};
use ratatui::Frame;

/// Rows the chart keeps however long the outstanding list is.
const MIN_CHART: u16 = 6;
/// Widest bar; wider terminals centre the chart.
const MAX_BAR: u16 = 12;

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let m = &cx.state.data.money;
    // Title and column header, then one row per order (or "nothing").
    let owed_want = m.owed.len().max(1) as u16 + 2;
    let spare = area.height.saturating_sub(3 + 1 + MIN_CHART + 1);
    // The list gets up to half of the height; the chart keeps the rest.
    let owed_rows = owed_want.min(spare.max(3)).min((area.height / 2).max(3));
    let [head, _gap, chart, _gap2, owed] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(MIN_CHART.min(area.height)),
        Constraint::Length(1),
        Constraint::Length(owed_rows),
    ])
    .areas(area);
    header(frame, head, cx);
    bar_chart(frame, chart, cx);
    outstanding(frame, owed, cx);
}

fn header(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let m = &cx.state.data.money;
    let col = |label: &str, a: Amount, style: ratatui::style::Style| -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(label.to_string(), t.muted())),
            Line::from(Span::styled(
                major(a.gross),
                style.add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("take-home {}", major(a.take_home)),
                t.muted(),
            )),
        ]
    };
    let [a, b, c] = Layout::horizontal([Constraint::Ratio(1, 3); 3]).areas(Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(1),
        ..area
    });
    frame.render_widget(
        Paragraph::new(col("outstanding", m.outstanding, t.text().fg(t.unpaid))),
        a,
    );
    frame.render_widget(
        Paragraph::new(col("received this month", m.month, t.text())),
        b,
    );
    frame.render_widget(
        Paragraph::new(col("received this year", m.year, t.text())),
        c,
    );
}

/// Bar value text for `gross` minor units in whole major units, shortened
/// (`12.3k`, `12k`, `1.2M`) until it is narrower than `bar_width`, which is
/// what ratatui needs to print it. Empty when nothing fits.
pub fn bar_value(gross: i64, bar_width: u16) -> String {
    let whole = (gross.max(0) + 50) / 100;
    if whole == 0 {
        return String::new();
    }
    let w = usize::from(bar_width);
    let k = whole as f64 / 1000.0;
    let m = whole as f64 / 1_000_000.0;
    let mut forms = vec![whole.to_string()];
    if whole >= 1000 {
        forms.push(format!("{k:.1}k"));
        forms.push(format!("{k:.0}k"));
    }
    if whole >= 1_000_000 {
        forms.push(format!("{m:.1}M"));
        forms.push(format!("{m:.0}M"));
    }
    forms.into_iter().find(|f| f.len() < w).unwrap_or_default()
}

fn bar_chart(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let months = &cx.state.data.money.by_month;
    // The year is ambiguous in the month labels: name the span in the title.
    let span = match (months.first(), months.last()) {
        (Some(a), Some(b)) => format!(" received per month, {} to {} ", a.label, b.label),
        _ => " received per month ".to_string(),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(t.border())
        .title(Span::styled(
            text::truncate(&span, usize::from(area.width.saturating_sub(2))),
            t.title(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let n = months.len().max(1) as u16;
    let gap = 1;
    let bar_width = (inner.width.saturating_sub(gap * (n - 1)) / n).clamp(1, MAX_BAR);
    let used = (bar_width * n + gap * (n - 1)).min(inner.width);
    let chart_area = Rect {
        x: inner.x + (inner.width - used) / 2,
        width: used,
        ..inner
    };
    let bars: Vec<Bar> = months
        .iter()
        .map(|mo| {
            let gross = mo.amount.gross.max(0);
            Bar::default()
                .value(((gross + 50) / 100) as u64)
                .text_value(bar_value(gross, bar_width))
                .label(Line::from(
                    mo.label.get(5..).unwrap_or(&mo.label).to_string(),
                ))
                .style(t.text().fg(t.accent))
                .value_style(t.base().fg(t.bg).bg(t.accent))
        })
        .collect();
    let chart = BarChart::vertical(bars)
        .bar_width(bar_width)
        .bar_gap(gap)
        .label_style(t.muted());
    frame.render_widget(chart, chart_area);
}

fn outstanding(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let owed = &cx.state.data.money.owed;
    let mut lines = vec![Line::from(Span::styled(
        " outstanding (delivered, unpaid; days since delivery)",
        t.title(),
    ))];
    if owed.is_empty() {
        lines.push(Line::from(Span::styled(" nothing outstanding", t.muted())));
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }
    let slug_w = owed
        .iter()
        .map(|o| text::width(&o.slug))
        .max()
        .unwrap_or(4)
        .clamp(4, 20);
    let price_w = owed
        .iter()
        .map(|o| text::width(&price(o.price_minor)))
        .max()
        .unwrap_or(1)
        .clamp(5, 12);
    let cur_w = owed
        .iter()
        .map(|o| text::width(&o.currency))
        .max()
        .unwrap_or(3);
    const DAYS: usize = 6;
    // " " slug _ price _ cur _ days __ title
    let fixed = 1 + slug_w + 1 + price_w + 1 + cur_w + 1 + DAYS + 2;
    let title_w = usize::from(area.width).saturating_sub(fixed);
    let d = t.muted();
    let mut head = vec![
        Span::raw(" "),
        cell("slug", slug_w, d),
        Span::raw(" "),
        cell_right("owed", price_w, d),
        Span::raw(" "),
        cell("", cur_w, d),
        Span::raw(" "),
        cell_right("days", DAYS, d),
    ];
    if title_w >= 6 {
        head.push(Span::raw("  "));
        head.push(cell("title", title_w, d));
    }
    lines.push(Line::from(head));
    let rows = usize::from(area.height).saturating_sub(lines.len());
    // When the list does not fit, its last row says how many are left out.
    let shown = if owed.len() > rows {
        rows.saturating_sub(1)
    } else {
        owed.len()
    };
    for o in owed.iter().take(shown) {
        let mut spans = vec![
            Span::raw(" "),
            cell(&o.slug, slug_w, t.text()),
            Span::raw(" "),
            cell_right(&price(o.price_minor), price_w, t.text().fg(t.unpaid)),
            Span::raw(" "),
            cell(&o.currency, cur_w, t.muted()),
            Span::raw(" "),
            cell_right(&days(o.days), DAYS, t.muted()),
        ];
        if title_w >= 6 {
            let title = cx
                .state
                .data
                .order(o.order_id)
                .map_or("", |r| r.order.title.as_str());
            spans.push(Span::raw("  "));
            spans.push(cell(title, title_w, t.muted()));
        }
        lines.push(Line::from(spans));
    }
    if shown < owed.len() {
        lines.push(Line::from(Span::styled(
            format!(" +{} more", owed.len() - shown),
            t.muted(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_values_fit_their_bar() {
        assert_eq!(bar_value(120_000, 5), "1200");
        assert_eq!(bar_value(80_050, 5), "801");
        assert_eq!(bar_value(1_200_000, 5), "12k");
        assert_eq!(bar_value(1_234_500, 6), "12345");
        assert_eq!(bar_value(123_456_700, 7), "1235k");
        assert_eq!(bar_value(123_456_700, 5), "1.2M");
        assert_eq!(bar_value(1_234_500, 9), "12345");
        assert_eq!(bar_value(0, 5), "");
        assert_eq!(bar_value(99_999_999, 3), "1M");
        assert_eq!(bar_value(99_999_999, 2), "");
        for w in 1..12 {
            for g in [1, 99, 150, 12_345_678, 999_999_999] {
                assert!(bar_value(g, w).len() < usize::from(w).max(1), "{g} {w}");
            }
        }
    }
}
