Build and run
```
sudo docker compose -f docker-compose.yml up -d --wait
cargo run
```

Manual creation of keyspace and table (curently in main)
```
cqlsh 127.0.0.1 9042 -e "CREATE KEYSPACE IF NOT EXISTS demo WITH replication = {'class':'SimpleStrategy','replication_factor':1};"
cqlsh 127.0.0.1 9042 -e "CREATE TABLE IF NOT EXISTS demo.items (id uuid PRIMARY KEY, name text, value bigint);"
```

POST /insert
Purpose: simple, ad-hoc INSERT (non-prepared).
```
curl -X POST http://127.0.0.1:3000/insert \
  -H "Content-Type: application/json" \
  -d '{"id":"550e8400-e29b-41d4-a716-446655440000","name":"alice","value":123}'
```

POST /insert_prepared
Purpose: prepares the INSERT and executes it (current implementation prepares per request).
```
curl -X POST http://127.0.0.1:3000/insert_prepared \
  -H "Content-Type: application/json" \
  -d '{"id":"550e8400-e29b-41d4-a716-446655440005","name":"felix","value":321}'
```

POST /insert_batch
Purpose: accepts an array of items and runs concurrent individual inserts (not a server-side Batch).
```
curl -X POST http://127.0.0.1:3000/insert_batch \
  -H "Content-Type: application/json" \
  -d '[{"id":"550e8400-e29b-41d4-a716-446655440001","name":"b","value":1},{"id":"550e8400-e29b-41d4-a716-446655440002","name":"c","value":2}]'
```
```
curl -X POST http://127.0.0.1:3000/insert_batch \
  -H "Content-Type: application/json" \
  -d '[{"id":"550e8400-e29b-41d4-a716-446655440003","name":"d","value":3},{"id":"550e8400-e29b-41d4-a716-446655440004","name":"e","value":4}]'
```

GET /query_iter
```
GET 'http://127.0.0.1:3000/query_iter'
```

POST /custom_insert and GET /custom_query
```
curl -X POST http://127.0.0.1:3000/custom_insert \
  -H "Content-Type: application/json" \
  -d '{"text":"hello advanced world"}'

curl http://127.0.0.1:3000/custom_query
```

```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    // Insert custom value
    let resp = client.post("http://127.0.0.1:3000/custom_insert")
        .json(&serde_json::json!({"text": "from rust client"}))
        .send()
        .await?;
    println!("Insert response: {:?}", resp.text().await?);

    // Query all custom values
    let resp = client.get("http://127.0.0.1:3000/custom_query")
        .send()
        .await?;
    let body = resp.text().await?;
    println!("Query response: {}", body);

    Ok(())
}
```

```
curl -X POST http://127.0.0.1:3000/custom_query_paged \
  -H "Content-Type: application/json" \
  -d '{"page_size": 1}'
```
```
curl -X POST http://127.0.0.1:3000/custom_query_paged \
  -H "Content-Type: application/json" \
  -d '{"page_size": 1, "paging_state": "base64string..."}'
```
```
curl http://127.0.0.1:3000/custom_query_paged_all
```

```
curl -X POST http://localhost:3000/custom_query_token_range \
  -H "Content-Type: application/json" \
  -d '{"start_token": -9223372036854775808, "end_token": 0, "page_size": 10}'
```

Direct data inspection with sqlsh
cqlsh 127.0.0.1 9042 -e "SELECT * FROM demo.items;"