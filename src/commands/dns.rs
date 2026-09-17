use owo_colors::OwoColorize;

use crate::api::{
    DEFAULT_DNS_TTL_SECONDS, DnsRecordType, StaticDnsRecord, StaticDnsWrite, UnifiClient,
};
use crate::output::{OutputConfig, use_color};

pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
    pub fields: Option<Vec<String>>,
}

pub struct ListFilter {
    pub record_type: Option<DnsRecordType>,
    pub name: Option<String>,
}

pub struct CreateArgs {
    pub name: String,
    pub value: String,
    pub record_type: String,
    pub ttl: Option<u32>,
    pub priority: Option<u32>,
    pub weight: Option<u32>,
    pub port: Option<u32>,
    pub disabled: bool,
}

pub struct UpdateArgs {
    pub name: Option<String>,
    pub value: Option<String>,
    pub ttl: Option<u32>,
    pub priority: Option<u32>,
    pub weight: Option<u32>,
    pub port: Option<u32>,
    pub enabled: bool,
    pub disabled: bool,
}

pub fn write_from_create_args(args: CreateArgs) -> Result<StaticDnsWrite, String> {
    let record_type = DnsRecordType::parse(&args.record_type)?;
    let ttl = match args.ttl {
        Some(ttl) => Some(ttl),
        None if record_type.accepts_ttl() => Some(DEFAULT_DNS_TTL_SECONDS),
        None => None,
    };
    let write = StaticDnsWrite {
        name: args.name,
        record_type,
        value: args.value,
        ttl,
        enabled: !args.disabled,
        priority: args.priority,
        weight: args.weight,
        port: args.port,
    };
    write.validate()?;
    Ok(write)
}

pub fn write_from_update_args(
    existing: &StaticDnsRecord,
    args: UpdateArgs,
) -> Result<StaticDnsWrite, String> {
    if args.name.is_none()
        && args.value.is_none()
        && args.ttl.is_none()
        && args.priority.is_none()
        && args.weight.is_none()
        && args.port.is_none()
        && !args.enabled
        && !args.disabled
    {
        return Err(
            "nothing to change; pass --name, --value, --ttl, --priority, --weight, --port, --enabled, or --disabled"
                .into(),
        );
    }
    let mut write = StaticDnsWrite::from_record(existing);
    if let Some(name) = args.name {
        write.name = name;
    }
    if let Some(value) = args.value {
        write.value = value;
    }
    if let Some(ttl) = args.ttl {
        write.ttl = Some(ttl);
    }
    if let Some(priority) = args.priority {
        write.priority = Some(priority);
    }
    if let Some(weight) = args.weight {
        write.weight = Some(weight);
    }
    if let Some(port) = args.port {
        write.port = Some(port);
    }
    if args.enabled {
        write.enabled = true;
    } else if args.disabled {
        write.enabled = false;
    }
    write.validate()?;
    Ok(write)
}

pub async fn list(
    client: &mut UnifiClient,
    out: OutputConfig,
    filter: ListFilter,
    pagination: Pagination,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut records = client.list_static_dns().await?;
    if let Some(record_type) = filter.record_type {
        records.retain(|record| record.record_type == record_type);
    }
    if let Some(name) = filter.name {
        let needle = name.to_ascii_lowercase();
        records.retain(|record| record.name.to_ascii_lowercase().contains(&needle));
    }

    let total = records.len();
    let page: Vec<StaticDnsRecord> = records
        .into_iter()
        .skip(pagination.offset)
        .take(pagination.limit)
        .collect();

    if out.is_json() {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|record| {
                let mut obj = record.to_json();
                if let Some(ref keep) = pagination.fields {
                    let map = obj.as_object_mut().expect("row is a JSON object");
                    map.retain(|k, _| keep.iter().any(|f| f == k));
                }
                obj
            })
            .collect();
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "items": items,
            "total": total,
            "limit": pagination.limit,
            "offset": pagination.offset,
        }))?);
    } else {
        render_table(&page);
    }
    out.print_message(&format!("\n{total} DNS records"));
    Ok(())
}

pub async fn show(
    client: &mut UnifiClient,
    identifier: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let record = client.get_static_dns(identifier).await?;
    let row = record.to_json();
    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&row)?);
        return Ok(());
    }

    println!(
        "{}",
        if use_color() {
            format!("{}", record.name.bold())
        } else {
            record.name.clone()
        }
    );
    for (label, value) in [
        ("ID", record.id.clone()),
        ("Type", record.record_type.as_str().to_string()),
        ("Value", dash(&record.value)),
        (
            "TTL",
            record
                .ttl
                .map(|ttl| ttl.to_string())
                .unwrap_or_else(|| "-".into()),
        ),
        ("Enabled", if record.enabled { "yes" } else { "no" }.into()),
        (
            "Priority",
            record
                .priority
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
        ),
        (
            "Weight",
            record
                .weight
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
        ),
        (
            "Port",
            record
                .port
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
        ),
    ] {
        println!("  {label:<18} {value}");
    }
    Ok(())
}

