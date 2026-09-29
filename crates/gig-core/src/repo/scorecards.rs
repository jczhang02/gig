use crate::models::Scorecard;
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};

pub fn upsert(conn: &Connection, s: &Scorecard) -> Result<Scorecard> {
    conn.execute(
        "INSERT INTO scorecards (order_id, decisions, repeat_questions, days_to_preview, cleanups, check_rejections, report_reworks, score, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(order_id) DO UPDATE SET decisions = excluded.decisions, repeat_questions = excluded.repeat_questions,
           days_to_preview = excluded.days_to_preview, cleanups = excluded.cleanups, check_rejections = excluded.check_rejections,
           report_reworks = excluded.report_reworks, score = excluded.score, note = excluded.note, created_at = excluded.created_at",
        params![
            s.order_id, s.decisions, s.repeat_questions, s.days_to_preview, s.cleanups,
            s.check_rejections, s.report_reworks, s.score, s.note, s.created_at
        ],
    )?;
    Ok(find(conn, s.order_id)?.expect("just upserted"))
}

pub fn find(conn: &Connection, order_id: i64) -> Result<Option<Scorecard>> {
    Ok(conn
        .query_row(
            "SELECT order_id, decisions, repeat_questions, days_to_preview, cleanups, check_rejections, report_reworks, score, note, created_at FROM scorecards WHERE order_id = ?1",
            [order_id],
            |r| {
                Ok(Scorecard {
                    order_id: r.get(0)?,
                    decisions: r.get(1)?,
                    repeat_questions: r.get(2)?,
                    days_to_preview: r.get(3)?,
                    cleanups: r.get(4)?,
                    check_rejections: r.get(5)?,
                    report_reworks: r.get(6)?,
                    score: r.get(7)?,
                    note: r.get(8)?,
                    created_at: r.get(9)?,
                })
            },
        )
        .optional()?)
}
