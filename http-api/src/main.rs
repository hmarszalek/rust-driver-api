use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use scylla::statement::prepared::PreparedStatement;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use rustls::{ClientConfig, RootCertStore};
use rustls::pki_types::{CertificateDer, pem::PemObject};
use futures::{TryStreamExt};
use std::io::BufReader;
use std::fs::File;
use rustls_pemfile::certs;
use anyhow::{Context};

// Holds a shared Session inside an Arc so the HTTP service 
// can clone this state cheaply between requests.
#[derive(Clone)]
struct AppState {
    session: Arc<Session>,
    // Cached prepared insert statement for reuse across requests.
    prepared_insert: PreparedStatement,
}

// Item is the JSON request/response model for inserts
// JSON serialization/deserialization is derived through Serde; 
// the code expects request bodies to be valid JSON matching this shape.
#[derive(Debug, Serialize, Deserialize)]
struct Item {
    id: uuid::Uuid,
    name: String,
    value: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct InsertResponse {
    success: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build rustls TLS context
    println!("Loading TLS configuration...");

    // Build a SessionBuilder so we can optionally configure TLS.
    let mut builder = SessionBuilder::new()
        // .known_node(uri.clone());        
        .known_node("127.0.0.2:9042")
        .known_node("127.0.0.3:9042")
        .known_node("127.0.0.4:9042");

    println!("Connecting to ScyllaDB");

    // If SCYLLA_USE_TLS=1 enable TLS with rustls.
    // For now TLS connection doesnt work.
    // if std::env::var("SCYLLA_USE_TLS").unwrap_or_default() == "1" {
        println!("Loading TLS root certificates from SCYLLA_TLS_CA...");

        let mut root_store = RootCertStore::empty();

        // Load CA certificate
        let ca_file = File::open("certs/ca.crt")?;
        let mut ca_reader = BufReader::new(ca_file);

        // Read certificates from PEM file
        let mut cert_count = 0;
        for cert in certs(&mut ca_reader) {
            let cert = cert.context("Failed to parse certificate from ca.crt")?;
            root_store.add(cert)?;
            cert_count += 1;
        }

        if cert_count == 0 {
            anyhow::bail!("No certificates found in certs/ca.crt");
        }


        // let rustls_ca = CertificateDer::from_pem_file("certs/ca.crt")?;
        // root_store.add(rustls_ca)?;
        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        builder = builder.tls_context(Some(std::sync::Arc::new(config)));
    // }

    let session: Session = builder.build().await?;

    // Ensure demo keyspace and table exist.
    let ks_cql = "CREATE KEYSPACE IF NOT EXISTS demo WITH replication = {'class': 'SimpleStrategy', 'replication_factor': 1}";
    session.query_unpaged(ks_cql, ()).await?;
    
    let tbl_cql = "CREATE TABLE IF NOT EXISTS demo.items (id uuid PRIMARY KEY, name text, value bigint);";
    session.query_unpaged(tbl_cql, ()).await?;

    // Prepare frequently used statements once and cache them in AppState.
    let prepared_insert = session
        .prepare("INSERT INTO demo.items (id, name, value) VALUES (?, ?, ?)")
        .await?;

    // Wrap the session and prepared statement in an Arc and store in AppState
    let state = Arc::new(AppState { session: Arc::new(session), prepared_insert });

    let make_svc = make_service_fn(move |_conn| {
        let state = state.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let s = state.clone();
                handle(req, s)
            }))
        }
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on http://{}", addr);
    Server::bind(&addr).serve(make_svc).await?;
    Ok(())
}

