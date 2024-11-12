# domain-checker

A simple command line tool to check if domain names are registered using DNS lookups.


## Installation

If you have Rust installed, you can install `domain-checker` using cargo:
```bash
cargo install domain-checker
```

Otherwise, binary releases will be available soon.


## Usage

Check a single domain:
```bash
domain-checker example.com
```

Check multiple domains:
```bash
domain-checker example.com example.org
```

Check domains from a file:
```bash
cat domains.txt | domain-checker
```

For help, run:
```bash
domain-checker --help
```

```bash
Check if domain names are registered using DNS lookups

Usage: domain-checker [OPTIONS] [DOMAINS]...

Arguments:
  [DOMAINS]...  Domain names to check (optional if reading from stdin)

Options:
  -c, --concurrent <CONCURRENT>    Maximum number of concurrent checks [default: 10]
  -j, --json                       Output as JSON to stdout
      --output-file <OUTPUT_FILE>  Save output to JSON file
  -t, --timestamp                  Include timestamp in output
  -c, --clean                      Strip whitespace and empty lines from input
  -h, --help                       Print help
  -V, --version                    Print version
```
