# Debug Proxy

A high-performance HTTP debugging reverse proxy with timeout handling and subprocess management.

## Features

- **HTTP Reverse Proxy**: Forward requests to upstream services with configurable timeouts
- **Subprocess Management**: Automatically start, monitor, and restart upstream processes
- **Request/Response Logging**: Capture and inspect HTTP traffic with a web interface
- **Timeout Handling**: Configurable client and upstream timeouts
- **Auto-restart**: Automatically restart crashed subprocesses with warnings
- **Port Mapping**: Support both managed subprocesses and external services

## Installation

### From Source

```bash
cargo build --release
```

The binary will be available at `./target/release/debug-proxy`.

### Frontend Assets

The web interface assets are automatically embedded into the binary during compilation:

- If `ui/dist/` exists, assets are embedded from there
- If not, `build.rs` will automatically run `npm install` and `npm run build`
- The final binary is completely self-contained with no external dependencies

## Usage

### Basic Usage

```bash
# Proxy to external service
debug-proxy 192.168.1.1:3000

# Proxy to localhost service and listen on port 8080
debug-proxy localhost:3000 -p 8080

# Proxy with managed subprocess
debug-proxy localhost:3000 -p 8080 -- python -m http.server 3000

# Bind to specific host address
debug-proxy localhost:3000 -p 8080 --host 127.0.0.1
```

### Command Line Options

- `UPSTREAM`: Upstream target in format `host:port` (e.g., `192.168.1.1:3000`, `localhost:3000`)
- `--port, -p`: Local port to listen on (default: `8080`)
- `--host`: Host address to bind to (default: `0.0.0.0`)
- `--upstream-timeout, -u`: Upstream timeout in milliseconds (default: `500`)
- `--client-timeout, -c`: Client timeout in milliseconds (default: `30000`)
- `--max-history, -m`: Maximum number of requests to keep in history (default: `100`)
- `--truncate-body`: Body truncation size in bytes (default: `1024`)
- `[COMMAND]...`: Optional command to run as upstream service (use `--` before command)

### Web Interface

When the proxy starts, it provides a web interface for inspecting HTTP traffic:

```
🌐 Web Interface
URL: http://localhost:8080/_proxy?token=<access-token>
```

The web interface allows you to:
- View request/response history
- Inspect headers and body content
- Configure proxy settings

## LICENSE

[MIT](LICENSE)