// The handle function matches (method, path) and handles endpoints. 
// It returns a Result<Response<Body>, Infallible> 
// (so errors are turned into HTTP responses rather than panics).
// https://doc.rust-lang.org/std/convert/enum.Infallible.html
async fn handle(req: Request<Body>, state: Arc<AppState>) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path().to_string();
    match (req.method(), path.as_str()) {
        (&Method::POST, "/insert") => {
            let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
            match serde_json::from_slice::<Item>(&whole) {
                Ok(item) => {
                    let cql = "INSERT INTO demo.items (id, name, value) VALUES (?, ?, ?)";
                    let _ = state.session.query_unpaged(cql, (item.id, item.name, item.value)).await;
                    let body = serde_json::to_string(&InsertResponse { success: true }).unwrap();
                    Ok(Response::new(Body::from(body)))
                }
                Err(e) => {
                    let mut resp = Response::new(Body::from(format!("invalid json: {}", e)));
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    Ok(resp)
                }
            }
        }
        (&Method::POST, "/insert_batch") => {
            let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
            match serde_json::from_slice::<Vec<Item>>(&whole) {
                Ok(items) => {
                    use scylla::statement::batch::Batch;
                    let mut batch = Batch::new(scylla::statement::batch::BatchType::Logged);

                    // For each item, append the prepared statement (cached in AppState) to the batch
                    // and collect its bound values in a Vec so we can pass them as BatchValues.
                    let mut values_vec: Vec<(uuid::Uuid, String, i64)> = Vec::with_capacity(items.len());
                    for item in items {
                        batch.append_statement(state.prepared_insert.clone());
                        values_vec.push((item.id, item.name, item.value));
                    }

                    // Execute the typed batch. The Vec<T> where T: SerializeRow implements BatchValues.
                    match state.session.batch(&batch, values_vec).await {
                        Ok(_) => {
                            let body = serde_json::to_string(&InsertResponse { success: true }).unwrap();
                            Ok(Response::new(Body::from(body)))
                        }
                        Err(e) => {
                            let mut resp = Response::new(Body::from(format!("batch error: {}", e)));
                            *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            Ok(resp)
                        }
                    }
                }
                Err(e) => {
                    let mut resp = Response::new(Body::from(format!("invalid json: {}", e)));
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    Ok(resp)
                }
            }
        }
        (&Method::POST, "/insert_prepared") => {
            let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
            match serde_json::from_slice::<Item>(&whole) {
                Ok(item) => {
                    // Use cached prepared statement from AppState (prepared at startup)
                    let prep = state.prepared_insert.clone();
                    let res = state.session.execute_unpaged(&prep, (item.id, item.name, item.value)).await;
                    match res {
                        Ok(_) => {
                            let body = serde_json::to_string(&InsertResponse { success: true }).unwrap();
                            Ok(Response::new(Body::from(body)))
                        }
                        Err(e) => {
                            let mut resp = Response::new(Body::from(format!("db error: {}", e)));
                            *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            Ok(resp)
                        }
                    }
                }
                Err(e) => {
                    let mut resp = Response::new(Body::from(format!("invalid json: {}", e)));
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    Ok(resp)
                }
            }
        }
        (&Method::GET, "/query_iter") => {
            // Run query_iter which returns a pager/iterator that streams rows
            match state.session.query_iter("SELECT id, name, value FROM demo.items", ()).await {
                Ok(pager) => {
                    match pager.rows_stream::<(uuid::Uuid, String, i64)>() {
                        Ok(mut rows_stream) => {
                            let mut out = Vec::new();
                            // rows_stream is a TryStream of Result<Row, Error>
                            while let Some(row_res) = rows_stream.try_next().await.unwrap_or(None) {
                                let (id, name, value) = row_res;
                                out.push(serde_json::json!({"id": id.to_string(), "name": name, "value": value}));
                            }
                            let body = serde_json::to_string(&serde_json::json!({"rows": out})).unwrap_or_else(|_| "{}".to_string());
                            Ok(Response::new(Body::from(body)))
                        }
                        Err(e) => {
                            let mut resp = Response::new(Body::from(format!("failed to get rows_stream: {}", e)));
                            *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            Ok(resp)
                        }
                    }
                }
                Err(e) => {
                    let mut resp = Response::new(Body::from(format!("query_iter error: {}", e)));
                    *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    Ok(resp)
                }
            }
        }
        _ => {
            let mut not_found = Response::new(Body::from("Not Found"));
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}
