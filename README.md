# Highlights of the project:

- http parser written with nom: src/parse.rs
- http server using mio in src/server.rs
- router based on radix trie (path-tree) in src/router.rs
- chat service in src/chat_service.rs
- serialzation with serde/serde_json in src/messages.rs

This sample project should run on rust 1.37+ stable.

Build and run with:

```
cargo run 
```

Which will launch and bind to 127.0.0.1:80 by default.

To bind to another interface/port:

```
cargo run 0.0.0.0:8080
```

Run test suite:

```
cargo test
```


## Next steps / Other improvements:

- Larger requests (> 8192 bytes, or chunked) would need a more sophistocated polling model.
- Multithreading and async/await support
    - Currently the server is single-threaded, but handles requests when readiness events trickle in from mio

