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
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}…")
    }
}
