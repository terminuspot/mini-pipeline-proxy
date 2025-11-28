mini_pipeline_proxy
======================

This is a complete minimal proxy framework implementing Steps 1-4 from the tutorial.
Features included:
- Async TCP server (tokio)
- Protocol abstraction (traits)
- SOCKS5 inbound handler (no-auth CONNECT only)
- Pipeline: middleware -> router -> outbound manager
- OutboundManager returning Box<dyn Connection> (TcpConnection implemented)
- Simple router and logging middleware

Build & run:
```
cargo run --release
```

Test with curl (after running):
```
curl -x socks5h://127.0.0.1:1080 http://example.com
```

Note: This is a learning/prototype skeleton. Production code needs timeouts, error handling, connection limits, TLS/SS/VMess implementations, and more.
