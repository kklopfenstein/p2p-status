## p2p-status

A simple p2p network written in Rust using [rust-libp2p](https://github.com/libp2p/rust-libp2p) and [hyper](https://github.com/hyperium/hyper) as a frontend.

Usage:

```
cargo run -- --description <HOST DESCRIPTION> --admin-port <ADMIN SERVER HTTP PORT>
```

Note that `--admin-port` will default to `3000` if not specified.


Once the server is running visit `http://localhost:3000` to view the admin interface.