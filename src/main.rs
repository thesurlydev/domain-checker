// src/main.rs
use clap::Parser;
use futures::stream::{self, StreamExt};
use serde::Serialize;
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
    /// Domain names to check
    #[arg(required = true)]
    domains: Vec<String>,

    /// Maximum number of concurrent checks
    #[arg(short, long, default_value = "10")]
    concurrent: usize,

    /// Output as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct DomainStatus {
    domain: String,
    registered: bool,
    has_dns: bool,
    has_ip: bool,
    nameservers: Vec<String>,
    ip_addresses: Vec<String>,
    error: Option<String>,
}

struct DomainChecker {
    resolver: TokioAsyncResolver,
}

impl DomainChecker {
    async fn new() -> Self {
        let mut opts = ResolverOpts::default();
        opts.timeout = Duration::from_secs(2);
        opts.attempts = 2;

        let resolver = TokioAsyncResolver::tokio(ResolverConfig::cloudflare(), opts);

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
                status.nameservers = ns_records.iter().map(|record| record.to_string()).collect();
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
                status.ip_addresses = ips.iter().map(|ip| ip.to_string()).collect();
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

    async fn check_domains(
        &self,
        domains: Vec<String>,
        concurrent_limit: usize,
    ) -> Vec<DomainStatus> {
        stream::iter(domains)
            .map(|domain| self.check_domain(domain))
            .buffer_unordered(concurrent_limit)
            .collect()
            .await
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let checker = DomainChecker::new().await;

    let results = checker.check_domains(cli.domains, cli.concurrent).await;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        for status in results {
            println!("\nDomain: {}", status.domain);
            println!("Registered: {}", status.registered);

            if !status.nameservers.is_empty() {
                println!("Nameservers:");
                for ns in status.nameservers {
                    println!("  - {}", ns);
                }
            }

            if !status.ip_addresses.is_empty() {
                println!("IP Addresses:");
                for ip in status.ip_addresses {
                    println!("  - {}", ip);
                }
            }

            if let Some(error) = status.error {
                println!("Error: {}", error);
            }
        }
    }

    Ok(())
}
