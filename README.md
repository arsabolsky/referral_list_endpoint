# Referral List - For Webhook Endpoint

If you don't know what this is, it isn't for you.

## Building

Install Rust and cargo via [rustup](https://rustup.rs)

```bash
cargo build --release
```

## Google Apps Script Post Handler

To setup the Google Apps Script Handler, follow the instructions at the beginning of the file.

## Running

The program will set itself up. Either run the binary you built above or run
via cargo.

```bash
cargo run --release
```

## TODO

- [X] Send to an network endpoint (encrypted)
- [X] Add Endpoint Details
- [ ] Make the Endpoint not accept bad input

## Debugging

You can set the environment variable ``RUST_LOG`` to ``info`` to get more
detailed logs.
Set this either in your .env file or ``export`` it on Linux.

