//! Money view (spec 2.3): header numbers with take-home, received per month
//! for the last 12 months as a bar chart, and the outstanding list.

use super::{cell, cell_right, days, price};
use crate::data::money::{major, Amount};
use crate::ui::RenderCx;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, Block, BorderType, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let m = &cx.state.data.money;
    let owed_rows = (m.owed.len().max(1) as u16) + 2;
    let [head, _gap, chart, _gap2, owed] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
        Constraint::Length(owed_rows.min(area.height / 3).max(3)),
    ])
    .areas(area);
    header(frame, head, cx);
    bar_chart(frame, chart, cx);
    outstanding(frame, owed, cx);
}

fn header(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let m = &cx.state.data.money;
    let col = |label: &str, a: Amount, style| -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(label.to_string(), t.dim())),
            Line::from(Span::styled(major(a.gross), style)),
            Line::from(Span::styled(
                format!("take-home {}", major(a.take_home)),
                t.dim(),
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

fn bar_chart(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let months = &cx.state.data.money.by_month;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(t.border())
        .title(Span::styled(" received per month ", t.title()));
    let inner_w = block.inner(area).width;
    let n = months.len().max(1) as u16;
    let gap = 1;
    let bar_width = (inner_w.saturating_sub(gap * (n - 1)) / n).clamp(1, 9);
    let bars: Vec<Bar> = months
        .iter()
        .map(|mo| {
            let gross = mo.amount.gross.max(0);
            // Whole major units; the text shows cents only when present.
            Bar::default()
                .value((gross / 100) as u64)
                .text_value(if gross == 0 {
                    String::new()
                } else {
                    major(gross)
                })
                .label(Line::from(
                    mo.label.get(5..).unwrap_or(&mo.label).to_string(),
                ))
                .style(t.text().fg(t.accent))
                .value_style(t.base().fg(t.bg).bg(t.accent))
        })
        .collect();
    let chart = BarChart::vertical(bars)
        .block(block)
        .bar_width(bar_width)
        .bar_gap(gap)
        .label_style(t.dim());
    frame.render_widget(chart, area);
}

fn outstanding(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let owed = &cx.state.data.money.owed;
    let mut lines = vec![Line::from(Span::styled(
        " outstanding (delivered, unpaid)",
        t.title(),
    ))];
    if owed.is_empty() {
        lines.push(Line::from(Span::styled(" nothing outstanding", t.dim())));
    }
    for o in owed {
        lines.push(Line::from(vec![
            Span::raw(" "),
            cell(&o.slug, 24, t.text()),
            Span::raw(" "),
            cell_right(&price(o.price_minor), 10, t.text().fg(t.unpaid)),
            Span::raw(" "),
            cell(&o.currency, 4, t.dim()),
            cell_right(&days(o.days), 6, t.dim()),
            Span::styled(" since delivery", t.dim()),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
