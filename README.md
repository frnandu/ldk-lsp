# LDK-LSP

A flexible Lightning Service Provider (LSP) built on top of [LDK Server](https://github.com/lightningdevkit/ldk-server). Provides Just-In-Time (JIT) liquidity for users lacking inbound channel capacity.

## Features

- **JIT Receive (Just-In-Time Liquidity)**: Users without inbound capacity can request a quote, pay an invoice, and automatically receive either:
  - A new channel opened to them (with zero-conf)
  - A splice-in to an existing channel
- **Dynamic Fee Estimation**: Real-time onchain fee rates fetched from your configured Esplora instance
- **Inbound Buffer**: Channels are opened with extra capacity beyond what the user paid for, giving them spare inbound liquidity
- **Zero-confirmation (zeroconf)**: New channels are usable immediately
- **HTTP API**: RESTful API for JIT receive requests
- **RabbitMQ Events**: Async payment notifications trigger automatic channel opens/splices

## Architecture

```
┌─────────────┐     HTTP API      ┌──────────────┐
│   Client    │ ◄───────────────► │  LDK-LSP     │
│  (Alby Hub) │                   │  (this repo) │
└─────────────┘                   └──────┬───────┘
                                          │
                                          │ gRPC
                                          │
                               ┌──────────▼──────────┐
                               │    LDK Server       │
                               │  (Lightning Node)   │
                               └─────────────────────┘
                                          │
                                          │ Esplora
                                          │
                               ┌──────────▼──────────┐
                               │   Esplora Server    │
                               │ (Fee estimates +    │
                               │  Blockchain data)   │
                               └─────────────────────┘
```

LDK-LSP acts as a layer on top of LDK Server:
1. Uses `ldk-server-client` to communicate with the Lightning node via gRPC
2. Provides an HTTP API for external services to request JIT liquidity
3. Manages channel lifecycle including automatic zeroconf handling
4. Consumes RabbitMQ events from ldk-server for real-time payment notifications

## Prerequisites

- Rust 1.70+ 
- LDK Server instance running (see [LDK Server](https://github.com/lightningdevkit/ldk-server))
- Esplora instance (for fee estimation and blockchain data)
- RabbitMQ (for event notifications)

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

[ldk_server.rabbitmq]
connection_string = "amqp://guest:guest@localhost:5672/%2F"
exchange_name = "ldk-server"

[lsp]
# JIT Receive settings
jit_receive_base_fee = 1000          # Base fee in sats
jit_receive_fee_ppm = 5000           # 0.5% fee (parts per million)
jit_min_receive_amount = 10000       # Minimum 10k sats
jit_inbound_buffer = 50000           # Extra 50k sats capacity added

# Fee estimation (uses same Esplora as ldk-server)
fee_confirmation_target = 6          # Target ~1 hour confirmation

# Channel limits
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

## How JIT Receive Works

1. **User requests quote**: Client provides their node ID, host, port, and amount they want to receive
2. **LSP detects channel status**: 
   - If no channel exists → will open new channel
   - If channel exists → will splice additional capacity
3. **Fee calculation**: 
   - Base fee + PPM fee on amount
   - Dynamic onchain fee estimate (from Esplora)
   - Total invoice amount = amount + all fees
4. **Invoice payment**: User pays the BOLT11 invoice
5. **Automatic execution**: When payment is detected via RabbitMQ, LSP automatically:
   - Opens a new channel (with zeroconf), or
   - Splices in additional funds to existing channel
6. **Channel capacity**: User receives `amount + jit_inbound_buffer` in channel capacity

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

### Request JIT Receive Quote

Request inbound liquidity to receive payments.

```bash
POST /v1/receive/quote
Content-Type: application/json

{
  "node_id": "0288f...",
  "host": "192.168.1.100",
  "port": 9735,
  "amount": 50000
}
```

Response:
```json
{
  "success": true,
  "data": {
    "receive_id": "550e8400-e29b-41d4-a716-446655440000",
    "amount": 50000,
    "fees": {
      "base": 1000,
      "ppm": 250,
      "onchain": 4800,
      "total": 6050,
      "fee_rate": 24
    },
    "total_invoice_amount": 56050,
    "is_splice": false,
    "channel_id": null,
    "lsp_node_id": "My LSP",
    "expiry": "2024-01-15T10:30:00Z",
    "invoice": "lnbc5605010n1p3...",
    "payment_hash": "abcd1234..."
  }
}
```

**Notes:**
- `is_splice`: `true` if splicing existing channel, `false` if opening new channel
- `fee_rate`: The sat/vbyte rate used for onchain fee calculation
- `total_invoice_amount`: What user pays (amount + fees)
- Channel will have `amount + jit_inbound_buffer` capacity

### Check Receive Request Status

```bash
GET /v1/receive/{receive_id}
```

Response:
```json
{
  "success": true,
  "data": {
    "receive_id": "550e8400-e29b-41d4-a716-446655440000",
    "status": "PendingPayment",
    "amount": 50000,
    "is_splice": false,
    "channel_id": null,
    "fees": {
      "base": 1000,
      "ppm": 250,
      "onchain": 4800,
      "total": 6050,
      "fee_rate": 24
    },
    "total_invoice_amount": 56050,
    "created_at": "2024-01-15T10:20:00Z",
    "failure_reason": null
  }
}
```

**Status values:**
- `PendingPayment`: Waiting for invoice to be paid
- `PaymentReceived`: Payment detected, action starting
- `ChannelOpening`: New channel opening initiated
- `SpliceInitiated`: Splice-in initiated
- `Completed`: Channel/splice ready for use
- `Failed`: Something went wrong (see `failure_reason`)

### Request Splice Quote (Legacy)

For manual splicing (not through JIT receive flow):

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
├── fee.rs            # Dynamic fee estimation from Esplora
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
│   ├── receive.rs    # JIT receive endpoints
│   ├── splicing.rs   # Splicing endpoints
│   └── health.rs     # Health checks
├── db/               # Database
│   ├── mod.rs        # Database connection
│   ├── models.rs     # Data models
│   └── queries.rs    # Database queries
└── rabbitmq.rs       # RabbitMQ event consumer
```

## Configuration Reference

### JIT Receive Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `jit_receive_base_fee` | 1000 | Base service fee in satoshis |
| `jit_receive_fee_ppm` | 5000 | PPM fee on receive amount (5000 = 0.5%) |
| `jit_min_receive_amount` | 10000 | Minimum amount user can request (sats) |
| `jit_inbound_buffer` | 50000 | Extra inbound capacity added to channel (sats) |
| `fee_confirmation_target` | 6 | Confirmation target for fee estimation (blocks) |

**Fee Calculation:**
```
Total Fee = base_fee + (amount * ppm / 1,000,000) + (200 vbytes * fee_rate)
Invoice Amount = amount + Total Fee
Channel Capacity = amount + inbound_buffer
```

## Roadmap

- [x] Basic project structure
- [x] LDK Server integration (gRPC client)
- [x] Configuration system
- [x] Zeroconf channel support
- [x] Channel splicing support
- [x] **JIT Receive (JIT liquidity)**
- [x] **Dynamic fee estimation from Esplora**
- [x] **Inbound buffer for extra capacity**
- [x] RabbitMQ event consumption
- [ ] Admin dashboard (web UI)
- [ ] Advanced fee scheduling
- [ ] Liquidity management tools
- [ ] Metrics and monitoring

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) for details.

## License

This project is licensed under the MIT OR Apache-2.0 license.
