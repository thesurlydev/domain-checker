use chrono::Utc;
use clap::Parser;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::time::Duration;
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::TokioAsyncResolver;

#[derive(Parser, Debug)]
#[command(
    name = "domain-checker",
    about = "Check if domain names are registered using DNS lookups",
    version
)]
struct Cli {
    /// Domain names to check (optional if reading from stdin)
    #[arg(required = false)]
    domains: Vec<String>,

    /// Maximum number of concurrent checks
    #[arg(short, long, default_value = "10")]
    concurrent: usize,

    /// Output as JSON to stdout
    #[arg(short, long)]
    json: bool,

    /// Save output to JSON file
    #[arg(long)]
    output_file: Option<PathBuf>,

    /// Strip whitespace and empty lines from input
    #[arg(long)]
    clean: bool,

    /// Show only unregistered domains in output
    #[arg(short = 'u', long)]
    unregistered_only: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DomainStatus {
    domain: String,
    registered: bool,
    has_dns: bool,
    has_ip: bool,
    nameservers: Vec<String>,
    ip_addresses: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CheckResult {
    timestamp: String,
    check_count: usize,
    domains: Vec<DomainStatus>,
    summary: ResultSummary,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResultSummary {
    total_checked: usize,
    registered: usize,
    unregistered: usize,
    errors: usize,
}

struct DomainChecker {
    resolver: TokioAsyncResolver,
}

impl DomainChecker {
    async fn new() -> Self {
        let mut opts = ResolverOpts::default();
        opts.timeout = Duration::from_secs(2);
        opts.attempts = 2;

        let resolver = TokioAsyncResolver::tokio(
            ResolverConfig::cloudflare(),
            opts,
        );

        Self { resolver }
    }

    async fn check_domain(&self, domain: String) -> DomainStatus {
        let mut status = DomainStatus {
            domain,
            registered: false,
            has_dns: false,
            has_ip: false,
            nameservers: Vec::new(),
            ip_addresses: Vec::new(),
            error: None,
        };

        // Check NS records
        match self.resolver.ns_lookup(status.domain.clone()).await {
            Ok(ns_records) => {
                status.has_dns = true;
                status.registered = true;
                status.nameservers = ns_records
                    .iter()
                    .map(|record| record.to_string())
                    .collect();
            }
            Err(e) => match e.kind() {
                trust_dns_resolver::error::ResolveErrorKind::NoRecordsFound { .. } => {}
                _ => {
                    if !status.registered {
                        status.error = Some(format!("NS lookup error: {}", e));
                    }
                }
            },
        }

        // Check A records
        match self.resolver.lookup_ip(status.domain.clone()).await {
            Ok(ips) => {
                status.has_ip = true;
                status.registered = true;
                status.ip_addresses = ips
                    .iter()
                    .map(|ip| ip.to_string())
                    .collect();
            }
            Err(e) => {
                if !status.registered {
                    match e.kind() {
                        trust_dns_resolver::error::ResolveErrorKind::NoRecordsFound { .. } => {}
                        _ => {
                            status.error = Some(format!("IP lookup error: {}", e));
                        }
                    }
                }
            }
        }

        status
    }

    async fn check_domains(&self, domains: Vec<String>, concurrent_limit: usize) -> Vec<DomainStatus> {
        stream::iter(domains)
            .map(|domain| self.check_domain(domain))
            .buffer_unordered(concurrent_limit)
            .collect()
            .await
    }
}

fn create_check_result(domains: Vec<DomainStatus>, timestamp: String) -> CheckResult {
    let total_checked = domains.len();
    let registered = domains.iter().filter(|d| d.registered).count();
    let unregistered = domains.iter().filter(|d| !d.registered).count();
    let errors = domains.iter().filter(|d| d.error.is_some()).count();

    CheckResult {
        timestamp,
        check_count: total_checked,
        domains,
        summary: ResultSummary {
            total_checked,
            registered,
            unregistered,
            errors,
        },
    }
}

fn filter_results(result: CheckResult, unregistered_only: bool) -> CheckResult {
    if !unregistered_only {
        return result;
    }

    // Keep all original summary counts
    let ResultSummary {
        total_checked,
        registered,
        unregistered,
        errors: _,  // We'll recalculate errors for filtered domains
    } = result.summary;

    let filtered_domains: Vec<DomainStatus> = result.domains
        .into_iter()
        .filter(|d| !d.registered)
        .collect();

    // Only update errors count for the filtered domains
    let errors = filtered_domains.iter().filter(|d| d.error.is_some()).count();

    CheckResult {
        timestamp: result.timestamp,
        check_count: total_checked,
        domains: filtered_domains,
        summary: ResultSummary {
            total_checked,
            registered,
            unregistered,
            errors,
        },
    }
}

fn print_text_output(result: &CheckResult) {
    
    println!("\nTimestamp: {}", result.timestamp);    
    println!("\nSummary:");
    println!("  Total Checked: {}", result.summary.total_checked);
    println!("  Registered: {}", result.summary.registered);
    println!("  Unregistered: {}", result.summary.unregistered);
    println!("  Errors: {}", result.summary.errors);

    println!("\nDetailed Results:");
    for status in &result.domains {
        println!("\nDomain: {}", status.domain);
        println!("Registered: {}", status.registered);

        if !status.nameservers.is_empty() {
            println!("Nameservers:");
            for ns in &status.nameservers {
                println!("  - {}", ns);
            }
        }

        if !status.ip_addresses.is_empty() {
            println!("IP Addresses:");
            for ip in &status.ip_addresses {
                println!("  - {}", ip);
            }
        }

        if let Some(error) = &status.error {
            println!("Error: {}", error);
        }
    }
}

fn read_domains_from_stdin(clean: bool) -> io::Result<Vec<String>> {
    let stdin = io::stdin();
    let mut domains = Vec::new();

    for line in stdin.lock().lines() {
        let line = line?;
        if clean {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                domains.push(trimmed.to_string());
            }
        } else {
            domains.push(line);
        }
    }

    Ok(domains)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let checker = DomainChecker::new().await;

    // Get domains from either command line args or stdin
    let domains = if cli.domains.is_empty() {
        // No domains provided as arguments, try reading from stdin
        read_domains_from_stdin(cli.clean)?
    } else {
        cli.domains
    };

    // Verify we have domains to check
    if domains.is_empty() {
        eprintln!("Error: No domains provided. Either specify domains as arguments or pipe them through stdin.");
        std::process::exit(1);
    }

    let results = checker
        .check_domains(domains, cli.concurrent)
        .await;

    let timestamp = Utc::now().to_rfc3339();

    let check_result = create_check_result(results, timestamp);
    let filtered_result = filter_results(check_result, cli.unregistered_only);

    // Handle output based on flags
    if cli.json || cli.output_file.is_some() {
        let json = serde_json::to_string_pretty(&filtered_result)?;

        if cli.json {
            println!("{}", json);
        }

        if let Some(path) = cli.output_file {
            fs::write(path, json)?;
        }
    } else {
        print_text_output(&filtered_result);
    }

    Ok(())
}