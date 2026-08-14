use crate::api::UnifiClient;
use crate::output::OutputConfig;

pub async fn list_rules(
    client: &UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let rules = client.list_firewall_rules().await?;
    let rows: Vec<_> = rules
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id, "name": r.name, "enabled": r.enabled, "ruleset": r.ruleset,
                "action": r.action, "protocol": r.protocol, "source": r.src_address,
                "destination": r.dst_address, "destination_port": r.dst_port,
                "index": r.rule_index, "logging": r.logging,
            })
        })
        .collect();
    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&rows)?);
    } else {
        println!(
            "{:<5} {:<8} {:<16} {:<8} Name",
            "Index", "Enabled", "Ruleset", "Action"
        );
        println!("{}", "-".repeat(78));
        for r in &rules {
            println!(
                "{:<5} {:<8} {:<16} {:<8} {}",
                r.rule_index
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".into()),
                if r.enabled { "yes" } else { "no" },
                r.ruleset.as_deref().unwrap_or("-"),
                r.action.as_deref().unwrap_or("-"),
                r.name.as_deref().unwrap_or("-")
            );
        }
    }
    out.print_message(&format!("\n{} firewall rules", rules.len()));
    Ok(())
}

pub async fn list_groups(
    client: &UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let groups = client.list_firewall_groups().await?;
    let rows: Vec<_> = groups
        .iter()
        .map(|g| {
            serde_json::json!({
                "id": g.id, "name": g.name, "type": g.group_type, "members": g.group_members,
            })
        })
        .collect();
    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&rows)?);
    } else {
        println!("{:<28} {:<16} Members", "Name", "Type");
        println!("{}", "-".repeat(78));
        for g in &groups {
            println!(
                "{:<28} {:<16} {}",
                g.name.as_deref().unwrap_or("-"),
                g.group_type.as_deref().unwrap_or("-"),
                g.group_members.join(", ")
            );
        }
    }
    out.print_message(&format!("\n{} firewall groups", groups.len()));
    Ok(())
}
