# LDK-LSP

A flexible Lightning Service Provider (LSP) built on top of [LDK Server](https://github.com/lightningdevkit/ldk-server).

## Features

- **Zero-confirmation (zeroconf) channel opens**: Allow clients to receive payments immediately without waiting for on-chain confirmations
- **Channel splicing**: Increase or decrease channel capacity without closing and reopening channels
- **HTTP API**: RESTful API for channel purchases and management
- **Webhook support**: Payment notifications for automated channel opening

## Architecture

```
┌─────────────┐     HTTP API      ┌──────────────┐
│   Client    │ ◄───────────────► │  LDK-LSP     │
│  Services   │                   │  (this repo) │
└─────────────┘                   └──────┬───────┘
                                         │
                                         │ gRPC
                                         │
                              ┌──────────▼──────────┐
                              │    LDK Server       │
                              │  (Lightning Node)   │
                              └─────────────────────┘
```

LDK-LSP acts as a layer on top of LDK Server:
1. Uses `ldk-server-client` to communicate with the Lightning node via gRPC
2. Provides an HTTP API for external services to purchase channels
3. Manages channel lifecycle including zeroconf handling and splicing

## Prerequisites

- Rust 1.70+ 
- LDK Server instance running (see [LDK Server](https://github.com/lightningdevkit/ldk-server))
- Bitcoin node (bitcoind) - managed by LDK Server

## Quick Start

### 1. Clone and Build

```bash
git clone https://github.com/yourusername/ldk-lsp.git
cd ldk-lsp
cargo build --release
```

### 2. Configure

Create a configuration file at `./ldk-lsp.toml`:

```toml
[node]
alias = "My LSP"
network = "regtest"  # or "testnet", "mainnet"
data_dir = "./data"

[ldk_server]
host = "127.0.0.1"
port = 3009  # Default ldk-server port

[lsp]
channel_open_base_fee = 10000        # 10k sats base fee
channel_open_fee_ppm = 10000         # 1% fee
min_channel_size = 100000            # 100k sats minimum
max_channel_size = 100000000         # 1 BTC maximum
enable_zeroconf = true
enable_splicing = true

[api]
bind_address = "127.0.0.1:8080"
enable_cors = true

[database]
url = "sqlite:ldk-lsp.db"
```

### 3. Run

```bash
./target/release/ldk-lsp
```

## API Documentation

### Health Check

```bash
GET /health
```

Response:
```json
{
  "success": true,
  "data": {
    "status": "healthy",
    "version": "0.1.0",
    "ldk_server_connected": true
  }
}
```

### Request Channel Quote

```bash
POST /v1/channels/quote
Content-Type: application/json

{
  "node_id": "...",
  "host": "...",
  "port": 9735,
  "capacity": 1000000,
  "require_zeroconf": true
}
```

Response:
```json
{
  "success": true,
  "data": {
    "request_id": "...",
    "capacity": 1000000,
    "fee": 20000,
    "total_cost": 1020000,
    "lsp_node_id": "...",
    "expiry": "2024-01-01T12:00:00Z"
  }
}
```

### Confirm Channel (after payment)

```bash
POST /v1/channels/{request_id}/confirm
```

### Request Splice Quote

```bash
POST /v1/splice/quote
Content-Type: application/json

{
  "channel_id": "...",
  "additional_capacity": 500000
}
```

## Development

### Running Tests

```bash
cargo test
```

### Building Documentation

```bash
cargo doc --no-deps --open
```

## Project Structure

```
src/
├── main.rs           # Application entry point
├── lib.rs            # Library root
├── config.rs         # Configuration management
├── node/             # LDK Server integration
│   ├── mod.rs        # Node wrapper
│   ├── client.rs     # gRPC client
│   └── events.rs     # Event handling
├── lsp/              # LSP core functionality
│   ├── mod.rs        # Main LSP service
│   ├── zeroconf.rs   # Zeroconf channel handling
│   └── splicing.rs   # Channel splicing
├── api/              # HTTP API
│   ├── mod.rs        # API server setup
│   ├── channels.rs   # Channel endpoints
│   ├── splicing.rs   # Splicing endpoints
│   ├── payments.rs   # Payment webhooks
│   └── health.rs     # Health checks
└── db/               # Database
    ├── mod.rs        # Database connection
    ├── models.rs     # Data models
    └── queries.rs    # Database queries
```

## Roadmap

- [x] Basic project structure
- [x] LDK Server integration (gRPC client)
- [x] Configuration system
- [x] Zeroconf channel support
- [x] Channel splicing support
- [x] HTTP API for channel requests
- [ ] Admin dashboard (web UI)
- [ ] Advanced fee scheduling
- [ ] Liquidity management tools
- [ ] Metrics and monitoring

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) for details.

## License

This project is licensed under the MIT OR Apache-2.0 license.
