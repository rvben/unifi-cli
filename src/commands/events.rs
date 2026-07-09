use owo_colors::OwoColorize;

use crate::api::UnifiClient;
use crate::output::{OutputConfig, use_color};

pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
    /// Field names already validated against `fields::EVENTS_LIST`.
    pub fields: Option<Vec<String>>,
}

pub async fn list(
    client: &UnifiClient,
    out: OutputConfig,
    pagination: Pagination,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = client.list_events(pagination.limit).await?;
    let total = events.len();
    let paginated: Vec<_> = events
        .into_iter()
        .skip(pagination.offset)
        .take(pagination.limit)
        .collect();

    if out.is_json() {
        let items: Vec<serde_json::Value> = paginated
            .iter()
            .map(|e| {
                let mut obj = serde_json::json!({
                    "key": e.key,
                    "msg": e.msg,
                    "subsystem": e.subsystem,
                    "time": e.time,
                    "datetime": e.datetime,
                });
                if let Some(ref keep) = pagination.fields {
                    let map = obj.as_object_mut().expect("event is a JSON object");
                    map.retain(|k, _| keep.iter().any(|f| f == k));
                }
                obj
            })
            .collect();
        out.print_data(
            &serde_json::to_string_pretty(&serde_json::json!({
                "items": items,
                "total": total,
                "limit": pagination.limit,
                "offset": pagination.offset,
            }))
            .expect("failed to serialize JSON"),
        );
    } else {
        let color = use_color();
        let header = format!(
            "{:<26} {:<10} {:<24} {}",
            "Time", "Subsystem", "Key", "Message"
        );
        if color {
            println!("{}", header.bold());
            println!("{}", "-".repeat(100).dimmed());
        } else {
            println!("{header}");
            println!("{}", "-".repeat(100));
        }

        for e in &paginated {
            let time = e.datetime.as_deref().unwrap_or("-");
            let subsystem = e.subsystem.as_deref().unwrap_or("-");
            let key = e.key.as_deref().unwrap_or("-");
            let msg = truncate(e.msg.as_deref().unwrap_or("-"), 80);

            if color {
                println!(
                    " {:<25} {:<10} {:<24} {}",
                    time.dimmed(),
                    subsystem,
                    key,
                    msg,
                );
            } else {
                println!(" {:<25} {:<10} {:<24} {}", time, subsystem, key, msg);
            }
        }
    }
    out.print_message(&format!("\n{} events", paginated.len()));
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let mut chars = s.chars();
    let mut result: String = (&mut chars).take(max).collect();
    if chars.next().is_some() {
        result.pop();
        result.push('\u{2026}');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_one_over() {
        assert_eq!(truncate("hello!", 5), "hell\u{2026}");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(
            truncate("hello world, this is long", 10),
            "hello wor\u{2026}"
        );
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn truncate_max_zero() {
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn truncate_max_one() {
        assert_eq!(truncate("hello", 1), "\u{2026}");
    }

    #[test]
    fn truncate_unicode() {
        assert_eq!(
            truncate("\u{03b1}\u{03b2}\u{03b3}\u{03b4}\u{03b5}\u{03b6}", 4),
            "\u{03b1}\u{03b2}\u{03b3}\u{2026}"
        );
    }

    #[test]
    fn truncate_unicode_exact() {
        assert_eq!(
            truncate("\u{03b1}\u{03b2}\u{03b3}\u{03b4}", 4),
            "\u{03b1}\u{03b2}\u{03b3}\u{03b4}"
        );
    }
}
