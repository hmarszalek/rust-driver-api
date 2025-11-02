use anyhow::{Context, Result};
use rustls::{ClientConfig, RootCertStore};
use rustls_pemfile::certs;
use scylla::client::execution_profile::ExecutionProfile;
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use scylla::policies::load_balancing;
use scylla::policies::retry::DefaultRetryPolicy;
use scylla::statement::Consistency;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;

fn load_rustls_config() -> Result<Arc<ClientConfig>> {
    let mut root_store = RootCertStore::empty();

    // Load CA certificate
    let ca_file = File::open("certs/ca.crt")
        .context("Failed to open CA certificate file. Make sure certs/ca.crt exists. Run ./generate-certs.sh if needed.")?;
    let mut ca_reader = BufReader::new(ca_file);

    // Read certificates from PEM file
    let mut cert_count = 0;
    for cert in certs(&mut ca_reader) {
        let cert = cert.context("Failed to parse certificate from ca.crt")?;
        root_store
            .add(cert)
            .context("Failed to add certificate to root store")?;
        cert_count += 1;
    }

    if cert_count == 0 {
        anyhow::bail!("No certificates found in certs/ca.crt");
    }

    println!(
        "✓ Loaded {} CA certificate(s) from certs/ca.crt",
        cert_count
    );

    // Build TLS config
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(Arc::new(config))
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Starting Rust ScyllaDB Application ===\n");

    // Build rustls TLS context
    println!("Loading TLS configuration...");
    let tls_config = load_rustls_config()?;

    println!("Building execution profile...");
    let execution_profile = ExecutionProfile::builder()
        .consistency(Consistency::LocalQuorum)
        .request_timeout(Some(Duration::from_secs(10)))
        .load_balancing_policy(Arc::new(load_balancing::DefaultPolicy::default()))
        .retry_policy(Arc::new(DefaultRetryPolicy::default()))
        .build();

    let handle = execution_profile.into_handle();

    // Connect to ScyllaDB cluster with TLS using rustls
    // Scylla listens on port 9042 with TLS enabled (not 9142)
    println!("Connecting to ScyllaDB cluster with TLS...");
    println!("  - Node 1: 127.0.0.2:9042 (TLS)");
    println!("  - Node 2: 127.0.0.3:9042 (TLS)");
    println!("  - Node 3: 127.0.0.4:9042 (TLS)");

    let session: Session = SessionBuilder::new()
        .known_node("127.0.0.2:9042") // TLS-enabled port for scylla1
        .known_node("127.0.0.3:9042") // TLS-enabled port for scylla2
        .known_node("127.0.0.4:9042") // TLS-enabled port for scylla3
        .tls_context(Some(tls_config)) // Arc<ClientConfig> auto-converts to TlsContext
        .default_execution_profile_handle(handle.clone())
        .build()
        .await
        .context("Failed to connect to ScyllaDB cluster. Ensure:\n  1. Docker containers are running (docker-compose ps)\n  2. Certificates are generated (./generate-certs.sh)\n  3. All nodes are healthy (docker exec rust-application-scylla1-1 nodetool status)")?;

    println!("✓ Connected to ScyllaDB cluster with TLS\n");

    // Create keyspace and table
    println!("Setting up database schema...");

    session
        .query_unpaged(
            "CREATE KEYSPACE IF NOT EXISTS cat_ks WITH REPLICATION = {'class' : 'NetworkTopologyStrategy', 'replication_factor' : 2}",
            &[],
        )
        .await
        .context("Failed to create keyspace")?;

    Ok(())
}
