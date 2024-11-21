# Referral List - For Webhook Endpoint

If you don't know what this is, it isn't for you.

Admittedly, it isn't named the greatest, as what it is is referral_list FOR a webhook endpoint. It grabs referrals data, encrypts it, and sends it to an endpoint.

## General Use

Download the latest release. As of writing this README, that is 1.3.1. 
Setup a webhook endpoint to receive the data. It needs to be able to decrypt and decode given you personal CRYPT Key.
A premade one is copyable [here](https://docs.google.com/spreadsheets/d/1uco9REWJxfWBnhVo8tlFTNVxtohYbIcTsHc2Wd9A3p8/edit?usp=sharing) for @missionary.org email domain users. It comes with instructions for setup.

### Automatic updates

Use Windows Task Scheduler to create an automated task.
1. run referral_list_endpoint.exe (1.3.1 or later) and obtain a runcode.
2. Create a Task in Windows Task Scheduler with a trigger of your choice.
3. Set it to run a program, referral_list_endpoint.exe. Set its argument to the runcode.
4. It's all set up! You can test it by clicking "run" on the task.

### 

## Development/Advanced Use
### Building

Install Rust and cargo via [rustup](https://rustup.rs)

```bash
cargo build --release
```

### Google Apps Script Post Handler

To setup the Google Apps Script Handler, follow the instructions at the beginning of the file.

### Running

The program will set itself up. Either run the binary you built above or run
via cargo.

```bash
cargo run --release
```

### TODO

- [X] Send to an network endpoint (encrypted)
- [X] Add Endpoint Details
- [ ] Make the Endpoint not accept bad input

### Debugging

You can set the environment variable ``RUST_LOG`` to ``info`` to get more
detailed logs.
Set this either in your .env file or ``export`` it on Linux.

