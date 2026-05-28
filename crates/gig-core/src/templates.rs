//! Template rendering using minijinja.
//!
//! Embeds default text templates exposed by the template command.

use crate::models::Order;
use crate::Error;
use crate::Result;
use minijinja::{context, Environment};

/// Embedded default templates: (name, content) pairs.
/// Name is without the `.j2` extension, matching the template subcommand.
pub const EMBEDDED_TEMPLATES: [(&str, &str); 3] = [
    ("project-readme", DEFAULT_PROJECT_README),
    ("quote-reply", DEFAULT_QUOTE_REPLY),
    ("delivery-checklist", DEFAULT_DELIVERY_CHECKLIST),
];

const DEFAULT_QUOTE_REPLY: &str = r#"Hello,

Here is my quote for the project you described:

- Project: {{ title }}
- Quote: {{ quoted_price }} {{ currency }}
- Delivery timeline: TBD

Feel free to reach out with any questions.
"#;

const DEFAULT_DELIVERY_CHECKLIST: &str = r#"# Delivery Checklist — {{ title }}

- [ ] Code tested
- [ ] Documentation updated
- [ ] Delivery package generated
- [ ] Link sent to client
"#;

/// Default project README template embedded in the binary.
const DEFAULT_PROJECT_README: &str = r#"# {{ title }}

**Slug:** `{{ slug }}`
**Quoted price:** {{ quoted_price }}
**Currency:** {{ currency }}
{% if source_org %}**Source org:** {{ source_org }}{% endif %}

## Notes

{{ notes }}
"#;

/// Render the project README for the given order.
pub fn render_project_readme(order: &Order) -> Result<String> {
    let mut env = Environment::new();
    env.add_template("project-readme.md.j2", DEFAULT_PROJECT_README)
        .map_err(|e| Error::Invalid(format!("template error: {e}")))?;
    let tmpl = env
        .get_template("project-readme.md.j2")
        .map_err(|e| Error::Invalid(format!("template error: {e}")))?;
    let result = tmpl
        .render(context! {
            title => order.title,
            slug => order.slug.as_deref().unwrap_or(""),
            quoted_price => order.quoted_price.map(|p| format!("{:.2}", p as f64 / 100.0)).unwrap_or_else(|| "TBD".into()),
            currency => order.currency,
            source_org => order.source_org.as_deref().unwrap_or(""),
            notes => order.notes.as_deref().unwrap_or(""),
        })
        .map_err(|e| Error::Invalid(format!("template render error: {e}")))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Order, OrderStatus};

    fn sample_order() -> Order {
        Order {
            id: 1,
            slug: Some("acme-scraper".into()),
            external_id: None,
            title: "Acme Scraper".into(),
            client_id: None,
            source_org: Some("Acme Corp".into()),
            source_id: None,
            project_type: None,
            status: OrderStatus::Accepted,
            quoted_price: Some(50_000),
            final_price: None,
            my_cut_ratio: 0.6,
            currency: "CNY".into(),
            dev_path: None,
            archive_path: None,
            notes: Some("Scrape product data daily".into()),
            created_at: 0,
            accepted_at: None,
            delivered_at: None,
            paid_at: None,
            archived_at: None,
        }
    }

    #[test]
    fn render_includes_title() {
        let o = sample_order();
        let out = render_project_readme(&o).unwrap();
        assert!(out.contains("Acme Scraper"), "title missing: {out}");
    }

    #[test]
    fn render_includes_slug() {
        let o = sample_order();
        let out = render_project_readme(&o).unwrap();
        assert!(out.contains("acme-scraper"), "slug missing: {out}");
    }
}
