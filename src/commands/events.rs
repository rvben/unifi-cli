use tabled::{Table, Tabled};

use crate::api::UnifiClient;
use crate::output::OutputConfig;

#[derive(Tabled)]
struct EventRow {
    #[tabled(rename = "Time")]
    time: String,
    #[tabled(rename = "Subsystem")]
    subsystem: String,
    #[tabled(rename = "Key")]
    key: String,
    #[tabled(rename = "Message")]
    message: String,
}

pub async fn list(
    client: &UnifiClient,
    out: OutputConfig,
    limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = client.list_events(limit).await?;

    if out.json {
        out.print_data(
            &serde_json::to_string_pretty(
                &events
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "key": e.key,
                            "msg": e.msg,
                            "subsystem": e.subsystem,
                            "time": e.time,
                            "datetime": e.datetime,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("failed to serialize JSON"),
        );
    } else {
        let rows: Vec<EventRow> = events
            .iter()
            .map(|e| EventRow {
                time: e.datetime.as_deref().unwrap_or("-").to_string(),
                subsystem: e.subsystem.as_deref().unwrap_or("-").to_string(),
                key: e.key.as_deref().unwrap_or("-").to_string(),
                message: truncate(e.msg.as_deref().unwrap_or("-"), 80),
            })
            .collect();

        out.print_data(&Table::new(rows).to_string());
    }
    out.print_message(&format!("\n{} events", events.len()));
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
        result.push('…');
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
        assert_eq!(truncate("hello!", 5), "hell…");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world, this is long", 10), "hello wor…");
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
        assert_eq!(truncate("hello", 1), "…");
    }

    #[test]
    fn truncate_unicode() {
        assert_eq!(truncate("αβγδεζ", 4), "αβγ…");
    }

    #[test]
    fn truncate_unicode_exact() {
        assert_eq!(truncate("αβγδ", 4), "αβγδ");
    }
}