pub async fn create(
    client: &mut UnifiClient,
    write: &StaticDnsWrite,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let record = client.create_static_dns(write).await?;
    print_mutation(&record, "create", &out);
    Ok(())
}

pub async fn update(
    client: &mut UnifiClient,
    identifier: &str,
    write: &StaticDnsWrite,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let record = client.update_static_dns(identifier, write).await?;
    print_mutation(&record, "update", &out);
    Ok(())
}

pub async fn delete(
    client: &mut UnifiClient,
    identifier: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let record = client.delete_static_dns(identifier).await?;
    print_mutation(&record, "delete", &out);
    Ok(())
}

fn print_mutation(record: &StaticDnsRecord, action: &str, out: &OutputConfig) {
    let mut result = record.to_json();
    result["status"] = serde_json::json!("ok");
    result["action"] = serde_json::json!(action);
    out.print_result(
        &result,
        &format!(
            "{} {} {} -> {}",
            match action {
                "create" => "Created",
                "update" => "Updated",
                _ => "Deleted",
            },
            record.record_type.as_str(),
            record.name,
            dash(&record.value)
        ),
    );
}

fn render_table(records: &[StaticDnsRecord]) {
    let header = format!(
        "{:<28} {:<6} {:<22} {:<8} {}",
        "Name", "Type", "Value", "TTL", "Enabled"
    );
    if use_color() {
        println!("{}", header.bold());
        println!("{}", "-".repeat(78).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(78));
    }
    for record in records {
        let ttl = record
            .ttl
            .map(|ttl| ttl.to_string())
            .unwrap_or_else(|| "-".into());
        let enabled = if record.enabled { "yes" } else { "no" };
        println!(
            "{:<28} {:<6} {:<22} {:<8} {}",
            truncate(&record.name, 28),
            record.record_type.as_str(),
            truncate(&dash(&record.value), 22),
            ttl,
            enabled
        );
    }
}

fn dash(value: &str) -> String {
    if value.is_empty() {
        "-".into()
    } else {
        value.to_string()
    }
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        value.to_string()
    } else if width <= 3 {
        value.chars().take(width).collect()
    } else {
        format!(
            "{}...",
            value
                .chars()
                .take(width.saturating_sub(3))
                .collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_args_default_a_record_ttl() {
        let write = write_from_create_args(CreateArgs {
            name: "nas.example.com".into(),
            value: "192.0.2.10".into(),
            record_type: "A".into(),
            ttl: None,
            priority: None,
            weight: None,
            port: None,
            disabled: false,
        })
        .unwrap();
        assert_eq!(write.ttl, Some(DEFAULT_DNS_TTL_SECONDS));
        assert!(write.enabled);
    }

    #[test]
    fn create_args_reject_a_record_with_ipv6() {
        let err = write_from_create_args(CreateArgs {
            name: "nas.example.com".into(),
            value: "2001:db8::1".into(),
            record_type: "A".into(),
            ttl: None,
            priority: None,
            weight: None,
            port: None,
            disabled: false,
        })
        .unwrap_err();
        assert!(err.contains("IPv4"), "{err}");
    }

    #[test]
    fn create_args_require_priority_for_mx() {
        let err = write_from_create_args(CreateArgs {
            name: "example.com".into(),
            value: "mail.example.com".into(),
            record_type: "MX".into(),
            ttl: None,
            priority: None,
            weight: None,
            port: None,
            disabled: false,
        })
        .unwrap_err();
        assert!(err.contains("--priority"), "{err}");
    }

    #[test]
    fn update_args_refuse_a_no_op() {
        let existing = StaticDnsRecord {
            id: "rec-1".into(),
            name: "nas.example.com".into(),
            record_type: DnsRecordType::A,
            value: "192.0.2.10".into(),
            ttl: Some(14400),
            enabled: true,
            priority: None,
            weight: None,
            port: None,
        };
        let err = write_from_update_args(
            &existing,
            UpdateArgs {
                name: None,
                value: None,
                ttl: None,
                priority: None,
                weight: None,
                port: None,
                enabled: false,
                disabled: false,
            },
        )
        .unwrap_err();
        assert!(err.contains("nothing to change"), "{err}");
    }
}
