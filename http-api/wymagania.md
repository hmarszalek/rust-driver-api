This will allow you to practice Rust and learn the API of Rust Driver (which you'll need to use a lot).
The goal is to learn Rust & Rust Driver, not to write something useful, so the app is intentionally artificial, and the description a bit vague.
We want you to use the following driver functionalities (or more!):
- Prepared Statements
- execute_iter / query_iter
- Borrowed deserialization
- Manual paging (execute_single_page / query_single_page).
- Iteration over token ranges (for vnodes)
- Manually implementing SerializeValue / DeserializeValue
- Request history
- Alternative load balancing policies
- Request configuration, execution profiles
- Connecting to cluster with TLS, using Rustls backend
- Reading schema metadata
- Batches


Please write a HTTP API (using some framework like Axum), which is a CRUD operating on 1-2 Scylla tables.
We envision the following endpoints:
Inserting data passed in the request, either single entity or multiple (Prepared statements, batches).
Reading whole table in chunks, performing some aggregation (token range iteration, execute_iter)
Read some part of data using manual paging (i.e., the next page of a paged query, continuing from some user-provided paging state), perform some transformation / aggregation and display it to the user. Use borrowed deserialization (i.e., use &str instead of String, etc., to save allocations).
Get metadata of the provided table, display it to the user.

General guidelines:
- Endpoints should accept a "debug" parameter, which will cause them to collect Request History for all request, and print it to the terminal (or return to the user)
- Endpoints (apart from token-range-scanning one) should accept an optional "node" parameter, which will allow the user to select one node to which requests will be sent. Use SingleTargetLoadBalancingPolicy.
- All requests should utilize Retry Policy, set some consistency level, and set timeouts. Configuration should be done using an execution profile.
- Some column in the database should be represented in code using a custom type (you should create them yourself - some struct will do), for which SerializeValue / DeserializeValue is not implemented by default. You should implement it manually.
- The app should connect to the cluster using TLS with Rustls.